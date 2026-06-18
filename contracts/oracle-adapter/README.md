# Oracle Adapter

The oracle adapter wraps a SEP-0402 compatible Reflector oracle and returns
WAD-normalized prices with staleness checks already applied.

## Example Init

```bash
stellar contract invoke \
  --id <ORACLE_ADAPTER_ID> \
  --network testnet \
  --source <SOURCE_ACCOUNT> \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --default_oracle <REFLECTOR_ORACLE_ID> \
  --default_max_staleness 300
```

Per-asset overrides can use a different oracle or a tighter freshness window:

```bash
stellar contract invoke \
  --id <ORACLE_ADAPTER_ID> \
  --network testnet \
  --source <SOURCE_ACCOUNT> \
  -- set_asset_oracle \
  --asset '{"Stellar":"<TOKEN_ID>"}' \
  --oracle <ASSET_ORACLE_ID> \
  --max_staleness 120
```

## Price Sanity Bounds

Admins can configure optional WAD-normalized bounds to reject obvious outliers
after decimal normalization:

```bash
stellar contract invoke \
  --id <ORACLE_ADAPTER_ID> \
  --network testnet \
  --source <SOURCE_ACCOUNT> \
  -- set_asset_bounds \
  --asset '{"Stellar":"<TOKEN_ID>"}' \
  --min_price_wad 900000000000000000 \
  --max_price_wad 1100000000000000000
```

Use `0` for either side to disable that side of the check.
