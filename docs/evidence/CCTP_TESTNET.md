# CCTP testnet transport evidence

Gate for C07: reproducible Stellar testnet ↔ Base Sepolia USDC transfers in
**both** directions, with tx hashes and amount reconciliation.

## Status: BLOCKED (awaiting a live run)

The harness (`ops/crosschain/`) is implemented but has **not been executed
against live networks**. It needs funded test wallets that the operator holds
locally. Mocks do not satisfy this gate. Until a run is published below, no
transport claim is made.

| Direction | Run | Result |
|---|---|---|
| Stellar testnet → Base Sepolia | — | not run |
| Base Sepolia → Stellar testnet | — | not run |
| Stellar testnet ↔ Ethereum Sepolia | — | configured, not run |

## Assumptions the first live run must confirm

1. Stellar `deposit_for_burn` argument names. The **order** is from Circle's
   quickstart; names are read from the on-chain spec at run time and recorded
   in the checkpoint (`steps.burn.argNames`).
2. Forwarder hook `uint32` fields are big-endian (Circle's quickstart uses
   `writeUInt32BE`). The codecs in `libs/crosschain-types` and `sdk/` assume it.
3. CCTP V2 `MessageV2` / `BurnMessageV2` version is `1` in both header and body.
4. Iris accepts the Stellar tx hash (hex, no `0x`) for `transactionHash`.

## How to run

```bash
# One-time: a funded Stellar testnet key with a USDC trustline, an EVM test key
# with Base Sepolia ETH, and testnet USDC on both sides (Circle faucet).
stellar keys generate cctp-test --network testnet --fund
export STELLAR_SOURCE=cctp-test
export EVM_PRIVATE_KEY=0x...          # test wallet only, never a real key
export BASE_SEPOLIA_RPC_URL=https://...

make cctp-stellar-to-evm RUN=s2b-001 AMOUNT=10000005 MAX_FEE=0   # 1.0000005 USDC (7 dp)
make cctp-evm-to-stellar RUN=b2s-001 AMOUNT=1000000  MAX_FEE=0   # 1 USDC (6 dp)
make cctp-evidence RUN=s2b-001
make cctp-evidence RUN=b2s-001
```

Every step is checkpointed in `ops/crosschain/runs/<run>.json` (gitignored).
Re-running the same `RUN` resumes:

| Interruption | Behaviour |
|---|---|
| Crash after signing, before send | Re-sends the **same** signed tx (same sequence/nonce) |
| Crash after send | Confirms the recorded hash; never builds a new burn |
| Attestation delayed | Keeps polling (`ATTESTATION_TIMEOUT_SECS`), rerun continues waiting |
| Mint already done by someone else | Detects the used nonce; records it; no duplicate mint |
| Missing Stellar USDC trustline | Fails in preflight, **before** any burn |

A burn cannot be cancelled. Once burned, a run only moves forward to delivery.
