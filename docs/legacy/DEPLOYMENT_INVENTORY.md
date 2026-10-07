# Legacy Soroban deployment inventory

The Soroban lending engine (isolated `market`, `vault`, adapters, factories, and
the older `core-pool` / `liquidation-engine`) is **frozen**. It is not the
cross-chain product ([ADR-0001](../adr/0001-crosschain-lending-direction.md)),
and it must never be deployed or upgraded implicitly from the new release path.

## Recorded deployments

Source: `deployments/testnet/state.json` (network `testnet`, deployer alias
`steins-testnet`, last updated 2026-06-19T07:07:45Z). The `reverse market`
`CCADAHEHDOQZXKZ6LVIWHDJW5MQL7NILNPRVJVB5KEKWT56TY2CEL6WS` is recorded only in
`addresses.env` (created by `ops/seed-markets-testnet.sh`).

| Alias | Contract | Status | Deployed |
|---|---|---|---|
| `oracle-adapter` | `CACJW5GN3RDF5LH3HZNYFJHLY5B257E2TPYMHFMCDFERL7E3WXNRK7QO` | initialized | 2026-06-19T06:54:38Z |
| `rate-model` | `CBUDQ3AVT4KLA4RGIN5A5PBCBKHUKJ2Z6LI3ANIPJHVWOE54BPM3WTUV` | initialized | 2026-06-19T06:54:49Z |
| `core-pool` | `CDWDB7LNEW3SR42PUCB7KMLC6A3V6K2J6MG6ETEFHQ3ZO5EU4FVENJJ2` | initialized | 2026-06-19T06:54:59Z |
| `liquidation-engine` | `CD3LP3GPNSV2WROGZQ5JLIRW7UXKVPQGAGUFPXNNPG3OXGYMIJ3RYXPZ` | initialized | 2026-06-19T06:55:07Z |
| `market-factory` | `CBM4BW772GFRAKX233ZJHGTAWN3WC4WPWGJIUJAO66B67NR7PYZ4PCWH` | initialized | 2026-06-19T06:55:18Z |
| `vault-factory` | `CCGA3ZPHRLPSVVH6JD7KC27HYWRP6KDHK6Z2G7XHJHUJU4ILX4VPRWBQ` | initialized | 2026-06-19T06:55:32Z |
| `adapter-registry` | `CAG67HPHYQW6LUTQRURE36SCWDPDQCCTRFAFWGO7LOHDTX5S3IYCJECI` | initialized | 2026-06-19T06:55:43Z |
| `mock-oracle` | `CCKZQRVYXA7C66LJ6FAPKWTAB3RZTRM5S2RL473ZZ73LQYKKEJONMINQ` | initialized | 2026-06-19T06:55:53Z |
| `test-usdc` | `CDZ4L3GZH4TGMOQC7XXPO3IKYABJM2FB2OGYXLZ7SPFFHM5HCLME3J7D` | initialized | 2026-06-19T06:56:03Z |
| `test-wbtc` | `CCBD6JIWJDHWSURR3NU42QRIWHGUHF4XLMPES7PI6RLTQUCEBG5MVP6T` | initialized | 2026-06-19T06:56:13Z |
| `demo-market` | `CCKXGK4SE3XW5M4MRRX3NKV5UOTQ57V73OVYUEFHAQ2GJOCYNTW36MRH` | initialized | - |
| `demo-vault` | `CDBJHSHCWGZ3DXRBL6K4IWP5BGWBIOU5PWBNLVAUT24RZNKZHTUFNO3A` | initialized | - |
| `market-adapter` | `CCHVHZFO5U74DT4JE2GHTFPKPWI5FIJ4IABRNVFB2OBGTR4DOZGD6YID` | initialized | - |

**Mainnet:** no `deployments/mainnet/state.json` exists in this repository; only
`config.env.example`. That shows no mainnet deployment was recorded *here*. It
does not prove none exists; confirm with the operators before any claim.

## Balances: not yet inventoried

Recorded state is not live state. Testnet-oriented configuration does not mean
"no users". Before any retirement, upgrade or migration, take a live snapshot:

```bash
make legacy-inventory NETWORK=testnet
```

This simulates read-only calls (`--send=no`) and writes
`deployments/testnet/legacy-inventory-<timestamp>.md` with market totals, vault
totals and test-token balances held by each contract. Status: **pending**.

## Deployment guard

Every script that writes to a network with legacy contracts
(`deploy-all`, `init-all`, `upgrade-all`, `upgrade-contract`, `rotate-admin`,
`deploy-morpho-testnet`, `seed-markets-testnet`, `make deploy-one`) sources
`ops/lib/legacy-guard.sh` and refuses to run unless both are explicit:

```bash
make deploy-all GENERATION=legacy-soroban NETWORK=testnet
# or
ASTRION_GENERATION=legacy-soroban ASTRION_TARGET_NETWORK=testnet ops/deploy-all.sh testnet deployer
```

A `NETWORK` taken only from the Makefile default is not forwarded, so it does
not satisfy the guard. `DRYRUN=1` needs neither.

GitHub Actions: `cd-testnet.yml` **no longer runs on push to `main`**. Both CD
workflows are manual dispatch only and require `generation=legacy-soroban`
(testnet also requires typing `testnet`).

## Existing-position exit access

No legacy contract is paused, upgraded or disabled by this change. Holders keep
the same entrypoints:

| Contract | Exit path |
|---|---|
| `market` | `repay`, `withdraw`, `withdraw_collateral` (health-checked) |
| `vault` | `withdraw`, `redeem` (subject to idle liquidity; `force_deallocate` exists) |
| `core-pool` | its repay/withdraw entrypoints, unchanged |

Any future retirement must keep these paths callable, must follow a live
inventory, and must not automate an unreviewed migration.

Known source-level findings: [REVIEW_FINDINGS.md](REVIEW_FINDINGS.md).
