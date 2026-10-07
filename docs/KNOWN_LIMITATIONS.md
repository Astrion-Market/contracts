# Known limitations (alpha)

| Area | Limitation | Consequence | Tracked by |
|---|---|---|---|
| Review | No independent security review yet | No route may be enabled on mainnet | W4 |
| Transport evidence | Live Stellar ↔ Base Sepolia CCTP runs not yet executed | Hook endianness and Stellar argument names are confirmed by Circle docs only | `docs/evidence/CCTP_TESTNET.md`, C07 |
| Manifests | Aave/Compound/Morpho addresses are `pending` bytecode verification | All routes disabled | C05, `make crosschain-verify-manifest` |
| Third protocol | Compound III is provisional until confirmed by maintainers | `compound-v3` routes stay unavailable | ADR-0001 |
| Morpho | No Morpho Blue market approved; fork tests use a fixture market | Morpho routes unavailable | W2 |
| Ethereum | No Ethereum fork suites yet; Ethereum is gated behind Base | Ethereum routes unavailable | W6 |
| Wallet model | Dual wallet only (Stellar + EVM). No Stellar-only control | Users need an EVM wallet | Stellar-only extension |
| Collateral | Collateral must already be on EVM. No USDC→collateral swap | Borrowing from only Stellar USDC is not supported | W2 (optional) |
| Fees | Relayer fee is a cap the user signs. No on-chain fee quote | UI must show the quote and the cap | C16 SDK |
| Gas griefing | A relayer can under-fund gas so a funded action fails after the mint | Funds stay in the user's account; the action is retryable | ADR-0002 |
| Aave settings | eMode must be 0; isolation-mode collateral unsupported | Plans revert for such accounts | C11 |
| Legacy engine | Soroban engine has open findings F1–F4 | Frozen; not part of the cross-chain path | `docs/legacy/REVIEW_FINDINGS.md` |
| CI | Pre-existing clippy errors and a raw `cargo build --target wasm32v1-none` step fail on the current toolchain | CI red until fixed | C20 notes |
