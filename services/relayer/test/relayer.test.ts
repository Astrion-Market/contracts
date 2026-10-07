import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { encodeAbiParameters, encodeEventTopics, keccak256, toHex, zeroAddress, zeroHash, type Address, type Hex, type Log } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { aaveAction, AaveKind, astrionAccountAbi, buildIntent, intentTypedData, transferId } from "@astrion/crosschain-sdk";
import { createApi } from "../src/api";
import { Indexer } from "../src/indexer";
import { Policy } from "../src/policy";
import { Relayer } from "../src/relayer";
import { Store } from "../src/store";
import type { JobRequest } from "../src/types";
import { FakeChain } from "./fakeChain";

const owner = privateKeyToAccount("0x00000000000000000000000000000000000000000000000000000000000a11ce");
const stranger = privateKeyToAccount("0x0000000000000000000000000000000000000000000000000000000000000bad");
const ACCOUNT: Address = "0x00000000000000000000000000000000000ac0de";
const USDC: Address = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
const T0 = 1_791_360_000_000;

let dir: string;
let dbPath: string;
let chain: FakeChain;
let clock: number;

const policyConfig = { jobsPerOwnerPerMinute: 3, ownerDailyBudgetWei: 10n ** 16n, globalDailyBudgetWei: 10n ** 17n };
const opts = () => ({ confirmations: 3, maxAttempts: 3, backoffMs: 1_000, now: () => clock });

function boot() {
  const store = new Store(dbPath);
  return { store, relayer: new Relayer(store, chain, new Policy(policyConfig), opts()) };
}

async function job(nonce: bigint, signer = owner): Promise<JobRequest> {
  const action = aaveAction(AaveKind.Supply, USDC, 1_000_000n);
  const intent = buildIntent({
    module: "0x1111111111111111111111111111111111111111",
    moduleCodeHash: `0x${"22".repeat(32)}`,
    target: "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5",
    action,
    recipient: owner.address,
    nonce,
    deadline: BigInt(T0 / 1000 + 3600),
    feeToken: USDC,
    maxFee: 10_000n,
  });
  const signature = await signer.signTypedData(intentTypedData(chain.chainId, ACCOUNT, intent));
  return { chainId: chain.chainId, account: ACCOUNT, intent, action, signature, fee: 5_000n };
}

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "relayer-"));
  dbPath = join(dir, "db.sqlite");
  chain = new FakeChain();
  chain.owners.set(ACCOUNT.toLowerCase(), owner.address);
  clock = T0;
});

afterEach(() => rmSync(dir, { recursive: true, force: true }));

describe("admission spends nothing on bad jobs", () => {
  test("forged, expired, over-fee, wrong-chain and tampered jobs are rejected", async () => {
    const { relayer } = boot();
    await expect(relayer.submit(await job(1n, stranger))).rejects.toThrow("InvalidSignature");
    const expired = await job(2n);
    clock = T0 + 7_200_000;
    await expect(relayer.submit(expired)).rejects.toThrow("IntentExpired");
    clock = T0;
    await expect(relayer.submit({ ...(await job(3n)), fee: 20_000n })).rejects.toThrow("FeeAboveCap");
    await expect(relayer.submit({ ...(await job(4n)), chainId: 1 })).rejects.toThrow("WrongChain");
    await expect(relayer.submit({ ...(await job(5n)), action: aaveAction(AaveKind.Borrow, USDC, 1n) })).rejects.toThrow(
      "ActionHashMismatch",
    );
    const funded = await job(6n);
    await expect(relayer.submit({ ...funded, funded: { message: "0x", attestation: "0x" } })).rejects.toThrow(
      "TransferBindingMismatch",
    );
    expect(chain.signed).toBe(0);
  });

  test("resubmitting the same intent is idempotent", async () => {
    const { relayer } = boot();
    const req = await job(1n);
    const a = await relayer.submit(req);
    const b = await relayer.submit(req);
    expect(a.created).toBe(true);
    expect(b.created).toBe(false);
    expect(b.job.id).toBe(a.job.id);
  });

  test("per-owner rate limit", async () => {
    const { relayer } = boot();
    for (const n of [1n, 2n, 3n]) await relayer.submit(await job(n));
    await expect(relayer.submit(await job(4n))).rejects.toThrow("RateLimited");
    clock += 60_000;
    await relayer.submit(await job(4n));
  });
});

