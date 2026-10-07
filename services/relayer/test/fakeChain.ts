import { keccak256, toHex, type Address, type Hex, type Log } from "viem";
import type { ChainAdapter, JobRequest, Receipt, Simulation } from "../src/types";

/** Deterministic in-memory chain with mempool, mining, reorgs and logs. */
export class FakeChain implements ChainAdapter {
  chainId = 8453;
  relayerAddress: Address = "0x00000000000000000000000000000000000000e1";
  owners = new Map<string, Address>();
  sim: Simulation = { ok: true, gas: 100_000n };
  gasPrice = 1_000_000_000n;
  failNextBroadcast = false;
  revertAll = false;

  signed = 0;
  broadcasts = 0;
  uniqueRaw = new Set<Hex>();
  private mempool = new Set<Hex>();
  private mined = new Map<Hex, bigint>();
  private blocks: Hex[] = [keccak256(toHex("genesis"))];
  private fork = 0;
  logs: Log[] = [];

  async ownerOf(account: Address) {
    const o = this.owners.get(account.toLowerCase());
    if (!o) throw new Error("no account");
    return o;
  }

  async simulate(_: JobRequest) {
    return this.sim;
  }

  async gasPriceWei() {
    return this.gasPrice;
  }

  async signTransaction(job: JobRequest, gas: bigint) {
    this.signed++;
    return toHex(`tx:${job.account}:${job.intent.nonce}:${gas}:${this.signed}`);
  }

  async broadcast(raw: Hex) {
    this.broadcasts++;
    if (this.failNextBroadcast) {
      this.failNextBroadcast = false;
      throw new Error("rpc down");
    }
    this.uniqueRaw.add(raw);
    const hash = keccak256(raw);
    if (!this.mined.has(hash)) this.mempool.add(hash);
    return hash;
  }

  mine(n = 1) {
    for (let i = 0; i < n; i++) {
      this.blocks.push(this.hashFor(BigInt(this.blocks.length)));
      const block = BigInt(this.blocks.length - 1);
      for (const h of this.mempool) this.mined.set(h, block);
      this.mempool.clear();
    }
  }

  reorg(fromBlock: bigint) {
    this.fork++;
    for (let n = Number(fromBlock); n < this.blocks.length; n++) this.blocks[n] = this.hashFor(BigInt(n));
    for (const [h, b] of this.mined) if (b >= fromBlock) this.mined.delete(h);
    this.logs = this.logs.filter((l) => (l.blockNumber as bigint) < fromBlock);
  }

  async receipt(hash: Hex): Promise<Receipt | null> {
    const b = this.mined.get(hash);
    if (b === undefined) return null;
    return { status: this.revertAll ? "reverted" : "success", blockNumber: b, blockHash: this.blocks[Number(b)]! };
  }

  async blockNumber() {
    return BigInt(this.blocks.length - 1);
  }

  async blockHash(n: bigint) {
    return this.blocks[Number(n)] ?? null;
  }

  head(): { number: bigint; hash: Hex } {
    return { number: BigInt(this.blocks.length - 1), hash: this.blocks[this.blocks.length - 1]! };
  }

  async accountLogs(accounts: Address[], from: bigint, to: bigint) {
    const set = new Set(accounts.map((a) => a.toLowerCase()));
    return this.logs.filter(
      (l) => set.has(l.address.toLowerCase()) && (l.blockNumber as bigint) >= from && (l.blockNumber as bigint) <= to,
    );
  }

  private hashFor(n: bigint): Hex {
    return keccak256(toHex(`block:${n}:fork:${this.fork}`));
  }
}
