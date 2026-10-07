# ADR-0001: Stellar cross-chain lending direction

- Status: accepted (planning)
- Date: 2026-10-07

## Context

The repository holds a Soroban lending engine (isolated markets, vaults, legacy
shared pool). The product direction is now to give Stellar users access to
established EVM lending markets rather than to grow a new Stellar lending
protocol. Stellar already has lending (Blend); the thesis is broader access and
a usable cross-chain experience.

## Decisions

1. **Transport: Circle CCTP, native USDC only.** CCTP moves USDC; it grants no
   authority over any lending position. Axelar GMP/ITS is a later candidate for
   authenticated remote commands, not used in the alpha. No other bridge is
   advertised without a validated route.
2. **Inbound to Stellar uses Circle's on-chain `CctpForwarder`.** `mintRecipient`
   and `destinationCaller` are set as Circle documents, with the final strkey in
   hook data. Stellar USDC has 7 decimals and CCTP amounts have 6; the 7th-decimal
   remainder stays on Stellar. Circle's hosted Forwarding Service is not
   available for Stellar, so Astrion submits transactions or users self-submit.
3. **Chain IDs and CCTP domains are separate fields.** Ethereum `1`/domain `0`,
   Base `8453`/domain `6`, Stellar domain `27` plus network passphrase. Mainnet
   and testnet manifests are separate.
4. **Ownership: one user-owned EVM execution account per scope**
   `(owner, chain, protocol, marketScope, version)`. Each account owns its own
   collateral, supply claims and debt. No shared router owns any position. The
   owner can always act directly, without Astrion's UI, relayer or registry.
5. **Execution model: no arbitrary delegatecall.** Protocol calls run in the
   account's own context under target/selector constraints; any module code is
   pinned by code hash. Final details are settled in C09's ADR.
6. **Alpha wallet model: dual wallet.** The user signs with a Stellar wallet for
   source transfers and a user-controlled EVM wallet for account actions. Wallet
   linking alone grants no authority. Stellar-only control is a separate,
   separately reviewed extension.
7. **Pinned protocol versions:** Aave V3, Morpho Blue, Compound III (third
   protocol provisional until confirmed). Base first, Ethereum second. Only
   explicitly approved markets, never "every permissionless market".
8. **Both directions are first-class.** Stellar → EVM (lend, repay, fund) and
   EVM → Stellar (withdraw or borrow proceeds home). A transfer never "moves" a
   position or a debt.
9. **Legacy Soroban engine is frozen**, not deleted, and is excluded from the new
   release path. Existing positions keep repay/withdraw access.

## Consequences

- New code lives in new paths (`evm/`, `spec/`, `libs/crosschain-types/`,
  `deployments/crosschain/`, `ops/crosschain/`, `sdk/`, `services/`); existing
  Rust workspace paths stay stable.
- Application pauses can block new risk but never owner recovery, repayment or
  protocol-valid exits.
- Bridge latency is never treated as a liquidation buffer; debt and risk are
  re-read immediately before execution.
