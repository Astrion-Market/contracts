# Evidence index

What is merged, what has been tested and at which level, what depends on
outside parties, and what is future work. Evidence levels are defined in
[ROADMAP](../ROADMAP.md#evidence-levels). Commit count is never a measure of
progress.

## Merged functionality

See [`CHANGELOG.md`](../../CHANGELOG.md) for C01–C20 with commit hashes.

## Test results

Local validation of the release candidate on 2026-10-07 (Foundry 1.7.1,
solc 0.8.30, Rust 1.97.1, Bun 1.4.2):

| Suite | Command | Level | Result |
|---|---|---|---|
| Rust workspace (legacy + crosschain-types) | `make test` | unit | 20 suites pass; 5 legacy finding reproductions `#[ignore]`d and confirmed failing |
| Spec / codec / manifest vectors (Rust) | `cargo test -p astrion-crosschain-types` | unit | 16 / 16 |
| SDK | `make sdk-test` | unit | 20 / 20 (codecs 11, SDK 9) |
| EIP-712 digest, viem vs Solidity | `IntentDigestVector.t.sol` | unit | match (`0xaef62e…6670`) |
| Relayer (restart, reorg, budgets, API, 200-seed property) | `make relayer-test` | unit | 12 / 12 |
| Solidity unit, fuzz and 7 invariants | `make evm-test` | unit | 67 / 67 |
| Fork suites (Aave, Morpho, Compound, lifecycle, transport) | `make evm-fork-test` | fork | **9 suites SKIPPED**: no `BASE_RPC_URL` in the validation environment |
| Route gates | `make route-gates` | fork | not yet generated (needs RPC) |
| CCTP both directions | `docs/evidence/CCTP_TESTNET.md` | testnet | **not yet run** |

Known red CI steps that predate this work: clippy on legacy contracts under
the current toolchain, and a raw `cargo build --workspace --target
wasm32v1-none` step (soroban-sdk requires `stellar contract build`).

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
