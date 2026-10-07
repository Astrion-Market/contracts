import { decodeEventLog, type Address, type Hex, type Log } from "viem";
import { astrionAccountAbi } from "./abi/generated";

export interface EventMeta {
  chainId: number;
  account: Address;
  blockNumber: bigint;
  blockHash: Hex;
  txHash: Hex;
  logIndex: number;
}

export type OperationEvent = EventMeta &
  (
    | { kind: "IntentExecuted"; nonce: bigint; module: Address; relayer: Address; fee: bigint; transferId: Hex }
    | { kind: "NonceRevoked"; nonce: bigint }
    | {
        kind: "TransferReceived";
        transferId: Hex;
        sourceDomain: number;
        cctpNonce: Hex;
        burnedAmount: bigint;
        feeExecuted: bigint;
        received: bigint;
      }
    | { kind: "FundedActionFailed"; transferId: Hex; nonce: bigint; reason: Hex }
  );

const TRACKED = new Set(["IntentExecuted", "NonceRevoked", "TransferReceived", "FundedActionFailed"]);

export function decodeAccountLogs(chainId: number, logs: Log[]): OperationEvent[] {
  const out: OperationEvent[] = [];
  for (const log of logs) {
    if (log.blockNumber === null || log.blockHash === null || log.transactionHash === null || log.logIndex === null) {
      continue;
    }
    let decoded;
    try {
      decoded = decodeEventLog({ abi: astrionAccountAbi, data: log.data, topics: log.topics });
    } catch {
      continue;
    }
    if (!TRACKED.has(decoded.eventName)) continue;
    const meta: EventMeta = {
      chainId,
      account: log.address,
      blockNumber: log.blockNumber,
      blockHash: log.blockHash,
      txHash: log.transactionHash,
      logIndex: log.logIndex,
    };
    const a = decoded.args as Record<string, unknown>;
    switch (decoded.eventName) {
      case "IntentExecuted":
        out.push({ ...meta, kind: "IntentExecuted", nonce: a.nonce as bigint, module: a.module as Address,
          relayer: a.relayer as Address, fee: a.fee as bigint, transferId: a.transferId as Hex });
        break;
      case "NonceRevoked":
        out.push({ ...meta, kind: "NonceRevoked", nonce: a.nonce as bigint });
        break;
      case "TransferReceived":
        out.push({ ...meta, kind: "TransferReceived", transferId: a.transferId as Hex,
          sourceDomain: Number(a.sourceDomain), cctpNonce: a.nonce as Hex, burnedAmount: a.burnedAmount as bigint,
          feeExecuted: a.feeExecuted as bigint, received: a.received as bigint });
        break;
      case "FundedActionFailed":
        out.push({ ...meta, kind: "FundedActionFailed", transferId: a.transferId as Hex,
          nonce: a.nonce as bigint, reason: a.reason as Hex });
        break;
    }
  }
  return out;
}

export type TransferStatus = "received" | "action_failed" | "consumed";

export interface TransferView {
  transferId: Hex;
  received: bigint;
  status: TransferStatus;
  sourceBlock: bigint;
}

const key = (e: EventMeta) => `${e.chainId}:${e.txHash}:${e.logIndex}`;
const order = (a: OperationEvent, b: OperationEvent) =>
  a.blockNumber === b.blockNumber ? a.logIndex - b.logIndex : a.blockNumber < b.blockNumber ? -1 : 1;

/**
 * Idempotent, order-independent event store. Re-ingesting a log is a no-op;
 * removing a reorged block hash drops its events; views are derived from the
 * canonical (block, logIndex) order every time.
 */
export class OperationStore {
  private readonly events = new Map<string, OperationEvent>();

  ingest(events: OperationEvent[]): number {
    let added = 0;
    for (const e of events) {
      if (!this.events.has(key(e))) {
        this.events.set(key(e), e);
        added++;
      }
    }
    return added;
  }

  dropBlock(blockHash: Hex): number {
    let removed = 0;
    for (const [k, e] of this.events) {
      if (e.blockHash === blockHash) {
        this.events.delete(k);
        removed++;
      }
    }
    return removed;
  }

  ordered(account?: Address): OperationEvent[] {
    return [...this.events.values()]
      .filter((e) => !account || e.account.toLowerCase() === account.toLowerCase())
      .sort(order);
  }

  transfers(account: Address): Map<Hex, TransferView> {
    const views = new Map<Hex, TransferView>();
    for (const e of this.ordered(account)) {
      if (e.kind === "TransferReceived") {
        views.set(e.transferId, { transferId: e.transferId, received: e.received, status: "received", sourceBlock: e.blockNumber });
      } else if (e.kind === "FundedActionFailed") {
        const v = views.get(e.transferId);
        if (v && v.status !== "consumed") v.status = "action_failed";
      } else if (e.kind === "IntentExecuted" && e.transferId !== `0x${"00".repeat(32)}`) {
        const v = views.get(e.transferId);
        if (v) v.status = "consumed";
      }
    }
    return views;
  }

  usedNonces(account: Address): Set<bigint> {
    const used = new Set<bigint>();
    for (const e of this.ordered(account)) {
      if (e.kind === "IntentExecuted" || e.kind === "NonceRevoked") used.add(e.nonce);
    }
    return used;
  }
}
