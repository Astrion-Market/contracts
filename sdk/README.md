# @astrion/crosschain-sdk

Dependency-free TypeScript codecs for Astrion's Stellar <-> EVM USDC routes
(spec v1). It mirrors `libs/crosschain-types`, and both run the same vectors
in `spec/fixtures/cctp/vectors.json`.

- Stellar strkeys (G / C / M), strict decoding with checksum
- 7 ↔ 6 decimal conversion with retained dust, bounded by i64::MAX
- `CctpForwarder` hook data (inbound to Stellar)
- Raw CCTP V2 message decoding (for when API address fields are null)
- Builders for Stellar `deposit_for_burn` and EVM `depositForBurnWithHook`
  that reject malformed destinations before anything is signed

```bash
make sdk-test   # bun test
```
