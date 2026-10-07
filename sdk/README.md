# @astrion/crosschain-sdk

TypeScript SDK for Astrion's Stellar ↔ EVM lending routes (spec v1). It shares
test vectors with `libs/crosschain-types` and the Solidity tests.

| Module | Provides |
|---|---|
| `cctp`, `strkey` | Dependency-free CCTP codecs: strkeys, 7↔6 decimals, forwarder hooks, raw message decoding, burn builders |
| `abi/generated` | ABIs (and account creation code) generated from `evm/out`. Never copied by hand |
| `actions` | Action encoders for the Aave V3, Morpho Blue, Compound III and CCTP return modules |
| `intent` | `ExecutionIntent` builder, EIP-712 typed data and digest, `transferId`, CREATE2 account prediction |
| `positions` | Lens readers returning normalized positions with protocol-specific risk, source block and freshness |
| `capabilities` | Route availability from `deployments/crosschain/*.json`. A route is usable only when enabled **and** verified |
| `events` | Account event decoding plus an idempotent, reorg-aware `OperationStore` |
| `contractErrors` | Revert data → stable error codes |

```bash
make sdk-abi    # regenerate src/abi/generated.ts after `make evm-build`
make sdk-test
```
