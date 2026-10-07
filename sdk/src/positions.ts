import type { Address, PublicClient } from "viem";
import { aaveV3LensAbi, compoundV3LensAbi, morphoBlueLensAbi } from "./abi/generated";
import type { ProtocolName } from "./protocols";

export interface Source {
  chainId: number;
  blockNumber: bigint;
  timestamp: bigint;
}

export type Risk =
  | {
      protocol: "aave-v3";
      healthFactor: bigint;
      ltv: bigint;
      liquidationThreshold: bigint;
      availableBorrowsBase: bigint;
      eModeCategory: bigint;
    }
  | {
      protocol: "morpho-blue";
      oracleOk: boolean;
      oraclePrice: bigint;
      maxBorrow: bigint;
      lltv: bigint;
      healthy: boolean;
    }
  | {
      protocol: "compound-v3";
      borrowCollateralized: boolean;
      liquidatable: boolean;
      baseBorrowMin: bigint;
    };

/**
 * Normalized shape, protocol-aware risk. Amounts are raw token units of the
 * protocol; risk is never combined across positions or protocols.
 */
export interface Position {
  protocol: ProtocolName;
  account: Address;
  source: Source;
  supplied: bigint;
  collateral: bigint;
  debt: bigint;
  risk: Risk;
}

export const freshnessSecs = (p: Position, nowSecs: bigint): bigint => nowSecs - p.source.timestamp;

export const isStale = (p: Position, nowSecs: bigint, maxAgeSecs: bigint): boolean =>
  freshnessSecs(p, nowSecs) > maxAgeSecs;

export async function readAavePosition(
  client: PublicClient,
  lens: Address,
  account: Address,
  loanAsset: Address,
  collateralAsset: Address,
): Promise<Position> {
  const p = await client.readContract({
    address: lens,
    abi: aaveV3LensAbi,
    functionName: "position",
    args: [account, loanAsset, collateralAsset],
  });
  return {
    protocol: "aave-v3",
    account,
    source: { chainId: await client.getChainId(), blockNumber: p.blockNumber, timestamp: p.timestamp },
    supplied: p.loanSupplied,
    collateral: p.collateralSupplied,
    debt: p.loanVariableDebt,
    risk: {
      protocol: "aave-v3",
      healthFactor: p.healthFactor,
      ltv: p.ltv,
      liquidationThreshold: p.liquidationThreshold,
      availableBorrowsBase: p.availableBorrowsBase,
      eModeCategory: p.eModeCategory,
    },
  };
}

export interface MorphoMarketParams {
  loanToken: Address;
  collateralToken: Address;
  oracle: Address;
  irm: Address;
  lltv: bigint;
}

export async function readMorphoPosition(
  client: PublicClient,
  lens: Address,
  params: MorphoMarketParams,
  account: Address,
): Promise<Position> {
  const p = await client.readContract({
    address: lens,
    abi: morphoBlueLensAbi,
    functionName: "position",
    args: [params, account],
  });
  return {
    protocol: "morpho-blue",
    account,
    source: { chainId: await client.getChainId(), blockNumber: p.blockNumber, timestamp: p.timestamp },
    supplied: p.supplyAssets,
    collateral: p.collateral,
    debt: p.borrowAssets,
    risk: {
      protocol: "morpho-blue",
      oracleOk: p.oracleOk,
      oraclePrice: p.oraclePrice,
      maxBorrow: p.maxBorrow,
      lltv: p.lltv,
      healthy: p.healthy,
    },
  };
}

export async function readCompoundPosition(
  client: PublicClient,
  lens: Address,
  comet: Address,
  account: Address,
  collateralAsset: Address,
): Promise<Position> {
  const p = await client.readContract({
    address: lens,
    abi: compoundV3LensAbi,
    functionName: "position",
    args: [comet, account, collateralAsset],
  });
  return {
    protocol: "compound-v3",
    account,
    source: { chainId: await client.getChainId(), blockNumber: p.blockNumber, timestamp: p.timestamp },
    supplied: p.baseSupplied,
    collateral: p.collateral,
    debt: p.baseBorrowed,
    risk: {
      protocol: "compound-v3",
      borrowCollateralized: p.borrowCollateralized,
      liquidatable: p.liquidatable,
      baseBorrowMin: p.baseBorrowMin,
    },
  };
}
