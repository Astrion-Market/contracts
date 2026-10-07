# Astrion contracts

**Astrion is an open-source interface and execution layer that lets Stellar users
reach lending markets on Base and Ethereum, moving native USDC between chains with
Circle CCTP.**

The destination protocol owns collateral accounting, interest, debt and
liquidation. Astrion owns route composition, user authorization, position
visibility, transaction recovery and the Stellar experience.

> **Status: pre-alpha, pre-audit.** No cross-chain route is enabled. Nothing here
> is production software. Every route is gated per market and per action; see
> [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Scope

| | |
|---|---|
| Transport | Circle CCTP, native USDC only (Stellar domain 27, Ethereum domain 0, Base domain 6) |
| Execution chains | Base first, Ethereum second |
| Protocols | Aave V3, Morpho Blue, Compound III (provisional); pinned versions only |
| Alpha wallet model | Stellar wallet **plus** a user-controlled EVM wallet |
| Position ownership | One user-owned EVM execution account per `(owner, chain, protocol, marketScope, version)` |

Out of scope for the first release: pooled cross-chain collateral, a new
liquidation engine, mirrored debt tokens, admin sweeping of user funds, automatic
leverage, Stellar-only signing (a separate, separately reviewed extension).

## Documents

- [Architecture: Stellar cross-chain lending](docs/architecture/crosschain.md):
  custody, authority, debt, collateral, fees and recovery for each journey.
- [ADR-0001: cross-chain lending direction](docs/adr/0001-crosschain-lending-direction.md)
- [ADR-0002: execution accounts and bounded intents](docs/adr/0002-account-execution-model.md)
- [Roadmap](docs/ROADMAP.md) · [Changelog](CHANGELOG.md) · [Evidence index](docs/evidence/INDEX.md)
- [Reproducible demos](docs/DEMO.md) · [Known limitations](docs/KNOWN_LIMITATIONS.md)
- [Security policy](SECURITY.md) · [Audit scope](docs/AUDIT_SCOPE.md) · [Maintainers](docs/MAINTAINERS.md)
- Ops: [alpha deployment](docs/ops/ALPHA_DEPLOYMENT.md), [incidents](docs/ops/INCIDENT_RUNBOOK.md), [direct owner actions](docs/ops/DIRECT_OWNER_ACTIONS.md)
- [Legacy Soroban lending engine (frozen)](docs/legacy/SOROBAN_ENGINE.md)
- [Contributing](docs/CONTRIBUTING.md) · [Security checklist](docs/SECURITY_CHECKLIST.md)

## Repository layout

```
contracts/      Soroban contracts of the legacy lending engine (frozen, see below)
libs/           Rust libraries (math, market types, crosschain-types)
deployments/    Per-network deployment state (crosschain/ manifests planned)
ops/            Deployment and operations scripts
mk/             Makefile fragments (all build/test/deploy targets)
sim/            Legacy testnet simulation harness
docs/           Architecture, ADRs, roadmap, security, legacy docs
```

Cross-chain layer: `evm/` (Foundry accounts, modules, lenses, scripts),
`spec/` (versioned schemas and shared vectors), `libs/crosschain-types/`,
`sdk/` (TypeScript), `services/relayer/`, `ops/crosschain/`,
`deployments/crosschain/`.

## The legacy Soroban lending engine

This repository previously shipped a Morpho Blue–style isolated lending engine
for Soroban (`market`, `vault`, adapters, factories) and an older shared pool
(`core-pool`, `liquidation-engine`). That code, its tests and its history are
kept for research and for anyone holding a testnet position. **It is not the new
product and is not part of the cross-chain production path.** Known review
findings are tracked before any reuse. Full documentation:
[`docs/legacy/SOROBAN_ENGINE.md`](docs/legacy/SOROBAN_ENGINE.md).

## Build and test

```bash
make build                 # Soroban WASM
make test                  # Rust workspace tests
make evm-deps evm-test     # Solidity unit, fuzz, invariants
make sdk-abi sdk-test      # TypeScript SDK
make relayer-test          # relayer service
make evm-fork-test         # needs BASE_RPC_URL
```

## License

MIT, Copyright (c) 2026 Astrion Labs
