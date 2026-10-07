# Astrion EVM execution layer

User-owned execution accounts, bounded intents and protocol modules for Base
and Ethereum. Design: [ADR-0001](../docs/adr/0001-crosschain-lending-direction.md),
[architecture](../docs/architecture/crosschain.md).

## Toolchain (pinned)

| Component | Version |
|---|---|
| Foundry (forge) | 1.7.1 |
| solc | 0.8.30, `evm_version = cancun` |
| forge-std | v1.17.0 (`lib/forge-std`, submodule) |
| OpenZeppelin Contracts | v5.7.0 (`lib/openzeppelin-contracts`, submodule) |

## Commands (from the repo root)

```bash
make evm-deps        # init pinned submodules
make evm-build
make evm-test        # unit tests, no network
make evm-fork-test   # fork suites; needs BASE_RPC_URL / ETH_RPC_URL
make evm-fmt-check
```

## Fork tests

Fork suites live in `test/fork/` and extend `ForkTest`, which pins one block
per chain (`BASE_FORK_BLOCK`, `ETHEREUM_FORK_BLOCK`). Without the RPC
environment variable a suite is reported **skipped** by forge. It is never
reported as a pass. No RPC URL or key is committed. Set them in your shell or
in CI secrets:

| Variable | Used by |
|---|---|
| `BASE_RPC_URL` | Base fork suites (archive node needed for the pinned block) |
| `ETH_RPC_URL` | Ethereum fork suites |
| `FORK_BLOCK_BASE`, `FORK_BLOCK_ETHEREUM` | Optional local override of the pin |
