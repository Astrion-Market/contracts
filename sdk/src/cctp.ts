/**
 * Circle CCTP V2 codec for Stellar <-> EVM USDC routes. Mirrors
 * libs/crosschain-types/src/cctp.rs (see that file for sources and the facts
 * encoded). All amounts are bigint raw units.
 */
import { CodecError } from "./errors";
import { decodeStrkey, encodeStrkey } from "./strkey";

export const STELLAR_DOMAIN = 27;
export const MESSAGE_VERSION = 1;
export const BURN_MESSAGE_VERSION = 1;
export const HOOK_VERSION = 0;
export const MAX_STELLAR_AMOUNT = (1n << 63n) - 1n;

const HEADER_LEN = 148;
const BURN_BODY_LEN = 228;
const HOOK_HEADER_LEN = 32;

// ─── bytes helpers ───────────────────────────────────────────────────────────

export function hexToBytes(hex: string): Uint8Array {
  const s = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (s.length % 2 !== 0 || /[^0-9a-fA-F]/.test(s)) throw new Error(`bad hex: ${hex}`);
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(s.slice(i * 2, i * 2 + 2), 16);
  return out;
}

export function bytesToHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")}`;
}

const equal = (a: Uint8Array, b: Uint8Array) =>
  a.length === b.length && a.every((v, i) => v === b[i]);

// ─── amounts ─────────────────────────────────────────────────────────────────

/** Stellar 7-decimal amount -> { burned (6 decimals), dust (stays on Stellar) }. */
export function stellarToCctp(stellarRaw: bigint): { burned: bigint; dust: bigint } {
  if (stellarRaw <= 0n) throw new CodecError("ZeroAmount");
  if (stellarRaw > MAX_STELLAR_AMOUNT) throw new CodecError("AmountTooLarge");
  const burned = stellarRaw / 10n;
  if (burned === 0n) throw new CodecError("DustOnly");
  return { burned, dust: stellarRaw % 10n };
}

/** CCTP 6-decimal amount -> Stellar 7-decimal amount minted. */
export function cctpToStellar(cctpRaw: bigint): bigint {
  if (cctpRaw <= 0n) throw new CodecError("ZeroAmount");
  const scaled = cctpRaw * 10n;
  if (scaled > MAX_STELLAR_AMOUNT) throw new CodecError("AmountTooLarge");
  return scaled;
}

// ─── addresses ───────────────────────────────────────────────────────────────

export function evmAddressToBytes32(address: string): Uint8Array {
  const raw = hexToBytes(address);
  if (raw.length !== 20) throw new CodecError("NotAnEvmAddress");
  const out = new Uint8Array(32);
  out.set(raw, 12);
  return out;
}

export function bytes32ToEvmAddress(value: Uint8Array): `0x${string}` {
  if (value.length !== 32 || value.subarray(0, 12).some((b) => b !== 0)) {
    throw new CodecError("NotAnEvmAddress");
  }
  return bytesToHex(value.subarray(12));
}

/** Contract strkey (C...) -> raw 32-byte id. Accounts/muxed are rejected. */
export function contractToBytes32(contract: string): Uint8Array {
  const key = decodeStrkey(contract);
  if (key.kind !== "contract") throw new CodecError("NotAContract");
  return key.key;
}

export function bytes32ToContract(value: Uint8Array): string {
  return encodeStrkey("contract", value);
}

// ─── forwarder hook ──────────────────────────────────────────────────────────

/** bytes24 zero magic | u32 BE version 0 | u32 BE length | strkey UTF-8 | payload */
export function forwarderHook(recipient: string, payload: Uint8Array = new Uint8Array()): Uint8Array {
  decodeStrkey(recipient);
  const r = new TextEncoder().encode(recipient);
  const out = new Uint8Array(HOOK_HEADER_LEN + r.length + payload.length);
  const view = new DataView(out.buffer);
  view.setUint32(24, HOOK_VERSION, false);
  view.setUint32(28, r.length, false);
  out.set(r, HOOK_HEADER_LEN);
  out.set(payload, HOOK_HEADER_LEN + r.length);
  return out;
}

export function parseForwarderHook(hook: Uint8Array): { recipient: string; payload: Uint8Array } {
  if (hook.length < HOOK_HEADER_LEN || hook.subarray(0, 24).some((b) => b !== 0)) {
    throw new CodecError("MalformedHook");
  }
  const view = new DataView(hook.buffer, hook.byteOffset, hook.length);
  if (view.getUint32(24, false) !== HOOK_VERSION) throw new CodecError("MalformedHook");
  const end = HOOK_HEADER_LEN + view.getUint32(28, false);
  if (end > hook.length) throw new CodecError("MalformedHook");
  let recipient: string;
  try {
    recipient = new TextDecoder("utf-8", { fatal: true }).decode(hook.subarray(HOOK_HEADER_LEN, end));
  } catch {
    throw new CodecError("MalformedHook");
  }
  decodeStrkey(recipient);
  return { recipient, payload: hook.slice(end) };
}

// ─── raw message decoding ────────────────────────────────────────────────────

