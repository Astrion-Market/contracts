// Runs spec/fixtures/cctp/vectors.json, the same file the Rust codec runs.
import { describe, expect, test } from "bun:test";
import vectors from "../../spec/fixtures/cctp/vectors.json";
import {
  bytes32ToContract,
  bytes32ToEvmAddress,
  bytesToHex,
  cctpToStellar,
  CodecError,
  contractToBytes32,
  decodeMessage,
  decodeStrkey,
  encodeStrkey,
  evmAddressToBytes32,
  evmBurnToStellar,
  forwarderHook,
  hexToBytes,
  inboundToStellar,
  parseForwarderHook,
  stellarBurn,
  stellarToCctp,
  type StrkeyKind,
} from "../src";

function code(fn: () => unknown): string {
  try {
    fn();
  } catch (e) {
    if (e instanceof CodecError) return e.code;
    throw e;
  }
  throw new Error("expected a CodecError");
}

const strip = (h: string) => (h.startsWith("0x") ? h.slice(2) : h);

describe("strkey", () => {
  test("decodes and re-encodes", () => {
    for (const v of vectors.strkeys) {
      const key = decodeStrkey(v.strkey);
      expect(strip(bytesToHex(key.key))).toBe(v.hex);
      expect(key.kind).toBe(v.kind as StrkeyKind);
      const id = "id" in v && v.id ? BigInt(v.id) : undefined;
      expect(key.muxedId).toBe(id);
      expect(encodeStrkey(key.kind, key.key, id)).toBe(v.strkey);
    }
  });

  test("rejects invalid strkeys", () => {
    for (const v of vectors.invalidStrkeys) {
      expect(code(() => decodeStrkey(v.strkey))).toBe(v.error);
    }
  });

  test("contract bytes32 round trip; accounts rejected", () => {
    for (const v of vectors.strkeys) {
      if (v.kind === "contract") {
        const raw = contractToBytes32(v.strkey);
        expect(strip(bytesToHex(raw))).toBe(v.hex);
        expect(bytes32ToContract(raw)).toBe(v.strkey);
      } else {
        expect(code(() => contractToBytes32(v.strkey))).toBe("NotAContract");
      }
    }
  });
});

describe("amounts", () => {
  test("stellar -> cctp", () => {
    for (const c of vectors.amounts.stellarToCctp) {
      const input = BigInt(c.stellar);
      if ("error" in c && c.error) expect(code(() => stellarToCctp(input))).toBe(c.error);
      else expect(stellarToCctp(input)).toEqual({ burned: BigInt(c.burned!), dust: BigInt(c.dust!) });
    }
  });

  test("cctp -> stellar", () => {
    for (const c of vectors.amounts.cctpToStellar) {
      const input = BigInt(c.cctp);
      if ("error" in c && c.error) expect(code(() => cctpToStellar(input))).toBe(c.error);
      else expect(cctpToStellar(input)).toBe(BigInt(c.stellar!));
    }
  });
});

test("evm bytes32", () => {
  for (const c of vectors.evm.valid) {
    const b = evmAddressToBytes32(c.address);
    expect(bytesToHex(b)).toBe(c.bytes32 as `0x${string}`);
    expect(bytes32ToEvmAddress(b)).toBe(c.address.toLowerCase() as `0x${string}`);
  }
  for (const c of vectors.evm.invalidBytes32) {
    expect(code(() => bytes32ToEvmAddress(hexToBytes(c.bytes32)))).toBe(c.error);
  }
});

test("forwarder hooks", () => {
  for (const c of vectors.hooks.valid) {
    const payload = hexToBytes(c.payload);
    const built = forwarderHook(c.recipient, payload);
    expect(bytesToHex(built)).toBe(c.hex as `0x${string}`);
    const parsed = parseForwarderHook(built);
    expect(parsed.recipient).toBe(c.recipient);
    expect(bytesToHex(parsed.payload)).toBe(bytesToHex(payload));
  }
  for (const c of vectors.hooks.invalid) {
    expect(code(() => parseForwarderHook(hexToBytes(c.hex)))).toBe(c.error);
  }
});

