# Cross-chain deployment manifests

`mainnet.json` and `testnet.json` are the single source of truth for chains,
canonical USDC, Circle CCTP contracts and lending-protocol routes. They are kept
separate on purpose: a testnet process can never resolve a mainnet write
target (enforced by `libs/crosschain-types/tests/manifests.rs`).

## Verification status

Every address-bearing record carries `verification`:

| `status` | Meaning |
|---|---|
| `source-documented` | Copied from the issuer's official page (`sourceUrl`) on `checkedAt`. Not yet checked on-chain |
| `pending` | Taken from the protocol's address registry; bytecode and parameters not yet checked |
| `bytecode-verified` | `ops/crosschain/verify-manifest.sh --write` found code at every address; `codeHash` recorded |

Code being present is necessary, not sufficient. Reserve configuration, caps,
pause state, oracle, LLTV, liquidity and market approval are reviewed by a person
before a route moves to `verified` and only then may it be enabled.

## Routes

All six protocol/chain combinations exist in each environment and **all are
disabled** (`"enabled": false`) with a visible `reason`. Current state:

| Route | Mainnet | Testnet |
|---|---|---|
| Aave V3 · Base | pending verification | unavailable (no Circle-USDC reserve) |
| Aave V3 · Ethereum | pending verification (gated behind Base) | unavailable |
| Morpho Blue · Base | unavailable (no market approved) | unavailable |
| Morpho Blue · Ethereum | unavailable | unavailable |
| Compound III · Base | unavailable (third protocol provisional) | unavailable |
| Compound III · Ethereum | unavailable | unavailable |

Testnet lending is covered by fork tests against mainnet bytecode. Testnet is
used for the **transport** (real CCTP transfers), not for lending.

## Native USDC vs USDbC

Base lists `USDbC` (`0xd9aA…b6CA`) under `nonCanonicalUsdc` only, so tools can
recognise and reject it. It can never appear in a route.

## Commands

```bash
make crosschain-verify-manifest ENV=mainnet          # report only
make crosschain-verify-manifest ENV=mainnet WRITE=1  # record code hashes
```

RPC URLs come from the environment only (`BASE_RPC_URL`, `ETH_RPC_URL`,
`BASE_SEPOLIA_RPC_URL`, `ETH_SEPOLIA_RPC_URL`).
