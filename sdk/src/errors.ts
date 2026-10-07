/** Wire codes shared with libs/crosschain-types (CodecError::as_str). */
export type CodecErrorCode =
  | "InvalidStrkey"
  | "UnsupportedStrkeyKind"
  | "NotAContract"
  | "ZeroAmount"
  | "DustOnly"
  | "AmountTooLarge"
  | "FeeExceedsAmount"
  | "NotAnEvmAddress"
  | "MalformedHook"
  | "MalformedMessage"
  | "BadForwarderFields";

export class CodecError extends Error {
  constructor(readonly code: CodecErrorCode) {
    super(code);
    this.name = "CodecError";
  }
}
