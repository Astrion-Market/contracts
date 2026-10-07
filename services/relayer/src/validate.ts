import { keccak256, recoverTypedDataAddress, zeroAddress, zeroHash, type Address, type Hex } from "viem";
import { intentDigest, intentTypedData } from "@astrion/crosschain-sdk";
import type { ChainAdapter, JobRequest } from "./types";

export class RejectedError extends Error {
  constructor(readonly code: string, detail?: string) {
    super(detail ? `${code}: ${detail}` : code);
  }
}

/**
 * Cheap, gas-free checks before a job is stored: an attacker cannot make the
 * relayer spend anything with arbitrary or forged jobs.
 */
export async function validateRequest(
  req: JobRequest,
  chain: ChainAdapter,
  nowSecs: bigint,
): Promise<{ id: Hex; owner: Address }> {
  if (req.chainId !== chain.chainId) throw new RejectedError("WrongChain");
  const i = req.intent;
  if (keccak256(req.action) !== i.actionHash) throw new RejectedError("ActionHashMismatch");
  if (i.deadline <= nowSecs) throw new RejectedError("IntentExpired");
  if (req.fee > i.maxFee) throw new RejectedError("FeeAboveCap");
  if (i.relayer !== zeroAddress && i.relayer.toLowerCase() !== chain.relayerAddress.toLowerCase()) {
    throw new RejectedError("WrongRelayer");
  }
  const funded = i.transferId !== zeroHash;
  if (funded !== !!req.funded) throw new RejectedError("TransferBindingMismatch");

  const recovered = await recoverTypedDataAddress({
    ...intentTypedData(req.chainId, req.account, i),
    signature: req.signature,
  }).catch(() => {
    throw new RejectedError("InvalidSignature");
  });
  const owner = await chain.ownerOf(req.account).catch(() => {
    throw new RejectedError("UnknownAccount");
  });
  if (recovered.toLowerCase() !== owner.toLowerCase()) throw new RejectedError("InvalidSignature");
  return { id: intentDigest(req.chainId, req.account, i), owner };
}
