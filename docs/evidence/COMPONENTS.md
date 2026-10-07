# Real deployed protocols vs. purpose-built test fixtures

Every claim about lending behaviour names which of these it was tested against.

| Component | In tests | Real or fixture |
|---|---|---|
| Aave V3 Pool (Base) | `evm/test/fork/aave`, lifecycle | **Real** bytecode on a Base fork at a pinned block |
| Morpho Blue (Base) | `evm/test/fork/morpho`, lifecycle | **Real** Morpho Blue bytecode. The **market** is a fixture we create (our `MockMorphoOracle` + `FixedRateIrm`, IRM enabled by pranking the owner on the fork) |
| Compound III cUSDCv3 (Base) | `evm/test/fork/compound`, lifecycle | **Real** bytecode on a Base fork |
| Circle TokenMessengerV2 (Base) | `evm/test/fork/transport` | **Real**. Skips if Stellar domain 27 is not registered at the pinned block |
| CCTP inbound MessageTransmitter | lifecycle suites | **Fixture** (`PrefundedMessageTransmitter`): pays pre-dealt real USDC and enforces `destinationCaller` and nonce reuse |
| CCTP outbound in lifecycle | lifecycle suites | **Fixture** (`MockTokenMessengerV2`): records burns |
| CCTP both directions end to end | `ops/crosschain/cctp-transfer.sh` | **Real** testnet (Stellar testnet ↔ Base Sepolia). Status: see `CCTP_TESTNET.md` |
| `MockLendingPool`, `MockPoolModule`, `MockERC20`, `MockMessageTransmitterV2` | unit and invariant suites | **Fixtures** only |
| Legacy Soroban `market`, `vault`, … | `cargo test` | Frozen legacy engine. Not part of the cross-chain path |

A testnet transport run and a fork lending run are separate evidence. They are
never presented as one production round trip.
