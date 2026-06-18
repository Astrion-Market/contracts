#!/usr/bin/env bash
# Astrion testnet simulation — one round.
#
# Each round rotates wallet roles using a 5-position cycle:
#
#   Position 0: supplier-A     (primary depositor)
#   Position 1: supplier-B     (secondary depositor)
#   Position 2: borrower-A     (primary borrower)
#   Position 3: borrower-B     (secondary borrower)
#   Position 4: liquidator     (checks health + attempts liquidation)
#
# For round R, wallet test-N takes role:  (N-1 + R-1) % 5
#
# Usage:
#   sim/run.sh [network] [addresses_env]
#   make sim-run
#   make sim-loop ROUNDS=10 DELAY=30

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$(dirname "$SCRIPT_DIR")"   # repo root

network="${1:-testnet}"
addresses_env="${2:-deployments/${network}/addresses.env}"
sim_dir="sim"

wallets_env="${sim_dir}/wallets.env"
tokens_env="${sim_dir}/tokens.env"
state_env="${sim_dir}/state.env"

# ─── pre-flight ───────────────────────────────────────────────────────────────
for f in "$wallets_env" "$tokens_env" "$state_env"; do
  if [[ ! -f "$f" ]]; then
    echo "error: ${f} not found — run make sim-setup first" >&2
    exit 1
  fi
done
if [[ ! -f "$addresses_env" ]]; then
  echo "error: ${addresses_env} not found — run make deploy-all first" >&2
  exit 1
fi

set -a
source "$wallets_env"
source "$tokens_env"
source "$addresses_env"
source "$state_env"
set +a

# ─── round counter ────────────────────────────────────────────────────────────
SIM_ROUND=$((SIM_ROUND + 1))
sed -i "s/^SIM_ROUND=.*/SIM_ROUND=${SIM_ROUND}/" "$state_env"

OFFSET=$(( (SIM_ROUND - 1) % 5 ))

# Map role index → wallet name
role_wallet() {
  local role_idx=$1
  local wallet_idx=$(( (role_idx - OFFSET + 5) % 5 + 1 ))
  echo "test${wallet_idx}"
}

SUPPLIER_A="$(role_wallet 0)"
SUPPLIER_B="$(role_wallet 1)"
BORROWER_A="$(role_wallet 2)"
BORROWER_B="$(role_wallet 3)"
LIQUIDATOR="$(role_wallet 4)"

# Resolve address for a wallet name
addr_of() {
  local w="$1"
  local var="${w//-/_}"
  var="${var^^}_ADDRESS"
  echo "${!var}"
}

SUPPLIER_A_ADDR="$(addr_of "$SUPPLIER_A")"
SUPPLIER_B_ADDR="$(addr_of "$SUPPLIER_B")"
BORROWER_A_ADDR="$(addr_of "$BORROWER_A")"
BORROWER_B_ADDR="$(addr_of "$BORROWER_B")"
LIQUIDATOR_ADDR="$(addr_of "$LIQUIDATOR")"

# ─── banner ───────────────────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
printf '║  Astrion Simulation  Round %-34s║\n' "${SIM_ROUND}"
printf '║  Network: %-51s║\n' "$network"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
printf '  %-14s → %s\n' "supplier-A"  "$SUPPLIER_A (${SUPPLIER_A_ADDR:0:8}…)"
printf '  %-14s → %s\n' "supplier-B"  "$SUPPLIER_B (${SUPPLIER_B_ADDR:0:8}…)"
printf '  %-14s → %s\n' "borrower-A"  "$BORROWER_A (${BORROWER_A_ADDR:0:8}…)"
printf '  %-14s → %s\n' "borrower-B"  "$BORROWER_B (${BORROWER_B_ADDR:0:8}…)"
printf '  %-14s → %s\n' "liquidator"  "$LIQUIDATOR (${LIQUIDATOR_ADDR:0:8}…)"
echo ""