describe("lifecycle", () => {
  test("submit -> confirm -> final with one signed transaction", async () => {
    const { relayer, store } = boot();
    const { job: j } = await relayer.submit(await job(1n));
    await relayer.tick();
    expect(store.getJob(j.id)?.state).toBe("submitted");
    chain.mine();
    await relayer.tick();
    expect(store.getJob(j.id)?.state).toBe("confirmed");
    chain.mine(2);
    await relayer.tick();
    expect(store.getJob(j.id)?.state).toBe("final");
    expect(chain.signed).toBe(1);
  });

  test("restart after every boundary never double-sends", async () => {
    let { relayer, store } = boot();
    const { job: j } = await relayer.submit(await job(1n));
    store.close();

    ({ relayer, store } = boot());
    chain.failNextBroadcast = true;
    await relayer.tick();
    expect(store.getJob(j.id)?.state).toBe("submitted");
    expect(store.getJob(j.id)?.rawTx).not.toBeNull();
    store.close();

    ({ relayer, store } = boot());
    await relayer.tick();
    chain.mine();
    store.close();

    ({ relayer, store } = boot());
    await relayer.tick();
    expect(store.getJob(j.id)?.state).toBe("confirmed");
    store.close();

    ({ relayer, store } = boot());
    chain.mine(2);
    await relayer.tick();
    expect(store.getJob(j.id)?.state).toBe("final");
    expect(chain.signed).toBe(1);
    expect(chain.uniqueRaw.size).toBe(1);
  });

  test("a reorg re-broadcasts the same transaction", async () => {
    const { relayer, store } = boot();
    const { job: j } = await relayer.submit(await job(1n));
    await relayer.tick();
    chain.mine();
    await relayer.tick();
    const minedAt = store.getJob(j.id)!.blockNumber!;
    chain.reorg(minedAt);
    await relayer.tick();
    expect(store.getJob(j.id)?.state).toBe("submitted");
    await relayer.tick();
    chain.mine(3);
    await relayer.tick();
    await relayer.tick();
    expect(store.getJob(j.id)?.state).toBe("final");
    expect(chain.signed).toBe(1);
    expect(chain.uniqueRaw.size).toBe(1);
  });

  test("a reverted transaction is final, never resubmitted", async () => {
    const { relayer, store } = boot();
    chain.revertAll = true;
    const { job: j } = await relayer.submit(await job(1n));
    await relayer.tick();
    chain.mine();
    await relayer.tick();
    await relayer.tick();
    expect(store.getJob(j.id)?.state).toBe("failed_final");
    expect(chain.signed).toBe(1);
  });
});

describe("sponsorship cannot be exhausted", () => {
  test("failing simulation never signs and backs off to failed_final", async () => {
    const { relayer, store } = boot();
    chain.sim = { ok: false, gas: 0n, error: "RoutePaused" };
    const { job: j } = await relayer.submit(await job(1n));
    for (let i = 0; i < 5; i++) {
      await relayer.tick();
      clock += 10_000;
    }
    expect(store.getJob(j.id)?.state).toBe("failed_final");
    expect(store.getJob(j.id)?.lastError).toBe("RoutePaused");
    expect(chain.signed).toBe(0);
  });

  test("per-owner daily budget caps spend", async () => {
    const { relayer, store } = boot();
    chain.sim = { ok: true, gas: 4_000_000n };
    const ids: Hex[] = [];
    for (const n of [1n, 2n, 3n]) ids.push((await relayer.submit(await job(n))).job.id);
    await relayer.tick();
    const states = ids.map((id) => store.getJob(id)?.state);
    expect(states.filter((s) => s === "submitted")).toHaveLength(2);
    expect(states.filter((s) => s === "failed_final")).toHaveLength(1);
    expect(chain.signed).toBe(2);
  });
});

describe("indexer", () => {
  const tid = transferId(27, `0x${"01".repeat(32)}`);

  function receivedLog(block: bigint, blockHash: Hex): Log {
    return {
      address: ACCOUNT,
      topics: encodeEventTopics({ abi: astrionAccountAbi, eventName: "TransferReceived", args: { transferId: tid } }) as [Hex],
      data: encodeAbiParameters(
        [{ type: "uint32" }, { type: "bytes32" }, { type: "uint256" }, { type: "uint256" }, { type: "uint256" }],
        [27, `0x${"01".repeat(32)}`, 1_000_000n, 0n, 1_000_000n],
      ),
      blockNumber: block,
      blockHash,
      transactionHash: keccak256(toHex(`tx${block}`)),
      logIndex: 0,
      transactionIndex: 0,
      removed: false,
    } as Log;
  }

  test("ingestion is idempotent and survives reorgs", async () => {
    const store = new Store(dbPath);
    store.watch(chain.chainId, ACCOUNT);
    const indexer = new Indexer(store, chain, { startBlock: 0n, batchSize: 100n, reorgDepth: 5n });
    chain.mine(3);
    chain.logs.push(receivedLog(2n, (await chain.blockHash(2n))!));
    expect(await indexer.tick()).toBe(1);
    expect(await indexer.tick()).toBe(0);
    expect(indexer.operations(ACCOUNT).transfers).toHaveLength(1);

    chain.reorg(2n);
    chain.mine(2);
    expect(await indexer.tick()).toBe(0);
    expect(indexer.operations(ACCOUNT).transfers).toHaveLength(0);

    chain.mine();
    const head = chain.head();
    chain.logs.push(receivedLog(head.number, head.hash));
    expect(await indexer.tick()).toBe(1);
    expect(indexer.operations(ACCOUNT).transfers[0]?.sourceBlock).toBe(head.number);
  });
});

describe("api", () => {
  test("POST is idempotent; GET reports state", async () => {
    const { relayer, store } = boot();
    const api = createApi(relayer, new Indexer(store, chain, { startBlock: 0n, batchSize: 10n, reorgDepth: 5n }));
    const req = await job(1n);
    const body = JSON.stringify(req, (_, v) => (typeof v === "bigint" ? v.toString() : v));
    const post = () => api(new Request("http://x/v1/jobs", { method: "POST", body }));
    const first = await post();
    const second = await post();
    expect(first.status).toBe(201);
    expect(second.status).toBe(200);
    const { id } = (await first.json()) as { id: Hex };
    const status = await api(new Request(`http://x/v1/jobs/${id}`));
    expect(((await status.json()) as { state: string }).state).toBe("validated");
    const forged = JSON.stringify(await job(2n, stranger), (_, v) => (typeof v === "bigint" ? v.toString() : v));
    expect((await api(new Request("http://x/v1/jobs", { method: "POST", body: forged }))).status).toBe(422);
    expect(zeroAddress).toBeDefined();
    expect(zeroHash).toBeDefined();
  });
});