export interface BurnMessage {
  version: number;
  burnToken: Uint8Array;
  mintRecipient: Uint8Array;
  amount: bigint;
  messageSender: Uint8Array;
  maxFee: bigint;
  feeExecuted: bigint;
  expirationBlock: bigint;
  hookData: Uint8Array;
}

export interface Message {
  version: number;
  sourceDomain: number;
  destinationDomain: number;
  nonce: Uint8Array;
  sender: Uint8Array;
  recipient: Uint8Array;
  destinationCaller: Uint8Array;
  minFinalityThreshold: number;
  finalityThresholdExecuted: number;
  burn: BurnMessage;
}

/** Decode a raw CCTP V2 burn message (use when API address fields are null). */
export function decodeMessage(data: Uint8Array): Message {
  if (data.length < HEADER_LEN + BURN_BODY_LEN) throw new CodecError("MalformedMessage");
  const view = new DataView(data.buffer, data.byteOffset, data.length);
  const u32 = (at: number) => view.getUint32(at, false);
  const b32 = (at: number) => data.slice(at, at + 32);
  const u256 = (at: number) => {
    if (data.subarray(at, at + 16).some((b) => b !== 0)) throw new CodecError("MalformedMessage");
    return BigInt(bytesToHex(data.subarray(at, at + 32)));
  };
  const body = HEADER_LEN;
  const version = u32(0);
  const burnVersion = u32(body);
  if (version !== MESSAGE_VERSION || burnVersion !== BURN_MESSAGE_VERSION) {
    throw new CodecError("MalformedMessage");
  }
  const burn: BurnMessage = {
    version: burnVersion,
    burnToken: b32(body + 4),
    mintRecipient: b32(body + 36),
    amount: u256(body + 68),
    messageSender: b32(body + 100),
    maxFee: u256(body + 132),
    feeExecuted: u256(body + 164),
    expirationBlock: u256(body + 196),
    hookData: data.slice(body + BURN_BODY_LEN),
  };
  if (burn.feeExecuted > burn.amount) throw new CodecError("MalformedMessage");
  return {
    version,
    sourceDomain: u32(4),
    destinationDomain: u32(8),
    nonce: b32(12),
    sender: b32(44),
    recipient: b32(76),
    destinationCaller: b32(108),
    minFinalityThreshold: u32(140),
    finalityThresholdExecuted: u32(144),
    burn,
  };
}

/** Validate an inbound message against the expected CctpForwarder. */
export function inboundToStellar(message: Message, forwarder: string) {
  const fwd = contractToBytes32(forwarder);
  if (
    message.destinationDomain !== STELLAR_DOMAIN ||
    !equal(message.burn.mintRecipient, fwd) ||
    !equal(message.destinationCaller, fwd)
  ) {
    throw new CodecError("BadForwarderFields");
  }
  const { recipient, payload } = parseForwarderHook(message.burn.hookData);
  return {
    forwardRecipient: recipient,
    hookPayload: payload,
    mintedStellarAmount: cctpToStellar(message.burn.amount - message.burn.feeExecuted),
  };
}

// ─── outbound builders ───────────────────────────────────────────────────────

/** Args for Stellar TokenMessengerMinter.deposit_for_burn (7-decimal amounts). */
export function stellarBurn(args: {
  amount: bigint;
  destinationDomain: number;
  evmRecipient: string;
  destinationCaller?: string;
  maxFee: bigint;
  minFinalityThreshold: number;
}) {
  if (args.destinationDomain === STELLAR_DOMAIN) throw new CodecError("BadForwarderFields");
  const { burned, dust } = stellarToCctp(args.amount);
  if (args.maxFee >= args.amount) throw new CodecError("FeeExceedsAmount");
  const mintRecipient = evmAddressToBytes32(args.evmRecipient);
  if (mintRecipient.every((b) => b === 0)) throw new CodecError("NotAnEvmAddress");
  return {
    amount: args.amount,
    destinationDomain: args.destinationDomain,
    mintRecipient,
    destinationCaller: args.destinationCaller
      ? evmAddressToBytes32(args.destinationCaller)
      : new Uint8Array(32),
    maxFee: args.maxFee,
    minFinalityThreshold: args.minFinalityThreshold,
    expectedBurned: burned,
    retainedDust: dust,
  };
}

/** Args for EVM TokenMessengerV2.depositForBurnWithHook to Stellar (6 decimals). */
export function evmBurnToStellar(args: {
  amount: bigint;
  forwarder: string;
  recipient: string;
  maxFee: bigint;
  minFinalityThreshold: number;
}) {
  const fwd = contractToBytes32(args.forwarder);
  const hookData = forwarderHook(args.recipient);
  if (args.amount <= 0n) throw new CodecError("ZeroAmount");
  if (args.maxFee >= args.amount) throw new CodecError("FeeExceedsAmount");
  cctpToStellar(args.amount - args.maxFee);
  return {
    amount: args.amount,
    destinationDomain: STELLAR_DOMAIN,
    mintRecipient: fwd,
    destinationCaller: fwd,
    maxFee: args.maxFee,
    minFinalityThreshold: args.minFinalityThreshold,
    hookData,
  };
}