# ─── helpers ──────────────────────────────────────────────────────────────────

invoke() {
  stellar contract invoke --network "$network" --source "$1" --id "$2" --resource-fee 5000000 -- "${@:3}"
}

invoke_ro() {
  # use deployer as source so test wallets' seq nums stay clean for real ops
  stellar contract invoke --network "$network" --source "deployer" --id "$1" --resource-fee 5000000 -- "${@:2}"
}

section() { printf '\n\033[1m%s\033[0m\n' "$1"; }

ok()   { printf '  ✓  %s\n' "$1"; }
skip() { printf '  ─  %s (scaffold — skipped)\n' "$1"; }
warn() { printf '  ⚠  %s\n' "$1"; }
fail() { printf '  ✗  %s\n' "$1"; }

# ─── 1. oracle health check ───────────────────────────────────────────────────
section "1. Oracle health"

if [[ -n "${ORACLE_ADAPTER_ID:-}" ]]; then
  if stellar_out="$(invoke_ro "$ORACLE_ADAPTER_ID" admin 2>&1)"; then
    ok "oracle-adapter admin: ${stellar_out}"
  else
    warn "oracle-adapter admin invocation failed"
  fi

  if staleness_out="$(invoke_ro "$ORACLE_ADAPTER_ID" default_max_staleness 2>&1)"; then
    ok "oracle-adapter max_staleness: ${staleness_out}"
  else
    warn "oracle-adapter staleness check failed"
  fi
else
  warn "ORACLE_ADAPTER_ID not set — skipping oracle checks"
fi

# ─── 2. interest rate curve ───────────────────────────────────────────────────
section "2. Interest rate curve"

if [[ -n "${RATE_MODEL_ID:-}" ]]; then
  UTIL_POINTS=("0" "200000000000000000" "500000000000000000" "800000000000000000" "950000000000000000")
  UTIL_LABELS=("0%" "20%" "50%" "80% (kink)" "95%")

  for i in "${!UTIL_POINTS[@]}"; do
    rate="$(
      invoke_ro "$RATE_MODEL_ID" get_borrow_rate \
        --utilization_wad "${UTIL_POINTS[$i]}" 2>/dev/null || echo "ERR"
    )"
    if [[ "$rate" == "ERR" ]]; then
      warn "rate at ${UTIL_LABELS[$i]}: invocation failed"
    else
      # Convert WAD to percentage: rate / 1e16 = basis points → %
      pct="$(awk "BEGIN { printf \"%.2f%%\", $rate / 10000000000000000 }" 2>/dev/null || echo "${rate}")"
      ok "borrow rate @ ${UTIL_LABELS[$i]}: ${pct}"
    fi
  done
else
  warn "RATE_MODEL_ID not set — skipping rate checks"
fi

# ─── 3. wallet USDC balances ──────────────────────────────────────────────────
section "3. Wallet balances (USDC)"

if [[ -n "${USDC_TOKEN_ID:-}" ]]; then
  for w in test1 test2 test3 test4 test5; do
    waddr="$(addr_of "$w")"
    bal="$(
      invoke_ro "$USDC_TOKEN_ID" balance --account "$waddr" 2>/dev/null || echo "ERR"
    )"
    if [[ "$bal" == "ERR" ]]; then
      warn "${w}: balance check failed"
    else
      human="$(awk "BEGIN { printf \"%.2f\", $bal / 10000000 }" 2>/dev/null || echo "$bal")"
      ok "${w}: ${human} USDC"
    fi
  done
else
  warn "USDC_TOKEN_ID not set — skipping balance checks"
fi

# ─── 4. wallet WBTC balances ─────────────────────────────────────────────────
section "4. Wallet balances (WBTC)"

