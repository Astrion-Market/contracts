# Changelog

## v0.1.0-rc.1: cross-chain alpha (unreleased)

Release candidate. **Not deployed to mainnet. No route enabled.** Evidence:
[`docs/evidence/INDEX.md`](docs/evidence/INDEX.md).

### Direction and safety
- C01 `599a46f`: Stellar → EVM lending architecture, ADR-0001, roadmap.
- C02 `aa4d832`, `11f6e97`: legacy Soroban engine frozen; deploys need an explicit generation and network; testnet CD no longer runs on push; review findings F1–F4 with reproduction tests.

### Spec, build and manifests
- C03 `a54eff8`: versioned intent, quote, status and receipt schemas with shared fixtures; `libs/crosschain-types` validator.
- C04 `34fc34f`: pinned Foundry workspace (solc 0.8.30, forge-std v1.17.0, OpenZeppelin v5.7.0); separate Rust and EVM CI; fork suites skip explicitly.
- C05 `ddcb9dc`: chain, USDC, CCTP and protocol manifests. All six routes disabled with reasons.

### Transport
- C06 `b171c75`: Rust and TypeScript CCTP codecs (strkeys, 7↔6 decimals, forwarder hooks, raw message decoding).
- C07 `4f370ee`: resumable bidirectional CCTP testnet harness (live runs pending).
- C10 `2417ef8`: CCTP receipt reconciliation with recoverable destination execution.
- C14 `629b53b`: return burns to Stellar via `CctpForwarder` with on-chain strkey validation.

### Accounts and intents
- C08 `aa4789d`: isolated, user-owned CREATE2 execution accounts.
- C09 `56e2deb`: EIP-712 intents, pinned module code hashes, exact approvals, recipient binding, route pauses (ADR-0002).

### Protocols
- C11 `08fa32f`: Aave V3 module and lens (Base fork tests).
- C12 `eac81ed`: Morpho Blue module and lens, pinned to full market params.
- C13 `98ffdbf`: Compound III module and lens (third protocol provisional).
- C15 `7ddca6c`: repay-available/repay-all, direct owner action path, lifecycle fork suites for all three.

### Product and operations
- C16 `fab28f4`: SDK: ABIs generated from build output, action encoders, intents, positions, capabilities, event store.
- C17 `efcdc16`: resumable relayer and indexer service.
- C18 `bccadc0`: cross-chain invariants and adversarial scenarios.
- C19 `a903e46`: manifest-driven deploy and smoke scripts, mainnet release-approval gate, route-gate report, runbooks.
- C20: contributor, security, audit scope, known limitations, work-package templates, evidence index.

### Fixes
- `353786b`: execution accounts can send plain ETH to EOAs.
