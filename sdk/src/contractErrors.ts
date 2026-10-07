import { decodeErrorResult, type Abi, type Hex } from "viem";
import {
  aaveV3ModuleAbi,
  astrionAccountAbi,
  cctpReturnModuleAbi,
  compoundV3ModuleAbi,
  morphoBlueModuleAbi,
} from "./abi/generated";

const ABIS = [astrionAccountAbi, aaveV3ModuleAbi, morphoBlueModuleAbi, compoundV3ModuleAbi, cctpReturnModuleAbi] as Abi[];

export const RETRYABLE_CONTRACT_ERRORS = new Set(["RoutePaused", "IntentExpired"]);

export interface ContractErrorInfo {
  code: string;
  args: readonly unknown[];
  retryable: boolean;
}

/** Decode revert data from any Astrion account or module into a stable code. */
export function decodeContractError(data: Hex): ContractErrorInfo {
  for (const abi of ABIS) {
    try {
      const r = decodeErrorResult({ abi, data });
      return { code: r.errorName, args: r.args ?? [], retryable: RETRYABLE_CONTRACT_ERRORS.has(r.errorName) };
    } catch {}
  }
  return { code: "Unknown", args: [], retryable: false };
}
