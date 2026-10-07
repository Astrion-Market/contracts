import { keccak256, toBytes, type Hex } from "viem";

export type ProtocolName = "aave-v3" | "morpho-blue" | "compound-v3";

export const PROTOCOL_IDS: Record<ProtocolName, Hex> = {
  "aave-v3": keccak256(toBytes("aave-v3")),
  "morpho-blue": keccak256(toBytes("morpho-blue")),
  "compound-v3": keccak256(toBytes("compound-v3")),
};

export function protocolFromId(id: Hex): ProtocolName {
  const found = (Object.keys(PROTOCOL_IDS) as ProtocolName[]).find(
    (p) => PROTOCOL_IDS[p].toLowerCase() === id.toLowerCase(),
  );
  if (!found) throw new Error(`unknown protocol id ${id}`);
  return found;
}
