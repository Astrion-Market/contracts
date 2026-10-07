# Astrion relayer and indexer

A small, open-source service that submits owner-signed intents to Astrion
execution accounts and indexes their operation events. **Relayer keys pay gas
only.** Users keep all spending authority, and every action can also be done
without this service (see `self` below and `docs/ops/DIRECT_OWNER_ACTIONS.md`).

## Guarantees

| Property | How |
|---|---|
| Idempotent submission | Job id = EIP-712 intent digest. Re-POSTing returns the existing job |
| No double send across restarts | The signed raw tx is persisted **before** broadcast. Restarts rebroadcast the same tx (same EVM nonce) |
| Finality and reorgs | `final` only after `CONFIRMATIONS` on a still-canonical block. A reorg sends the job back to `submitted` and rebroadcasts the same tx |
| Sponsorship cannot be drained | Signature recovers to the on-chain `owner()`, deadline/fee/relayer/action-hash checks, per-owner rate limit, simulation before signing, per-owner and global daily gas budgets |
| Durable state | SQLite (WAL, `synchronous=FULL`) for jobs, spend, events and cursors |
| Indexer | Events keyed by `chain:tx:logIndex` (inserted once). Hash-checked cursor rewinds `REORG_DEPTH` blocks on mismatch |

A reverted transaction is `failed_final`. It is never retried as a new send.
The owner decides what to do next.

## Run

```bash
bun install
RPC_URL=… CHAIN_ID=8453 RELAYER_PRIVATE_KEY=0x… DB_PATH=relayer.sqlite bun src/cli.ts serve
bun src/cli.ts submit job.json     # POST /v1/jobs
bun src/cli.ts status <id>         # GET  /v1/jobs/:id
SENDER_PRIVATE_KEY=0x… RPC_URL=… bun src/cli.ts self job.json   # no server: pay your own gas
```

API: `POST /v1/jobs`, `GET /v1/jobs/:id`, `GET /v1/accounts/:account/operations`, `GET /health`.

Config (env): `CONFIRMATIONS` (10), `MAX_ATTEMPTS` (5), `BACKOFF_MS` (15000),
`JOBS_PER_OWNER_PER_MINUTE` (5), `OWNER_DAILY_BUDGET_WEI`, `GLOBAL_DAILY_BUDGET_WEI`,
`START_BLOCK`, `BATCH_SIZE` (2000), `REORG_DEPTH` (64), `POLL_MS` (4000), `PORT` (8787).
