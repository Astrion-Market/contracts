# Security policy

Astrion is **pre-audit alpha software**. No route is enabled on mainnet. Do
not deposit funds you cannot afford to lose on any deployment.

## Reporting a vulnerability

Report privately through GitHub Security Advisories on this repository
("Report a vulnerability"). Do not open a public issue for anything that
could put funds at risk. Include affected files and commit, impact, and a
reproduction (a Foundry or `cargo` test is ideal). We acknowledge within 3
business days.

## Scope

In scope: `evm/src/**`, `libs/crosschain-types/**`, `sdk/src/**`,
`services/relayer/src/**`, `ops/crosschain/**`, and the manifests in
`deployments/crosschain/`. The audit scope with its priorities is in
[`docs/AUDIT_SCOPE.md`](docs/AUDIT_SCOPE.md).

Out of scope: Aave, Morpho, Compound and Circle CCTP contracts themselves
(report those to their teams). Also out of scope are the frozen legacy Soroban
engine's known findings ([`docs/legacy/REVIEW_FINDINGS.md`](docs/legacy/REVIEW_FINDINGS.md)),
and test fixtures under `evm/test/`.

## Trust model in one paragraph

Each user owns an isolated EVM account per scope. Only the owner, directly or
through an EIP-712 intent pinned to a module code hash, can move its funds.
The relayer pays gas and can only submit what the owner signed. The route
guardian can pause new risk but cannot move funds or block owner exits.
Circle CCTP and the destination protocols are trusted dependencies. Details:
[ADR-0002](docs/adr/0002-account-execution-model.md),
[known limitations](docs/KNOWN_LIMITATIONS.md).
