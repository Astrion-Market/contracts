import type { Address, Hex } from "viem";
import type { Indexer } from "./indexer";
import type { Relayer } from "./relayer";
import type { JobRequest } from "./types";
import { RejectedError } from "./validate";

const replacer = (_: string, v: unknown) => (typeof v === "bigint" ? v.toString() : v);
const reply = (status: number, body: unknown) =>
  new Response(JSON.stringify(body, replacer), { status, headers: { "content-type": "application/json" } });

const BIGINT_FIELDS = ["maxFee", "nonce", "deadline", "fee"];
const reviver = (k: string, v: unknown) => (BIGINT_FIELDS.includes(k) && typeof v === "string" ? BigInt(v) : v);

/** Status API. Jobs are keyed by intent digest, so POST is idempotent. */
export function createApi(relayer: Relayer, indexer: Indexer) {
  return async (req: Request): Promise<Response> => {
    const url = new URL(req.url);
    const parts = url.pathname.split("/").filter(Boolean);
    try {
      if (req.method === "GET" && url.pathname === "/health") return reply(200, { ok: true });
      if (req.method === "POST" && url.pathname === "/v1/jobs") {
        const body = JSON.parse(await req.text(), reviver) as JobRequest;
        const { job, created } = await relayer.submit(body);
        return reply(created ? 201 : 200, { id: job.id, state: job.state });
      }
      if (req.method === "GET" && parts[0] === "v1" && parts[1] === "jobs" && parts[2]) {
        const job = relayer.store.getJob(parts[2] as Hex);
        return job
          ? reply(200, { id: job.id, state: job.state, txHash: job.txHash, blockNumber: job.blockNumber, lastError: job.lastError })
          : reply(404, { error: "NotFound" });
      }
      if (req.method === "GET" && parts[0] === "v1" && parts[1] === "accounts" && parts[2] && parts[3] === "operations") {
        return reply(200, indexer.operations(parts[2] as Address));
      }
      return reply(404, { error: "NotFound" });
    } catch (e) {
      if (e instanceof RejectedError) return reply(422, { error: e.code });
      return reply(400, { error: "BadRequest", detail: (e as Error).message });
    }
  };
}
