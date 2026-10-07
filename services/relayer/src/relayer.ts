import { keccak256, type Hex } from "viem";
import type { Store } from "./store";
import { dayOf, type Policy } from "./policy";
import type { ChainAdapter, Job, JobRequest } from "./types";
import { validateRequest } from "./validate";

export interface RelayerOptions {
  confirmations: number;
  maxAttempts: number;
  backoffMs: number;
  now: () => number;
}

/**
 * Resumable submission state machine. Every transition is persisted before
 * the next side effect, and a signed raw transaction is stored BEFORE it is
 * broadcast, so a restart re-broadcasts the same transaction (same EVM
 * nonce) rather than creating a second one.
 *
 *   validated -> submitted -> confirmed -> final
 *        \-> failed_retryable (backoff) -> ... -> failed_final
 */
export class Relayer {
  constructor(
    readonly store: Store,
    readonly chain: ChainAdapter,
    readonly policy: Policy,
    readonly opts: RelayerOptions,
  ) {}

  async submit(req: JobRequest): Promise<{ job: Job; created: boolean }> {
    const now = this.opts.now();
    const { id, owner } = await validateRequest(req, this.chain, BigInt(Math.floor(now / 1000)));
    const existing = this.store.getJob(id);
    if (existing) return { job: existing, created: false };
    this.policy.admit(owner, now);
    this.store.watch(req.chainId, req.account);
    return this.store.insertJob(id, req, owner, now);
  }

  async tick(): Promise<void> {
    const now = this.opts.now();
    for (const job of this.store.jobsIn(["validated", "failed_retryable"], now)) await this.send(job);
    for (const job of this.store.jobsIn(["submitted"], now)) await this.track(job);
    for (const job of this.store.jobsIn(["confirmed"], now)) await this.finalize(job);
  }

  private async send(job: Job) {
    const now = this.opts.now();
    if (job.rawTx) {
      this.store.update(job.id, { state: "submitted" }, now);
      await this.chain.broadcast(job.rawTx).catch(() => undefined);
      return;
    }
    if (job.request.intent.deadline <= BigInt(Math.floor(now / 1000))) {
      this.store.update(job.id, { state: "failed_final", lastError: "IntentExpired" }, now);
      return;
    }
    const sim = await this.chain.simulate(job.request).catch((e: Error) => ({ ok: false, gas: 0n, error: e.message }));
    if (!sim.ok) return this.retryOrFail(job, sim.error ?? "SimulationFailed");

    const cost = sim.gas * (await this.chain.gasPriceWei());
    if (!this.policy.chargeable(this.store, job.owner, cost, now)) {
      this.store.update(job.id, { state: "failed_final", lastError: "BudgetExhausted" }, now);
      return;
    }
    const rawTx = await this.chain.signTransaction(job.request, sim.gas);
    const txHash = keccak256(rawTx);
    this.store.db.transaction(() => {
      this.store.update(job.id, { state: "submitted", rawTx, txHash, attempts: job.attempts + 1 }, now);
      this.store.addSpend(job.owner, dayOf(now), cost);
    })();
    await this.chain.broadcast(rawTx).catch(() => undefined);
  }

  private async track(job: Job) {
    const now = this.opts.now();
    const receipt = await this.chain.receipt(job.txHash as Hex);
    if (!receipt) {
      await this.chain.broadcast(job.rawTx as Hex).catch(() => undefined);
      return;
    }
    if (receipt.status === "reverted") {
      this.store.update(job.id, { state: "failed_final", lastError: "Reverted", blockNumber: receipt.blockNumber, blockHash: receipt.blockHash }, now);
      return;
    }
    this.store.update(job.id, { state: "confirmed", blockNumber: receipt.blockNumber, blockHash: receipt.blockHash }, now);
  }

  private async finalize(job: Job) {
    const now = this.opts.now();
    const canonical = await this.chain.blockHash(job.blockNumber as bigint);
    if (canonical !== job.blockHash) {
      this.store.update(job.id, { state: "submitted", blockNumber: null, blockHash: null, lastError: "Reorged" }, now);
      return;
    }
    const head = await this.chain.blockNumber();
    if (head - (job.blockNumber as bigint) + 1n >= BigInt(this.opts.confirmations)) {
      this.store.update(job.id, { state: "final" }, now);
    }
  }

  private retryOrFail(job: Job, error: string) {
    const now = this.opts.now();
    const attempts = job.attempts + 1;
    if (attempts >= this.opts.maxAttempts) {
      this.store.update(job.id, { state: "failed_final", attempts, lastError: error }, now);
    } else {
      this.store.update(
        job.id,
        { state: "failed_retryable", attempts, lastError: error, nextAttemptAt: now + this.opts.backoffMs * 2 ** (attempts - 1) },
        now,
      );
    }
  }
}
