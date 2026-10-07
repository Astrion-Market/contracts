import {
  createPublicClient,
  createWalletClient,
  defineChain,
  encodeFunctionData,
  http,
  type Address,
  type Hex,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { astrionAccountAbi, decodeContractError } from "@astrion/crosschain-sdk";
import type { ChainAdapter, JobRequest, Simulation } from "./types";

export function callData(job: JobRequest): Hex {
  return job.funded
    ? encodeFunctionData({
        abi: astrionAccountAbi,
        functionName: "executeFundedIntent",
        args: [job.intent, job.action, job.signature, job.fee, job.funded.message, job.funded.attestation],
      })
    : encodeFunctionData({
        abi: astrionAccountAbi,
        functionName: "executeIntent",
        args: [job.intent, job.action, job.signature, job.fee],
      });
}

export function createViemChain(cfg: { rpcUrl: string; chainId: number; relayerKey: Hex }): ChainAdapter {
  const chain = defineChain({
    id: cfg.chainId,
    name: `chain-${cfg.chainId}`,
    nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
    rpcUrls: { default: { http: [cfg.rpcUrl] } },
  });
  const signer = privateKeyToAccount(cfg.relayerKey);
  const pub = createPublicClient({ chain, transport: http(cfg.rpcUrl) });
  const wallet = createWalletClient({ chain, account: signer, transport: http(cfg.rpcUrl) });

  return {
    chainId: cfg.chainId,
    relayerAddress: signer.address,
    ownerOf: (account: Address) =>
      pub.readContract({ address: account, abi: astrionAccountAbi, functionName: "owner" }) as Promise<Address>,
    async simulate(job: JobRequest): Promise<Simulation> {
      try {
        const data = callData(job);
        const { data: result } = await pub.call({ account: signer.address, to: job.account, data });
        if (job.funded && result === `0x${"00".repeat(32)}`) {
          return { ok: false, gas: 0n, error: "FundedActionWouldFail" };
        }
        const gas = await pub.estimateGas({ account: signer.address, to: job.account, data });
        return { ok: true, gas: (gas * 12n) / 10n };
      } catch (e) {
        const data = (e as { cause?: { data?: Hex } }).cause?.data;
        return { ok: false, gas: 0n, error: data ? decodeContractError(data).code : (e as Error).message };
      }
    },
    gasPriceWei: () => pub.getGasPrice(),
    async signTransaction(job: JobRequest, gas: bigint) {
      const request = await wallet.prepareTransactionRequest({ to: job.account, data: callData(job), gas });
      return wallet.signTransaction(request);
    },
    broadcast: (rawTx: Hex) => pub.sendRawTransaction({ serializedTransaction: rawTx }),
    async receipt(hash: Hex) {
      const r = await pub.getTransactionReceipt({ hash }).catch(() => null);
      return r ? { status: r.status, blockNumber: r.blockNumber, blockHash: r.blockHash } : null;
    },
    blockNumber: () => pub.getBlockNumber(),
    async blockHash(blockNumber: bigint) {
      const b = await pub.getBlock({ blockNumber }).catch(() => null);
      return b?.hash ?? null;
    },
    accountLogs: (accounts: Address[], fromBlock: bigint, toBlock: bigint) =>
      pub.getLogs({ address: accounts, fromBlock, toBlock }),
  };
}
