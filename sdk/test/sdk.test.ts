import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  decodeAbiParameters,
  encodeAbiParameters,
  encodeEventTopics,
  keccak256,
  maxUint256,
  type Address,
  type Hex,
  type Log,
} from "viem";
import {
  AaveKind,
  aaveAction,
  aaveMarketScope,
  accountSalt,
  astrionAccountAbi,
  buildIntent,
  canPerform,
  CompoundKind,
  compoundAction,
  decodeAccountLogs,
  intentDigest,
  morphoAction,
  MorphoKind,
  OperationStore,
  PROTOCOL_IDS,
  protocolFromId,
  returnToStellarAction,
  routeAvailability,
  transferId,
  type Manifest,
} from "../src";

const root = join(import.meta.dir, "../..");
const json = (p: string) => JSON.parse(readFileSync(join(root, p), "utf8"));
const G = "GAUHMCMUP5FZO5675W3ISZ6E6CNYJGXBUW5WANE2JR4TGAARYCTSCBKI";
const USDC: Address = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";

describe("actions", () => {
  test("encode in the module's own layout", () => {
    const a = aaveAction(AaveKind.RepayAll, USDC, 0n);
    expect(decodeAbiParameters([{ type: "uint8" }, { type: "address" }, { type: "uint256" }], a)).toEqual([
      5,
      USDC,
      0n,
    ]);
    const m = morphoAction(MorphoKind.Withdraw, maxUint256);
    expect(decodeAbiParameters([{ type: "uint8" }, { type: "uint256" }], m)).toEqual([2, maxUint256]);
    expect(compoundAction(CompoundKind.RepayAvailable, 0n)).toBe(
      encodeAbiParameters([{ type: "uint8" }, { type: "uint256" }], [5, 0n]),
    );
  });

  test("return action validates the Stellar recipient before signing", () => {
    const r = returnToStellarAction({ amount: 100n, maxFee: 1n, minFinality: 2000, recipient: G });
    const [amount, , finality, recipient] = decodeAbiParameters(
      [{ type: "uint256" }, { type: "uint256" }, { type: "uint32" }, { type: "string" }],
      r,
    );
    expect([amount, finality, recipient]).toEqual([100n, 2000, G]);
    expect(() => returnToStellarAction({ amount: 100n, maxFee: 0n, minFinality: 2000, recipient: G.slice(0, 55) + "A" })).toThrow();
    expect(() => returnToStellarAction({ amount: 100n, maxFee: 100n, minFinality: 2000, recipient: G })).toThrow();
  });

  test("market scopes", () => {
    expect(aaveMarketScope(USDC)).toBe(`0x000000000000000000000000${USDC.slice(2)}` as Hex);
    expect(protocolFromId(PROTOCOL_IDS["morpho-blue"])).toBe("morpho-blue");
  });
});

describe("intents", () => {
  test("digest matches the Solidity-verified vector", () => {
    const v = json("spec/fixtures/evm/intent-digest.json");
    const i = { ...v.intent, maxFee: BigInt(v.intent.maxFee), nonce: BigInt(v.intent.nonce), deadline: BigInt(v.intent.deadline) };
    expect(intentDigest(v.chainId, v.account, i)).toBe(v.digest);
  });

  test("buildIntent binds the action hash and defaults", () => {
    const action = aaveAction(AaveKind.Supply, USDC, 1_000_000n);
    const i = buildIntent({
      module: "0x1111111111111111111111111111111111111111",
      moduleCodeHash: `0x${"22".repeat(32)}`,
      target: "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5",
      action,
      recipient: "0x4444444444444444444444444444444444444444",
      nonce: 1n,
      deadline: 2n,
      feeToken: USDC,
    });
    expect(i.actionHash).toBe(keccak256(action));
    expect(i.transferId).toBe(`0x${"00".repeat(32)}`);
    expect(() => buildIntent({ ...i, action, recipient: "0x0000000000000000000000000000000000000000" })).toThrow();
  });

  test("transfer id and salt are deterministic", () => {
    expect(transferId(27, `0x${"01".repeat(32)}`)).toBe(
      keccak256(encodeAbiParameters([{ type: "uint32" }, { type: "bytes32" }], [27, `0x${"01".repeat(32)}`])),
    );
    const scope = { owner: USDC, chainId: 8453n, protocolId: PROTOCOL_IDS["aave-v3"], marketScope: aaveMarketScope(USDC), version: 1 };
    expect(accountSalt(scope)).toBe(accountSalt({ ...scope }));
    expect(accountSalt(scope)).not.toBe(accountSalt({ ...scope, chainId: 1n }));
  });
});

