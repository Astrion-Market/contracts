# Audit RFP Outline

## Scope

- `contracts/core-pool`
- `contracts/market`
- `contracts/market-factory`
- `contracts/liquidation-engine`
- `contracts/oracle-adapter`
- `contracts/interest-rate-model`
- `libs/math`

## Focus Areas

- Authorization and role separation.
- Upgrade and migration safety.
- Oracle sanity and stale data handling.
- Interest accrual and fixed-point rounding.
- Health factor and liquidation correctness.
- Storage TTL and footprint risks.
- Event compatibility for indexers.

## Required Deliverables

- Severity-ranked findings.
- Exploitability notes and proof-of-concept tests where applicable.
- Remediation review after fixes.
- Final public report redacted only for live exploit risk.

## Materials To Provide

- `SECURITY_CHECKLIST.md`
- `ADVANCED_SECURITY_PLAN.md`
- `FORMAL_SPEC.md`
- Latest commit hash.
- Test and build commands.
- Known limitations and unresolved design questions.
