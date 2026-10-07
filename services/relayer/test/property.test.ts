import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Address, Hex } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { aaveAction, AaveKind, buildIntent, intentTypedData } from "@astrion/crosschain-sdk";
import { Policy } from "../src/policy";
import { Relayer } from "../src/relayer";
import { Store } from "../src/store";
import type { JobRequest } from "../src/types";
import { FakeChain } from "./fakeChain";

const owner = privateKeyToAccount("0x00000000000000000000000000000000000000000000000000000000000a11ce");
const ACCOUNT: Address = "0x00000000000000000000000000000000000ac0de";
const USDC: Address = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
const T0 = 1_791_360_000_000;
const SEEDS = Number(process.env.RELAYER_PROPERTY_SEEDS ?? 40);

function prng(seed: number) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13;
    s ^= s >>> 17;
    s ^= s << 5;
    return (s >>> 0) / 0xffffffff;
  };
}

let dir: string;
beforeEach(() => (dir = mkdtempSync(join(tmpdir(), "relayer-prop-"))));
afterEach(() => rmSync(dir, { recursive: true, force: true }));

async function request(chain: FakeChain, nonce: bigint): Promise<JobRequest> {
  const action = aaveAction(AaveKind.Supply, USDC, 1_000_000n + nonce);
  const intent = buildIntent({
    module: "0x1111111111111111111111111111111111111111",
    moduleCodeHash: `0x${"22".repeat(32)}`,
    target: "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5",
    action,
    recipient: owner.address,
    nonce,
    deadline: BigInt(T0 / 1000 + 86_400),
    feeToken: USDC,
  });
  const signature = await owner.signTypedData(intentTypedData(chain.chainId, ACCOUNT, intent));
  return { chainId: chain.chainId, account: ACCOUNT, intent, action, signature, fee: 0n };
}

test(`random restarts, reorgs, RPC failures and duplicate submits: one signed tx per job, all final (${SEEDS} seeds)`, async () => {
  for (let seed = 1; seed <= SEEDS; seed++) {
    const rand = prng(seed);
    const chain = new FakeChain();
    chain.owners.set(ACCOUNT.toLowerCase(), owner.address);
    const db = join(dir, `seed-${seed}.sqlite`);
    let clock = T0;
    const policy = new Policy({ jobsPerOwnerPerMinute: 1_000, ownerDailyBudgetWei: 10n ** 20n, globalDailyBudgetWei: 10n ** 21n });
    const boot = () => {
      const store = new Store(db);
      return { store, relayer: new Relayer(store, chain, policy, { confirmations: 3, maxAttempts: 5, backoffMs: 1, now: () => clock }) };
    };
    let { store, relayer } = boot();
    const requests = await Promise.all([1n, 2n, 3n, 4n].map((n) => request(chain, n)));
    const ids = new Set<Hex>();

    for (let step = 0; step < 120; step++) {
      const r = rand();
      clock += 1_000;
      if (r < 0.15) ids.add((await relayer.submit(requests[Math.floor(rand() * requests.length)]!)).job.id);
      else if (r < 0.35) await relayer.tick();
      else if (r < 0.55) chain.mine(1 + Math.floor(rand() * 2));
      else if (r < 0.62) {
        const head = await chain.blockNumber();
        if (head > 1n) chain.reorg(head - BigInt(Math.floor(rand() * 2)));
      } else if (r < 0.7) chain.failNextBroadcast = true;
      else if (r < 0.78) {
        store.close();
        ({ store, relayer } = boot());
      }
    }
    for (const req of requests) ids.add((await relayer.submit(req)).job.id);
    for (let i = 0; i < 30; i++) {
      clock += 1_000;
      await relayer.tick();
      chain.mine();
    }
    for (const id of ids) expect(store.getJob(id)?.state, `seed ${seed}`).toBe("final");
    expect(chain.signed, `seed ${seed}`).toBe(requests.length);
    expect(chain.uniqueRaw.size, `seed ${seed}`).toBe(requests.length);
    store.close();
  }
});
