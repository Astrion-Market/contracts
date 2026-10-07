import { Database } from "bun:sqlite";
import type { Address, Hex } from "viem";
import type { OperationEvent } from "@astrion/crosschain-sdk";
import type { Job, JobRequest, JobState } from "./types";

const json = (v: unknown) => JSON.stringify(v, (_, x) => (typeof x === "bigint" ? `${x}n` : x));
const parse = <T>(s: string): T =>
  JSON.parse(s, (_, x) => (typeof x === "string" && /^-?\d+n$/.test(x) ? BigInt(x.slice(0, -1)) : x));

/** Durable job, budget, event and cursor storage. Every write is a single transaction. */
export class Store {
  readonly db: Database;

  constructor(path: string) {
    this.db = new Database(path, { create: true });
    this.db.exec("PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;");
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS jobs (
        id TEXT PRIMARY KEY, request TEXT NOT NULL, owner TEXT NOT NULL, state TEXT NOT NULL,
        attempts INTEGER NOT NULL, next_attempt_at INTEGER NOT NULL, raw_tx TEXT, tx_hash TEXT,
        block_number TEXT, block_hash TEXT, last_error TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS jobs_state ON jobs(state, next_attempt_at);
      CREATE TABLE IF NOT EXISTS spend (owner TEXT NOT NULL, day TEXT NOT NULL, wei TEXT NOT NULL, PRIMARY KEY(owner, day));
      CREATE TABLE IF NOT EXISTS events (key TEXT PRIMARY KEY, account TEXT NOT NULL, block_number TEXT NOT NULL,
        block_hash TEXT NOT NULL, body TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS cursors (chain_id INTEGER PRIMARY KEY, block_number TEXT NOT NULL, block_hash TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS accounts (chain_id INTEGER NOT NULL, account TEXT NOT NULL, PRIMARY KEY(chain_id, account));
    `);
  }

  close() {
    this.db.close();
  }

  insertJob(id: Hex, request: JobRequest, owner: Address, now: number): { job: Job; created: boolean } {
    const existing = this.getJob(id);
    if (existing) return { job: existing, created: false };
    this.db
      .query(
        `INSERT INTO jobs (id, request, owner, state, attempts, next_attempt_at, created_at, updated_at)
         VALUES (?, ?, ?, 'validated', 0, ?, ?, ?)`,
      )
      .run(id, json(request), owner.toLowerCase(), now, now, now);
    return { job: this.getJob(id)!, created: true };
  }

  getJob(id: Hex): Job | null {
    const r = this.db.query("SELECT * FROM jobs WHERE id = ?").get(id) as Record<string, unknown> | null;
    return r ? this.toJob(r) : null;
  }

  jobsIn(states: JobState[], now: number): Job[] {
    const placeholders = states.map(() => "?").join(",");
    return (
      this.db
        .query(`SELECT * FROM jobs WHERE state IN (${placeholders}) AND next_attempt_at <= ? ORDER BY created_at`)
        .all(...states, now) as Record<string, unknown>[]
    ).map((r) => this.toJob(r));
  }

  update(id: Hex, patch: Partial<Omit<Job, "id" | "request" | "owner">>, now: number) {
    const cols: Record<string, unknown> = {
      state: patch.state,
      attempts: patch.attempts,
      next_attempt_at: patch.nextAttemptAt,
      raw_tx: patch.rawTx,
      tx_hash: patch.txHash,
      block_number: patch.blockNumber === undefined ? undefined : patch.blockNumber?.toString() ?? null,
      block_hash: patch.blockHash,
      last_error: patch.lastError,
    };
    const set = Object.entries(cols).filter(([, v]) => v !== undefined);
    const sql = `UPDATE jobs SET ${set.map(([k]) => `${k} = ?`).join(", ")}, updated_at = ? WHERE id = ?`;
    this.db.query(sql).run(...(set.map(([, v]) => v) as never[]), now, id);
  }

  spentToday(owner: Address, day: string): bigint {
    const r = this.db.query("SELECT wei FROM spend WHERE owner = ? AND day = ?").get(owner.toLowerCase(), day) as
      | { wei: string }
      | null;
    return r ? BigInt(r.wei) : 0n;
  }

  totalSpent(day: string): bigint {
    const rows = this.db.query("SELECT wei FROM spend WHERE day = ?").all(day) as { wei: string }[];
    return rows.reduce((a, r) => a + BigInt(r.wei), 0n);
  }

  addSpend(owner: Address, day: string, wei: bigint) {
    const next = this.spentToday(owner, day) + wei;
    this.db
      .query("INSERT INTO spend (owner, day, wei) VALUES (?, ?, ?) ON CONFLICT(owner, day) DO UPDATE SET wei = excluded.wei")
      .run(owner.toLowerCase(), day, next.toString());
  }

  watch(chainId: number, account: Address) {
    this.db.query("INSERT OR IGNORE INTO accounts (chain_id, account) VALUES (?, ?)").run(chainId, account.toLowerCase());
  }

  watched(chainId: number): Address[] {
    return (this.db.query("SELECT account FROM accounts WHERE chain_id = ?").all(chainId) as { account: Address }[]).map(
      (r) => r.account,
    );
  }

  insertEvents(events: OperationEvent[]): number {
    const stmt = this.db.query(
      "INSERT OR IGNORE INTO events (key, account, block_number, block_hash, body) VALUES (?, ?, ?, ?, ?)",
    );
    let added = 0;
    this.db.transaction(() => {
      for (const e of events) {
        const r = stmt.run(`${e.chainId}:${e.txHash}:${e.logIndex}`, e.account.toLowerCase(), e.blockNumber.toString(), e.blockHash, json(e));
        added += r.changes;
      }
    })();
    return added;
  }

  dropEventsFrom(blockNumber: bigint) {
    this.db.query("DELETE FROM events WHERE CAST(block_number AS INTEGER) >= ?").run(Number(blockNumber));
  }

  events(account?: Address): OperationEvent[] {
    const rows = (account
      ? this.db.query("SELECT body FROM events WHERE account = ?").all(account.toLowerCase())
      : this.db.query("SELECT body FROM events").all()) as { body: string }[];
    return rows.map((r) => parse<OperationEvent>(r.body));
  }

  cursor(chainId: number): { blockNumber: bigint; blockHash: Hex } | null {
    const r = this.db.query("SELECT * FROM cursors WHERE chain_id = ?").get(chainId) as
      | { block_number: string; block_hash: Hex }
      | null;
    return r ? { blockNumber: BigInt(r.block_number), blockHash: r.block_hash } : null;
  }

  setCursor(chainId: number, blockNumber: bigint, blockHash: Hex) {
    this.db
      .query(
        "INSERT INTO cursors (chain_id, block_number, block_hash) VALUES (?, ?, ?) ON CONFLICT(chain_id) DO UPDATE SET block_number = excluded.block_number, block_hash = excluded.block_hash",
      )
      .run(chainId, blockNumber.toString(), blockHash);
  }

  private toJob(r: Record<string, unknown>): Job {
    return {
      id: r.id as Hex,
      request: parse<JobRequest>(r.request as string),
      owner: r.owner as Address,
      state: r.state as JobState,
      attempts: r.attempts as number,
      nextAttemptAt: r.next_attempt_at as number,
      rawTx: (r.raw_tx as Hex | null) ?? null,
      txHash: (r.tx_hash as Hex | null) ?? null,
      blockNumber: r.block_number ? BigInt(r.block_number as string) : null,
      blockHash: (r.block_hash as Hex | null) ?? null,
      lastError: (r.last_error as string | null) ?? null,
      createdAt: r.created_at as number,
      updatedAt: r.updated_at as number,
    };
  }
}
