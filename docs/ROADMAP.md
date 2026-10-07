# Roadmap

Direction: Stellar users reach Aave V3, Morpho Blue and (provisionally)
Compound III on Base and Ethereum, with native USDC moved by Circle CCTP. See
[ADR-0001](adr/0001-crosschain-lending-direction.md) and the
[architecture](architecture/crosschain.md).

Success is measured by merged, tested functionality and published evidence,
never by commit count. A route is "supported" only when its release gate has
evidence; disabled routes stay visible with a reason.

## Phase 1: product and API agreement

- C01 Architecture, scope and ADR (this document set).
- C02 Quarantine legacy Soroban deployments; record review findings.
- C03 Versioned intent, quote, status and receipt schemas in `spec/`.
- C04 Pinned Foundry workspace under `evm/`; Rust and EVM CI run separately.
- C05 Chain, token and protocol manifests in `deployments/crosschain/`, all
  routes disabled by default.

## Phase 2: transport proof

- C06 Stellar CCTP codecs (7↔6 decimal conversion, recipients, hook data).
- C07 Reproducible bidirectional CCTP testnet harness (Stellar ↔ Base Sepolia).
- C08 Isolated, user-owned EVM execution accounts.
- C09 Bounded signed intents and pinned protocol modules.
- C10 CCTP receipt reconciliation with recoverable destination execution.

## Phase 3: lending proof

- C11 Aave V3, C12 Morpho Blue, C13 Compound III modules with fork tests.
- C14 Return path: EVM withdrawals and borrow proceeds to Stellar.
- C15 Repayment and unwind across all adapters, plus direct EVM fallback.

## Phase 4: observable alpha and handoff

- C16 SDK events and position read models.
- C17 Resumable relayer and indexer service.
- C18 Cross-chain failure invariants and adversarial tests.
- C19 Reproducible alpha deployments and route release gates.
- C20 Contributor work packages and alpha evidence.

## Evidence levels

| Level | Means | Does not mean |
|---|---|---|
| Unit / mock | Logic is correct against our own fixtures | Anything about real protocols or bridges |
| Fork | Behaviour against real protocol bytecode at a pinned block | That a bridge transfer arrives |
| Testnet | A real CCTP transfer on test networks, with tx hashes | Mainnet liquidity, fees or protocol state |
| Production | Reviewed, explicitly released, capped mainnet route | That every other route is ready |

A testnet bridge run plus a fork lending test are two separate pieces of
evidence and are never presented as one production round trip.

## Not planned for the first release

Pooled cross-chain collateral, automatic leverage, novel debt receipts,
Stellar-only signing (separate extension with its own review), Aave V4, Morpho
Midnight, or every Morpho vault generation.
