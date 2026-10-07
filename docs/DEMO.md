# Reproducible demos

Each demo states its evidence level.

## 1. Accounts, intents and funded execution (unit; no network)

```bash
make evm-deps evm-test
```

Shows scoped accounts, signed intents with every rejection case, CCTP receipt
reconciliation with recoverable mints, the return burn, direct owner exits and
the invariant suite.

## 2. Lending lifecycles on real protocols (fork; needs an archive RPC)

```bash
BASE_RPC_URL=https://... make evm-fork-test
```

Aave V3, Morpho Blue (fixture market on real Morpho) and Compound III on Base
at the pinned block. Steps: collateral, borrow, proceeds burned toward
Stellar, interest during transit, Stellar-funded repayment, payoff, collateral
back, residual USDC returned.

## 3. CCTP transport (testnet; needs funded test wallets)

See [`docs/evidence/CCTP_TESTNET.md`](evidence/CCTP_TESTNET.md):
`make cctp-stellar-to-evm`, `make cctp-evm-to-stellar`, `make cctp-evidence`.

## 4. Codecs and SDK (unit)

```bash
make crosschain-test sdk-test relayer-test
```

The same vectors run in Rust, TypeScript and Solidity.
