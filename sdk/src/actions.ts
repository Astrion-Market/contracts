import { encodeAbiParameters, maxUint256, padHex, type Address, type Hex } from "viem";
import { decodeStrkey } from "./strkey";

export const MAX = maxUint256;

export enum AaveKind {
  Supply,
  SupplyCollateral,
  Withdraw,
  Borrow,
  Repay,
  RepayAll,
  RepayAvailable,
}

export enum MorphoKind {
  Supply,
  SupplyCollateral,
  Withdraw,
  WithdrawCollateral,
  Borrow,
  Repay,
  RepayAll,
  RepayAvailable,
}

export enum CompoundKind {
  SupplyBase,
  WithdrawBase,
  SupplyCollateral,
  WithdrawCollateral,
  RepayAll,
  RepayAvailable,
}

export const aaveAction = (kind: AaveKind, asset: Address, amount: bigint): Hex =>
  encodeAbiParameters(
    [{ type: "uint8" }, { type: "address" }, { type: "uint256" }],
    [kind, asset, amount],
  );

export const morphoAction = (kind: MorphoKind, amount: bigint): Hex =>
  encodeAbiParameters([{ type: "uint8" }, { type: "uint256" }], [kind, amount]);

export const compoundAction = (kind: CompoundKind, amount: bigint): Hex =>
  encodeAbiParameters([{ type: "uint8" }, { type: "uint256" }], [kind, amount]);

export type Finality = 1000 | 2000;

export function returnToStellarAction(args: {
  amount: bigint;
  maxFee: bigint;
  minFinality: Finality;
  recipient: string;
}): Hex {
  decodeStrkey(args.recipient);
  if (args.amount <= 0n || args.maxFee >= args.amount) throw new Error("FeeExceedsAmount");
  return encodeAbiParameters(
    [{ type: "uint256" }, { type: "uint256" }, { type: "uint32" }, { type: "string" }],
    [args.amount, args.maxFee, args.minFinality, args.recipient],
  );
}

export const aaveMarketScope = (loanAsset: Address): Hex => padHex(loanAsset, { size: 32 });
export const compoundMarketScope = (comet: Address): Hex => padHex(comet, { size: 32 });
