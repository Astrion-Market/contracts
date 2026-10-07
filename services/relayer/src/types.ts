import type { Address, Hex } from "viem";
import type { ExecutionIntent } from "@astrion/crosschain-sdk";

export type JobState =
  | "validated"
  | "submitted"
  | "confirmed"
  | "final"
  | "failed_retryable"
  | "failed_final"
  | "rejected";

export interface JobRequest {
  chainId: number;
  account: Address;
  intent: ExecutionIntent;
  action: Hex;
  signature: Hex;
  fee: bigint;
  funded?: { message: Hex; attestation: Hex };
}

export interface Job {
  id: Hex;
  request: JobRequest;
  owner: Address;
  state: JobState;
  attempts: number;
  nextAttemptAt: number;
  rawTx: Hex | null;
  txHash: Hex | null;
  blockNumber: bigint | null;
  blockHash: Hex | null;
  lastError: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface Receipt {
  status: "success" | "reverted";
  blockNumber: bigint;
  blockHash: Hex;
}

export interface Simulation {
  ok: boolean;
  gas: bigint;
  error?: string;
}

/** Everything the relayer needs from a chain. Implemented with viem, faked in tests. */
export interface ChainAdapter {
  chainId: number;
  relayerAddress: Address;
  ownerOf(account: Address): Promise<Address>;
  simulate(job: JobRequest): Promise<Simulation>;
  gasPriceWei(): Promise<bigint>;
  signTransaction(job: JobRequest, gas: bigint): Promise<Hex>;
  broadcast(rawTx: Hex): Promise<Hex>;
  receipt(txHash: Hex): Promise<Receipt | null>;
  blockNumber(): Promise<bigint>;
  blockHash(blockNumber: bigint): Promise<Hex | null>;
  accountLogs(accounts: Address[], fromBlock: bigint, toBlock: bigint): Promise<import("viem").Log[]>;
}
