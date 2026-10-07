# Evidence index

What is merged, what has been tested and at which level, what depends on
outside parties, and what is future work. Evidence levels are defined in
[ROADMAP](../ROADMAP.md#evidence-levels). Commit count is never a measure of
progress.

## Merged functionality

See [`CHANGELOG.md`](../../CHANGELOG.md) for C01–C20 with commit hashes.

## Test results

Local validation of the release candidate (results recorded by the
validation commit that follows C20):

| Suite | Command | Level | Result |
|---|---|---|---|
| Rust workspace (legacy + crosschain-types) | `make test` | unit | _see validation commit_ |
| Spec / codec vectors (Rust) | `cargo test -p astrion-crosschain-types` | unit | _see validation commit_ |
| SDK | `make sdk-test` | unit | _see validation commit_ |
| Relayer (restart, reorg, property) | `make relayer-test` | unit | _see validation commit_ |
| Solidity unit, fuzz and invariants | `make evm-test` | unit | _see validation commit_ |
| Fork suites (Aave, Morpho, Compound, lifecycle, transport) | `make evm-fork-test` | fork | needs `BASE_RPC_URL`; reports SKIPPED without it |
| Route gates | `make route-gates` | fork | needs RPC |
| CCTP both directions | `docs/evidence/CCTP_TESTNET.md` | testnet | **not yet run** |

## Integration dependencies (outside this repo)

| Dependency | Needed for | Status |
|---|---|---|
| Circle CCTP V2 on Stellar and Base/Ethereum | transport | live per Circle docs (checked 2026-10-07); our live run pending |
| Archive RPC for Base/Ethereum | fork evidence | operator-provided |
| Funded testnet wallets | C07 evidence | operator-provided |
| Independent security review | any mainnet route | not started (W4) |
| Maintainer decision on the third protocol | Compound routes | pending |
| Approved Morpho markets | Morpho routes | pending |

## Future deliverables

Next-wave milestones W1–W6 in the [roadmap](../ROADMAP.md#next-wave-after-c20).
Known gaps: [`KNOWN_LIMITATIONS.md`](../KNOWN_LIMITATIONS.md).

Funding links and program details are added only after they are verified.
