import {
  encodeAbiParameters,
  getContractAddress,
  hashTypedData,
  keccak256,
  concatHex,
  zeroHash,
  type Address,
  type Hex,
} from "viem";

export interface ExecutionIntent {
  module: Address;
  moduleCodeHash: Hex;
  target: Address;
  actionHash: Hex;
  recipient: Address;
  relayer: Address;
  feeToken: Address;
  maxFee: bigint;
  nonce: bigint;
  deadline: bigint;
  transferId: Hex;
}

export const EXECUTION_INTENT_TYPES = {
  ExecutionIntent: [
    { name: "module", type: "address" },
    { name: "moduleCodeHash", type: "bytes32" },
    { name: "target", type: "address" },
    { name: "actionHash", type: "bytes32" },
    { name: "recipient", type: "address" },
    { name: "relayer", type: "address" },
    { name: "feeToken", type: "address" },
    { name: "maxFee", type: "uint256" },
    { name: "nonce", type: "uint256" },
    { name: "deadline", type: "uint256" },
    { name: "transferId", type: "bytes32" },
  ],
} as const;

export const intentDomain = (chainId: number, account: Address) =>
  ({ name: "AstrionAccount", version: "1", chainId, verifyingContract: account }) as const;

/** Typed data for wallet signing (eth_signTypedData_v4). */
export function intentTypedData(chainId: number, account: Address, intent: ExecutionIntent) {
  return {
    domain: intentDomain(chainId, account),
    types: EXECUTION_INTENT_TYPES,
    primaryType: "ExecutionIntent" as const,
    message: intent,
  };
}

export function intentDigest(chainId: number, account: Address, intent: ExecutionIntent): Hex {
  return hashTypedData(intentTypedData(chainId, account, intent));
}

export function buildIntent(args: {
  module: Address;
  moduleCodeHash: Hex;
  target: Address;
  action: Hex;
  recipient: Address;
  nonce: bigint;
  deadline: bigint;
  feeToken: Address;
  maxFee?: bigint;
  relayer?: Address;
  transferId?: Hex;
}): ExecutionIntent {
  if (args.recipient === "0x0000000000000000000000000000000000000000") throw new Error("InvalidRecipient");
  return {
    module: args.module,
    moduleCodeHash: args.moduleCodeHash,
    target: args.target,
    actionHash: keccak256(args.action),
    recipient: args.recipient,
    relayer: args.relayer ?? "0x0000000000000000000000000000000000000000",
    feeToken: args.feeToken,
    maxFee: args.maxFee ?? 0n,
    nonce: args.nonce,
    deadline: args.deadline,
    transferId: args.transferId ?? zeroHash,
  };
}

export const transferId = (sourceDomain: number, nonce: Hex): Hex =>
  keccak256(encodeAbiParameters([{ type: "uint32" }, { type: "bytes32" }], [sourceDomain, nonce]));

export interface AccountScope {
  owner: Address;
  chainId: bigint;
  protocolId: Hex;
  marketScope: Hex;
  version: number;
}

export function accountSalt(scope: AccountScope): Hex {
  return keccak256(
    encodeAbiParameters(
      [{ type: "address" }, { type: "uint256" }, { type: "bytes32" }, { type: "bytes32" }, { type: "uint32" }],
      [scope.owner, scope.chainId, scope.protocolId, scope.marketScope, scope.version],
    ),
  );
}

/** CREATE2 address of a scoped account; mirrors AstrionAccountFactory.predictAccount. */
export function predictAccount(args: {
  factory: Address;
  accountCreationCode: Hex;
  policy: Address;
  messageTransmitter: Address;
  usdc: Address;
  localDomain: number;
  scope: AccountScope;
}): Address {
  const ctorArgs = encodeAbiParameters(
    [
      { type: "address" },
      { type: "address" },
      { type: "address" },
      { type: "address" },
      { type: "uint32" },
      { type: "bytes32" },
      { type: "bytes32" },
      { type: "uint32" },
    ],
    [
      args.scope.owner,
      args.policy,
      args.messageTransmitter,
      args.usdc,
      args.localDomain,
      args.scope.protocolId,
      args.scope.marketScope,
      args.scope.version,
    ],
  );
  return getContractAddress({
    opcode: "CREATE2",
    from: args.factory,
    salt: accountSalt(args.scope),
    bytecode: concatHex([args.accountCreationCode, ctorArgs]),
  });
}