describe("messages", () => {
  const forwarder = vectors.messages.forwarderTestnet;

  test("decode valid messages", () => {
    for (const c of vectors.messages.valid) {
      const m = decodeMessage(hexToBytes(c.hex));
      const d = c.decoded;
      expect(m.version).toBe(d.version);
      expect(m.sourceDomain).toBe(d.sourceDomain);
      expect(m.destinationDomain).toBe(d.destinationDomain);
      expect(bytesToHex(m.nonce)).toBe(d.nonce as `0x${string}`);
      expect(bytesToHex(m.destinationCaller)).toBe(d.destinationCaller as `0x${string}`);
      expect(m.minFinalityThreshold).toBe(d.minFinalityThreshold);
      expect(m.finalityThresholdExecuted).toBe(d.finalityThresholdExecuted);
      expect(bytesToHex(m.burn.burnToken)).toBe(d.burn.burnToken as `0x${string}`);
      expect(bytesToHex(m.burn.mintRecipient)).toBe(d.burn.mintRecipient as `0x${string}`);
      expect(m.burn.amount).toBe(BigInt(d.burn.amount));
      expect(bytesToHex(m.burn.messageSender)).toBe(d.burn.messageSender as `0x${string}`);
      expect(m.burn.maxFee).toBe(BigInt(d.burn.maxFee));
      expect(m.burn.feeExecuted).toBe(BigInt(d.burn.feeExecuted));
      expect(m.burn.expirationBlock).toBe(BigInt(d.burn.expirationBlock));
      expect(bytesToHex(m.burn.hookData)).toBe(d.burn.hookData as `0x${string}`);
      if ("inboundToStellar" in c && c.inboundToStellar) {
        const r = inboundToStellar(m, forwarder);
        expect(r.forwardRecipient).toBe(c.inboundToStellar.forwardRecipient);
        expect(r.mintedStellarAmount).toBe(BigInt(c.inboundToStellar.mintedStellarAmount));
      }
    }
  });

  test("reject malformed messages", () => {
    for (const c of vectors.messages.invalid) {
      expect(code(() => decodeMessage(hexToBytes(c.hex)))).toBe(c.error);
    }
  });

  test("reject bad inbound forwarder fields", () => {
    for (const c of vectors.messages.invalidInbound) {
      expect(code(() => inboundToStellar(decodeMessage(hexToBytes(c.hex)), forwarder))).toBe(c.error);
    }
  });
});

test("builders fail before burn on bad destinations", () => {
  const forwarder = vectors.messages.forwarderTestnet;
  const g = "GAUHMCMUP5FZO5675W3ISZ6E6CNYJGXBUW5WANE2JR4TGAARYCTSCBKI";
  const account = "0x2222222222222222222222222222222222222222";
  const base = { destinationDomain: 6, evmRecipient: account, maxFee: 100_000n, minFinalityThreshold: 1000 };

  const burn = stellarBurn({ ...base, amount: 10_000_005n });
  expect(burn.expectedBurned).toBe(1_000_000n);
  expect(burn.retainedDust).toBe(5n);
  expect(bytesToHex(burn.destinationCaller)).toBe(bytesToHex(new Uint8Array(32)));
  expect(code(() => stellarBurn({ ...base, amount: 10_000_000n, destinationDomain: 27 }))).toBe("BadForwarderFields");
  expect(code(() => stellarBurn({ ...base, amount: 10_000_000n, evmRecipient: "0x" + "00".repeat(20) }))).toBe("NotAnEvmAddress");
  expect(code(() => stellarBurn({ ...base, amount: 9n, maxFee: 0n }))).toBe("DustOnly");
  expect(code(() => stellarBurn({ ...base, amount: 100n, maxFee: 100n }))).toBe("FeeExceedsAmount");

  const out = evmBurnToStellar({ amount: 1_000_000n, forwarder, recipient: g, maxFee: 500n, minFinalityThreshold: 2000 });
  const fwd = bytesToHex(contractToBytes32(forwarder));
  expect(out.destinationDomain).toBe(27);
  expect(bytesToHex(out.mintRecipient)).toBe(fwd);
  expect(bytesToHex(out.destinationCaller)).toBe(fwd);
  expect(parseForwarderHook(out.hookData).recipient).toBe(g);
  const args = { amount: 1_000_000n, maxFee: 0n, minFinalityThreshold: 2000 };
  expect(code(() => evmBurnToStellar({ ...args, forwarder: g, recipient: g }))).toBe("NotAContract");
  expect(code(() => evmBurnToStellar({ ...args, forwarder, recipient: g.slice(0, 55) + "A" }))).toBe("InvalidStrkey");
  expect(code(() => evmBurnToStellar({ ...args, amount: 500n, maxFee: 500n, forwarder, recipient: g }))).toBe("FeeExceedsAmount");
});
