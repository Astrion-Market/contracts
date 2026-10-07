# Maintainers and ownership

| Area | Paths | Owner | Reviewer for security changes |
|---|---|---|---|
| Architecture, ADRs, releases | `docs/`, `CHANGELOG.md`, `deployments/crosschain/release-approval*.json` | @0xsteins | second maintainer (open) |
| EVM accounts and intents | `evm/src/account`, `evm/src/libraries` | @0xsteins | independent reviewer (W4) |
| Protocol modules | `evm/src/modules`, `evm/src/lens` | @0xsteins | open: one integrator per protocol |
| Codecs and spec | `spec/`, `libs/crosschain-types`, `sdk/src/cctp.ts` | @0xsteins | open: Stellar engineer |
| SDK | `sdk/` | @0xsteins | open |
| Relayer and indexer | `services/relayer` | @0xsteins | open: backend engineer |
| Ops and manifests | `ops/crosschain`, `deployments/crosschain` | @0xsteins | open |
| Legacy Soroban engine (frozen) | `contracts/`, `libs/math`, `libs/market-types` | @0xsteins | n/a (exit access only) |

"Open" seats are offered to contributors who complete a work package in that
area. Until filled, security-relevant PRs wait for the W4 independent review.
