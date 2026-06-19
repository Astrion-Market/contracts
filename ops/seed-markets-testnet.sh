#!/usr/bin/env bash
# Seed Morpho isolated markets on testnet with borrowable liquidity so users can
# lend and borrow immediately. Run AFTER deploy-morpho-testnet.sh.
#
# What it does (as the admin / steins-testnet key):
#   1. Mints USDC + WBTC to the admin (so the admin can lend).
#   2. Ensures both market directions exist:
#        M1  collateral WBTC / loan USDC   (borrow USDC against WBTC)  — from deploy
#        M2  collateral USDC / loan WBTC   (borrow WBTC against USDC)  — created here
#   3. Supplies loan-asset liquidity into each market (admin acts as the first lender).
#
# Usage:
#   SOURCE=steins-testnet ops/seed-markets-testnet.sh
#
# Idempotent-ish: minting and supplying are additive; market creation is skipped
# if the reverse market already exists.

set -euo pipefail

network="${NETWORK:-testnet}"
source_account="${SOURCE:-steins-testnet}"
deploy_dir="${DEPLOY_DIR:-deployments/${network}}"

set -a; source "${deploy_dir}/addresses.env"; set +a
admin="$(stellar keys address "$source_account")"
inv() { stellar -q contract invoke --network "$network" --source "$source_account" "$@"; }

# ─── amounts (7-decimal tokens) ──────────────────────────────────────────────
USDC_MINT=1500000000000   # 150,000 USDC
WBTC_MINT=100000000       #      10 WBTC
USDC_SUPPLY=1000000000000 # 100,000 USDC lent into M1
WBTC_SUPPLY=50000000      #       5 WBTC lent into M2
LLTV_70=700000000000000000
LIQ_BONUS=50000000000000000
RESERVE_FACTOR=100000000000000000

echo "Admin: $admin"

# ─── 1. mint admin liquidity ─────────────────────────────────────────────────
echo "── minting admin liquidity ──"
inv --id "$TEST_USDC_ID" -- mint --account "$admin" --amount "$USDC_MINT" >/dev/null
inv --id "$TEST_WBTC_ID" -- mint --account "$admin" --amount "$WBTC_MINT" >/dev/null
echo "  minted 150,000 USDC + 10 WBTC to admin"

# ─── 2. ensure reverse market (collateral USDC / loan WBTC) ───────────────────
echo "── ensuring reverse market (USDC collateral / WBTC loan) ──"
market_count="$(inv --id "$MARKET_FACTORY_ID" -- get_markets | tr ',' '\n' | grep -c 'C' || echo 0)"
if [[ "$market_count" -ge 2 ]]; then
  echo "  reverse market already present (markets=$market_count), skipping create"
else
  rev_cfg="{\"collateral_asset\":\"${TEST_USDC_ID}\",\"loan_asset\":\"${TEST_WBTC_ID}\",\"oracle_adapter\":\"${ORACLE_ADAPTER_ID}\",\"lltv\":\"${LLTV_70}\",\"liquidation_bonus\":\"${LIQ_BONUS}\",\"reserve_factor\":\"${RESERVE_FACTOR}\",\"supply_cap\":\"0\",\"borrow_cap\":\"0\",\"rate_model\":\"${RATE_MODEL_ID}\",\"treasury\":\"${admin}\"}"
  REVERSE_MARKET_ID="$(inv --id "$MARKET_FACTORY_ID" -- create_market --config "$rev_cfg" | tr -d '"' | tail -n1)"
  echo "  reverse market: $REVERSE_MARKET_ID"
  grep -q '^REVERSE_MARKET_ID=' "${deploy_dir}/addresses.env" || \
    printf 'REVERSE_MARKET_ID=%q\n' "$REVERSE_MARKET_ID" >> "${deploy_dir}/addresses.env"
fi

# ─── 3. supply loan liquidity into each market ───────────────────────────────
# Re-read markets so we have both addresses regardless of create/skip path.
mapfile -t MARKETS < <(inv --id "$MARKET_FACTORY_ID" -- get_markets | tr -d '[]"' | tr ',' '\n' | sed '/^$/d')

echo "── supplying loan liquidity ──"
for m in "${MARKETS[@]}"; do
  loan="$(inv --id "$m" -- get_market_params | sed -n 's/.*"loan_asset":"\([^"]*\)".*/\1/p')"
  if [[ "$loan" == "$TEST_USDC_ID" ]]; then
    amount="$USDC_SUPPLY"; label="100,000 USDC"
  elif [[ "$loan" == "$TEST_WBTC_ID" ]]; then
    amount="$WBTC_SUPPLY"; label="5 WBTC"
  else
    echo "  ${m:0:8}…: unknown loan asset, skipping"; continue
  fi
  # supply(supplier, assets, on_behalf)
  inv --id "$m" -- supply --supplier "$admin" --assets "$amount" --on_behalf "$admin" >/dev/null
  echo "  ${m:0:8}…: supplied $label"
done

echo ""
echo "Seed complete. Markets now hold borrowable liquidity."
for m in "${MARKETS[@]}"; do
  echo "  $m"
  inv --id "$m" -- get_market_state | sed 's/^/    /'
done
