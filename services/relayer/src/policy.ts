import type { Address } from "viem";
import type { Store } from "./store";
import { RejectedError } from "./validate";

export interface PolicyConfig {
  jobsPerOwnerPerMinute: number;
  ownerDailyBudgetWei: bigint;
  globalDailyBudgetWei: bigint;
}

export const dayOf = (nowMs: number) => new Date(nowMs).toISOString().slice(0, 10);

/** Per-owner token bucket plus per-owner and global daily gas budgets. */
export class Policy {
  private readonly buckets = new Map<string, { tokens: number; at: number }>();

  constructor(readonly config: PolicyConfig) {}

  admit(owner: Address, nowMs: number) {
    const key = owner.toLowerCase();
    const rate = this.config.jobsPerOwnerPerMinute;
    const b = this.buckets.get(key) ?? { tokens: rate, at: nowMs };
    b.tokens = Math.min(rate, b.tokens + ((nowMs - b.at) / 60_000) * rate);
    b.at = nowMs;
    if (b.tokens < 1) throw new RejectedError("RateLimited");
    b.tokens -= 1;
    this.buckets.set(key, b);
  }

  chargeable(store: Store, owner: Address, costWei: bigint, nowMs: number): boolean {
    const day = dayOf(nowMs);
    return (
      store.spentToday(owner, day) + costWei <= this.config.ownerDailyBudgetWei &&
      store.totalSpent(day) + costWei <= this.config.globalDailyBudgetWei
    );
  }
}
