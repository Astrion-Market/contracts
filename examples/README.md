# Integration Examples

These snippets show the intended off-chain call flow. Replace placeholder IDs
with deployed contract addresses and sign with the caller's Stellar account.

## CorePool Supply

```bash
stellar contract invoke \
  --id <CORE_POOL_ID> \
  --network testnet \
  --source <USER> \
  -- supply \
  --supplier <USER_ADDRESS> \
  --asset <TOKEN_ID> \
  --amount 10000000
```

## CorePool Borrow

```bash
stellar contract invoke \
  --id <CORE_POOL_ID> \
  --network testnet \
  --source <USER> \
  -- borrow \
  --borrower <USER_ADDRESS> \
  --asset <TOKEN_ID> \
  --amount 5000000
```

## Keeper Liquidation With Limits

```bash
stellar contract invoke \
  --id <LIQUIDATION_ENGINE_ID> \
  --network testnet \
  --source <KEEPER> \
  -- liquidate_with_limits \
  --liquidator <KEEPER_ADDRESS> \
  --borrower <BORROWER_ADDRESS> \
  --debt_asset <DEBT_TOKEN_ID> \
  --collateral_asset <COLLATERAL_TOKEN_ID> \
  --repay_amount 1000000 \
  --max_collateral_seized 1200000 \
  --deadline 1893456000 \
  --nonce 1
```

## Oracle Bounds

```bash
stellar contract invoke \
  --id <ORACLE_ADAPTER_ID> \
  --network testnet \
  --source <ADMIN> \
  -- set_asset_bounds \
  --asset '{"Stellar":"<TOKEN_ID>"}' \
  --min_price_wad 900000000000000000 \
  --max_price_wad 1100000000000000000
```