if [[ -n "${WBTC_TOKEN_ID:-}" ]]; then
  for w in test1 test2 test3 test4 test5; do
    waddr="$(addr_of "$w")"
    bal="$(
      invoke_ro "$WBTC_TOKEN_ID" balance --account "$waddr" 2>/dev/null || echo "ERR"
    )"
    if [[ "$bal" == "ERR" ]]; then
      warn "${w}: balance check failed"
    else
      human="$(awk "BEGIN { printf \"%.8f\", $bal / 10000000 }" 2>/dev/null || echo "$bal")"
      ok "${w}: ${human} WBTC"
    fi
  done
else
  warn "WBTC_TOKEN_ID not set — skipping WBTC balance checks"
fi

# ─── 5. supply (CorePool) ─────────────────────────────────────────────────────
section "5. Supply — ${SUPPLIER_A} + ${SUPPLIER_B} (USDC liquidity)"

# 100 USDC = 1_000_000_000 (7 decimals)
SUPPLY_USDC=1000000000

if [[ -n "${CORE_POOL_ID:-}" && -n "${USDC_TOKEN_ID:-}" ]]; then
  for w in "$SUPPLIER_A" "$SUPPLIER_B"; do
    waddr="$(addr_of "$w")"
    if invoke "$w" "$CORE_POOL_ID" supply \
         --supplier "$waddr" \
         --asset    "$USDC_TOKEN_ID" \
         --amount   "$SUPPLY_USDC" \
         2>/dev/null
    then
      ok "${w} supplied 100 USDC to CorePool"
    else
      skip "CorePool.supply (USDC)"
      break
    fi
  done
else
  skip "CorePool supply (CORE_POOL_ID or USDC_TOKEN_ID not set)"
fi

# ─── 5b. collateral deposit ───────────────────────────────────────────────────
section "5b. Collateral — ${BORROWER_A} + ${BORROWER_B} (WBTC → CorePool)"

# 0.1 WBTC = 1_000_000 (7 decimals) → $6,000 collateral at $60k/BTC
# Enables borrowing up to $4,200 USDC at 70% LTV
COLLATERAL_WBTC=1000000

if [[ -n "${CORE_POOL_ID:-}" && -n "${WBTC_TOKEN_ID:-}" ]]; then
  for w in "$BORROWER_A" "$BORROWER_B"; do
    waddr="$(addr_of "$w")"
    if invoke "$w" "$CORE_POOL_ID" supply \
         --supplier "$waddr" \
         --asset    "$WBTC_TOKEN_ID" \
         --amount   "$COLLATERAL_WBTC" \
         2>/dev/null
    then
      ok "${w} supplied 0.1 WBTC as collateral"
    else
      skip "CorePool.supply (WBTC collateral)"
      break
    fi
  done
else
  skip "CorePool WBTC collateral (CORE_POOL_ID or WBTC_TOKEN_ID not set)"
fi

# ─── 6. borrow (CorePool) ────────────────────────────────────────────────────
section "6. Borrow — ${BORROWER_A} + ${BORROWER_B} (60 USDC each)"

# 60 USDC = 600_000_000 (7 decimals)
# Well within 70% LTV on $6,000 WBTC collateral ($4,200 max borrowable)
BORROW_USDC=600000000

if [[ -n "${CORE_POOL_ID:-}" && -n "${USDC_TOKEN_ID:-}" ]]; then
  for w in "$BORROWER_A" "$BORROWER_B"; do
    waddr="$(addr_of "$w")"
    if invoke "$w" "$CORE_POOL_ID" borrow \
         --borrower "$waddr" \
         --asset    "$USDC_TOKEN_ID" \
         --amount   "$BORROW_USDC" \
         2>/dev/null
    then
      ok "${w} borrowed 60 USDC from CorePool"
    else
      skip "CorePool.borrow (not yet implemented)"
      break
    fi
  done
else
  skip "CorePool borrow (CORE_POOL_ID or USDC_TOKEN_ID not set)"
fi

# ─── 7. check liquidation eligibility ────────────────────────────────────────
section "7. Liquidation check — ${LIQUIDATOR}"

