/**
 * Stellar strkey codec (SEP-23) for G (account), C (contract) and M (muxed)
 * recipients. Mirrors libs/crosschain-types/src/strkey.rs; both run
 * spec/fixtures/cctp/vectors.json.
 */
import { CodecError } from "./errors";

const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const VERSION_ACCOUNT = 6 << 3;
const VERSION_CONTRACT = 2 << 3;
const VERSION_MUXED = 12 << 3;

export type StrkeyKind = "account" | "contract" | "muxed";

export interface Strkey {
  kind: StrkeyKind;
  /** ed25519 public key (G, M) or contract id (C). */
  key: Uint8Array;
  /** Muxed id (M only). */
  muxedId?: bigint;
}

function crc16xmodem(data: Uint8Array): number {
  let crc = 0;
  for (const byte of data) {
    crc ^= byte << 8;
    for (let i = 0; i < 8; i++) {
      crc = crc & 0x8000 ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  return crc;
}

function base32Decode(s: string): Uint8Array {
  const out: number[] = [];
  let buffer = 0;
  let bits = 0;
  for (const c of s) {
    const value = ALPHABET.indexOf(c);
    if (value < 0) throw new CodecError("InvalidStrkey");
    buffer = (buffer << 5) | value;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      out.push((buffer >> bits) & 0xff);
      buffer &= (1 << bits) - 1;
    }
  }
  if (bits >= 5 || buffer !== 0) throw new CodecError("InvalidStrkey");
  return Uint8Array.from(out);
}

function base32Encode(data: Uint8Array): string {
  let out = "";
  let buffer = 0;
  let bits = 0;
  for (const byte of data) {
    buffer = (buffer << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      out += ALPHABET[(buffer >> bits) & 31];
    }
    buffer &= (1 << bits) - 1;
  }
  if (bits > 0) out += ALPHABET[(buffer << (5 - bits)) & 31];
  return out;
}

/** Strictly decode a G, C or M strkey. */
export function decodeStrkey(s: string): Strkey {
  if (s.length !== 56 && s.length !== 69) throw new CodecError("InvalidStrkey");
  const raw = base32Decode(s);
  if (raw.length < 3) throw new CodecError("InvalidStrkey");
  const body = raw.subarray(0, raw.length - 2);
  const crc = crc16xmodem(body);
  if (raw[raw.length - 2] !== (crc & 0xff) || raw[raw.length - 1] !== crc >> 8) {
    throw new CodecError("InvalidStrkey");
  }
  const version = body[0];
  const payload = body.subarray(1);
  let kind: StrkeyKind;
  let expectedLen: number;
  if (version === VERSION_ACCOUNT) [kind, expectedLen] = ["account", 32];
  else if (version === VERSION_CONTRACT) [kind, expectedLen] = ["contract", 32];
  else if (version === VERSION_MUXED) [kind, expectedLen] = ["muxed", 40];
  else throw new CodecError("UnsupportedStrkeyKind");
  if (payload.length !== expectedLen) throw new CodecError("InvalidStrkey");
  const key = payload.slice(0, 32);
  if (kind !== "muxed") return { kind, key };
  const view = new DataView(payload.buffer, payload.byteOffset + 32, 8);
  return { kind, key, muxedId: view.getBigUint64(0, false) };
}

export function encodeStrkey(kind: StrkeyKind, key: Uint8Array, muxedId?: bigint): string {
  if (key.length !== 32) throw new CodecError("InvalidStrkey");
  const version =
    kind === "account" ? VERSION_ACCOUNT : kind === "contract" ? VERSION_CONTRACT : VERSION_MUXED;
  const len = kind === "muxed" ? 41 : 33;
  const body = new Uint8Array(len + 2);
  body[0] = version;
  body.set(key, 1);
  if (kind === "muxed") new DataView(body.buffer).setBigUint64(33, muxedId ?? 0n, false);
  const crc = crc16xmodem(body.subarray(0, len));
  body[len] = crc & 0xff;
  body[len + 1] = crc >> 8;
  return base32Encode(body);
}
