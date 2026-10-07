import { OperationStore, decodeAccountLogs } from "@astrion/crosschain-sdk";
import type { Address } from "viem";
import type { Store } from "./store";
import type { ChainAdapter } from "./types";

export interface IndexerOptions {
  startBlock: bigint;
  batchSize: bigint;
  reorgDepth: bigint;
}

/** Polls account logs into durable, de-duplicated events; rewinds on reorg. */
export class Indexer {
  constructor(
    readonly store: Store,
    readonly chain: ChainAdapter,
    readonly opts: IndexerOptions,
  ) {}

  async tick(): Promise<number> {
    const id = this.chain.chainId;
    let cursor = this.store.cursor(id);
    if (cursor) {
      const canonical = await this.chain.blockHash(cursor.blockNumber);
      if (canonical !== cursor.blockHash) {
        const from = cursor.blockNumber > this.opts.reorgDepth ? cursor.blockNumber - this.opts.reorgDepth : 0n;
        this.store.dropEventsFrom(from);
        const hash = from > 0n ? await this.chain.blockHash(from - 1n) : null;
        if (hash) this.store.setCursor(id, from - 1n, hash);
        cursor = hash ? { blockNumber: from - 1n, blockHash: hash } : null;
      }
    }
    const head = await this.chain.blockNumber();
    const from = cursor ? cursor.blockNumber + 1n : this.opts.startBlock;
    if (from > head) return 0;
    const to = from + this.opts.batchSize - 1n < head ? from + this.opts.batchSize - 1n : head;
    const accounts = this.store.watched(id);
    const added = accounts.length
      ? this.store.insertEvents(decodeAccountLogs(id, await this.chain.accountLogs(accounts, from, to)))
      : 0;
    const toHash = await this.chain.blockHash(to);
    if (toHash) this.store.setCursor(id, to, toHash);
    return added;
  }

  operations(account: Address) {
    const s = new OperationStore();
    s.ingest(this.store.events(account));
    return {
      transfers: [...s.transfers(account).values()],
      usedNonces: [...s.usedNonces(account)],
      events: s.ordered(account),
    };
  }
}