describe("capabilities", () => {
  test("every route is visible and none is usable before verification", () => {
    for (const env of ["mainnet", "testnet"]) {
      const m = json(`deployments/crosschain/${env}.json`) as Manifest;
      const routes = routeAvailability(m);
      expect(routes).toHaveLength(6);
      for (const r of routes) {
        expect(r.usable).toBe(false);
        expect(r.reason?.length).toBeGreaterThan(0);
      }
      expect(canPerform(m, routes[0]!.id, "lend")).toBe(false);
    }
  });
});

describe("events", () => {
  const account: Address = "0x00000000000000000000000000000000000a11ce";
  const tid = transferId(27, `0x${"01".repeat(32)}`);

  function log(eventName: string, args: Record<string, unknown>, data: Hex, block: bigint, index: number, hash: Hex): Log {
    const topics = encodeEventTopics({ abi: astrionAccountAbi, eventName, args } as never);
    return {
      address: account,
      topics: topics as [Hex, ...Hex[]],
      data,
      blockNumber: block,
      blockHash: hash,
      transactionHash: keccak256(`0x${block.toString(16).padStart(2, "0")}${index.toString(16).padStart(2, "0")}`),
      logIndex: index,
      transactionIndex: 0,
      removed: false,
    } as Log;
  }

  const received = log(
    "TransferReceived",
    { transferId: tid },
    encodeAbiParameters(
      [{ type: "uint32" }, { type: "bytes32" }, { type: "uint256" }, { type: "uint256" }, { type: "uint256" }],
      [27, `0x${"01".repeat(32)}`, 1_000_000n, 100n, 999_900n],
    ),
    10n, 0, `0x${"aa".repeat(32)}`,
  );
  const failed = log(
    "FundedActionFailed",
    { transferId: tid, nonce: 1n },
    encodeAbiParameters([{ type: "bytes" }], ["0x1234"]),
    10n, 1, `0x${"aa".repeat(32)}`,
  );
  const executed = log(
    "IntentExecuted",
    { nonce: 2n, module: "0x1111111111111111111111111111111111111111" },
    encodeAbiParameters(
      [{ type: "address" }, { type: "uint256" }, { type: "bytes32" }],
      ["0x9999999999999999999999999999999999999999", 0n, tid],
    ),
    12n, 3, `0x${"bb".repeat(32)}`,
  );

  test("repeated and out-of-order ingestion yields the same state", () => {
    const a = new OperationStore();
    a.ingest(decodeAccountLogs(8453, [received, failed, executed]));
    const b = new OperationStore();
    b.ingest(decodeAccountLogs(8453, [executed, received]));
    b.ingest(decodeAccountLogs(8453, [failed, received, executed]));
    expect(b.ingest(decodeAccountLogs(8453, [received]))).toBe(0);
    expect([...a.transfers(account).values()]).toEqual([...b.transfers(account).values()]);
    expect(a.transfers(account).get(tid)?.status).toBe("consumed");
    expect([...a.usedNonces(account)]).toEqual([2n]);
  });

  test("a reorged block is dropped and views recompute", () => {
    const s = new OperationStore();
    s.ingest(decodeAccountLogs(8453, [received, failed, executed]));
    expect(s.dropBlock(`0x${"bb".repeat(32)}`)).toBe(1);
    expect(s.transfers(account).get(tid)?.status).toBe("action_failed");
    expect(s.transfers(account).get(tid)?.received).toBe(999_900n);
  });
});
