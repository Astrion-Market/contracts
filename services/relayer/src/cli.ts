import { readFileSync } from "node:fs";
import { createPublicClient, createWalletClient, defineChain, http, type Hex } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { createApi } from "./api";
import { Indexer } from "./indexer";
import { Policy } from "./policy";
import { Relayer } from "./relayer";
import { Store } from "./store";
import type { JobRequest } from "./types";
import { callData, createViemChain } from "./viemChain";

const env = (k: string, d?: string) => {
  const v = process.env[k] ?? d;
  if (v === undefined) throw new Error(`missing env ${k}`);
  return v;
};
const BIG = ["maxFee", "nonce", "deadline", "fee"];
const readJob = (path: string) =>
  JSON.parse(readFileSync(path, "utf8"), (k, v) => (BIG.includes(k) && typeof v === "string" ? BigInt(v) : v)) as JobRequest;

async function serve() {
  const chain = createViemChain({
    rpcUrl: env("RPC_URL"),
    chainId: Number(env("CHAIN_ID")),
    relayerKey: env("RELAYER_PRIVATE_KEY") as Hex,
  });
  const store = new Store(env("DB_PATH", "relayer.sqlite"));
  const policy = new Policy({
    jobsPerOwnerPerMinute: Number(env("JOBS_PER_OWNER_PER_MINUTE", "5")),
    ownerDailyBudgetWei: BigInt(env("OWNER_DAILY_BUDGET_WEI", "2000000000000000")),
    globalDailyBudgetWei: BigInt(env("GLOBAL_DAILY_BUDGET_WEI", "100000000000000000")),
  });
  const relayer = new Relayer(store, chain, policy, {
    confirmations: Number(env("CONFIRMATIONS", "10")),
    maxAttempts: Number(env("MAX_ATTEMPTS", "5")),
    backoffMs: Number(env("BACKOFF_MS", "15000")),
    now: () => Date.now(),
  });
  const indexer = new Indexer(store, chain, {
    startBlock: BigInt(env("START_BLOCK", "0")),
    batchSize: BigInt(env("BATCH_SIZE", "2000")),
    reorgDepth: BigInt(env("REORG_DEPTH", "64")),
  });
  const pollMs = Number(env("POLL_MS", "4000"));
  const loop = async () => {
    await relayer.tick().catch((e) => console.error("relayer tick", e));
    await indexer.tick().catch((e) => console.error("indexer tick", e));
    setTimeout(loop, pollMs);
  };
  loop();
  const port = Number(env("PORT", "8787"));
  Bun.serve({ port, fetch: createApi(relayer, indexer) });
  console.log(`relayer ${chain.relayerAddress} on chain ${chain.chainId}, listening on :${port}`);
}

/** Self-operated path: submit the same call with your own key. No server, no sponsorship. */
async function self(path: string) {
  const job = readJob(path);
  const rpcUrl = env("RPC_URL");
  const chain = defineChain({
    id: job.chainId,
    name: `chain-${job.chainId}`,
    nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
    rpcUrls: { default: { http: [rpcUrl] } },
  });
  const account = privateKeyToAccount(env("SENDER_PRIVATE_KEY") as Hex);
  const wallet = createWalletClient({ chain, account, transport: http(rpcUrl) });
  const hash = await wallet.sendTransaction({ to: job.account, data: callData(job) });
  const receipt = await createPublicClient({ chain, transport: http(rpcUrl) }).waitForTransactionReceipt({ hash });
  console.log(JSON.stringify({ hash, status: receipt.status, blockNumber: receipt.blockNumber.toString() }));
}

async function client(method: "POST" | "GET", path: string, body?: string) {
  const res = await fetch(`${env("RELAYER_URL", "http://localhost:8787")}${path}`, {
    method,
    body,
    headers: body ? { "content-type": "application/json" } : undefined,
  });
  console.log(res.status, await res.text());
}

const [cmd, arg] = process.argv.slice(2);
switch (cmd) {
  case "serve":
    await serve();
    break;
  case "submit":
    await client("POST", "/v1/jobs", readFileSync(arg!, "utf8"));
    break;
  case "status":
    await client("GET", `/v1/jobs/${arg}`);
    break;
  case "self":
    await self(arg!);
    break;
  default:
    console.log("usage: cli.ts serve | submit <job.json> | status <id> | self <job.json>");
    process.exit(1);
}