if [[ -n "${LIQUIDATION_ENGINE_ID:-}" && -n "${CORE_POOL_ID:-}" ]]; then
  BORROWER_A_ADDR_FULL="$(addr_of "$BORROWER_A")"
  if hf_out="$(invoke_ro "$CORE_POOL_ID" get_health_factor \
       --user "$BORROWER_A_ADDR_FULL" 2>/dev/null)"
  then
    # get_health_factor returns WAD; 1e18 = fully healthy
    hf_pct="$(awk "BEGIN { printf \"%.4f\", $hf_out / 1000000000000000000 }" 2>/dev/null || echo "$hf_out")"
    ok "${BORROWER_A} health factor: ${hf_pct}"

    # If health factor < 1.0 (1e18), attempt liquidation
    if awk "BEGIN { exit !($hf_out < 1000000000000000000) }"; then
      ok "  ${BORROWER_A} is undercollateralized — attempting liquidation…"
      REPAY_AMOUNT=$(( BORROW_USDC / 2 ))
      if invoke "$LIQUIDATOR" "$LIQUIDATION_ENGINE_ID" liquidate \
           --liquidator      "$LIQUIDATOR_ADDR" \
           --borrower        "$BORROWER_A_ADDR_FULL" \
           --debt_asset      "$USDC_TOKEN_ID" \
           --collateral_asset "$USDC_TOKEN_ID" \
           --repay_amount    "$REPAY_AMOUNT" \
           2>/dev/null
      then
        ok "  Liquidation executed by ${LIQUIDATOR}"
      else
        skip "  liquidate (not yet implemented)"
      fi
    else
      ok "  ${BORROWER_A} is healthy — no liquidation needed"
    fi
  else
    skip "CorePool.get_health_factor (no supply yet)"
  fi
else
  skip "Liquidation checks (LIQUIDATION_ENGINE_ID or CORE_POOL_ID not set)"
fi

# ─── 8. core-pool state ───────────────────────────────────────────────────────
section "8. CorePool state"

if [[ -n "${CORE_POOL_ID:-}" ]]; then
  if markets_out="$(invoke_ro "$CORE_POOL_ID" get_markets 2>/dev/null)"; then
    ok "core-pool markets: ${markets_out}"
  else
    skip "core-pool.get_markets (no markets configured yet)"
  fi

  if [[ -n "${USDC_TOKEN_ID:-}" ]]; then
    if state_out="$(invoke_ro "$CORE_POOL_ID" get_market_state \
         --asset "$USDC_TOKEN_ID" 2>/dev/null)" && [[ "$state_out" != "null" && -n "$state_out" ]]
    then
      total_supply_raw="$(echo "$state_out" | grep -o '"total_scaled_supply":[^,}]*' | grep -o '[0-9-]*' || echo 0)"
      ok "core-pool USDC market state: ${state_out}"
    else
      skip "core-pool USDC market state (market not configured yet)"
    fi
  fi
else
  skip "CorePool state (CORE_POOL_ID not set)"
fi

# ─── round summary ────────────────────────────────────────────────────────────
echo ""
echo "════════════════════════════════════════════════════════════════"
printf 'Round %d complete  |  Network: %s  |  %s\n' \
  "$SIM_ROUND" "$network" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo ""
echo "Role assignments next round:"
next_offset=$(( SIM_ROUND % 5 ))
NEXT_WALLETS=()
for i in 0 1 2 3 4; do
  idx=$(( (i - next_offset + 5) % 5 + 1 ))
  NEXT_WALLETS+=("test${idx}")
done
printf '  supplier-A → %s   supplier-B → %s   borrower-A → %s   borrower-B → %s   liquidator → %s\n' \
  "${NEXT_WALLETS[0]}" "${NEXT_WALLETS[1]}" "${NEXT_WALLETS[2]}" "${NEXT_WALLETS[3]}" "${NEXT_WALLETS[4]}"
echo ""
