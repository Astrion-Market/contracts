#!/usr/bin/env bash
# Post-deployment verification for Astrion contracts.
#
# Checks:
#   1. Each contract is recorded in state.json
#   2. WASM sha256 matches the on-chain bytecode (via stellar contract fetch)
#   3. Health-check invocations confirm contracts are initialized and responsive
#
# Usage:
#   ops/verify-deploy.sh [network] [source]
#   make verify NETWORK=testnet

set -euo pipefail

network="${1:-testnet}"
source_account="${2:-deployer}"
deploy_dir="${DEPLOY_DIR:-deployments/${network}}"
addresses_file="${deploy_dir}/addresses.env"

export DEPLOY_DIR="$deploy_dir"
source ops/state.sh

# ─── counters ─────────────────────────────────────────────────────────────────
pass=0; fail=0; warn=0

check_pass() { printf '  %-35s \033[32m✓ PASS\033[0m  %s\n' "$1" "${2:-}"; pass=$((pass+1)); }
check_fail() { printf '  %-35s \033[31m✗ FAIL\033[0m  %s\n' "$1" "${2:-}"; fail=$((fail+1)); }
check_warn() { printf '  %-35s \033[33m⚠ WARN\033[0m  %s\n' "$1" "${2:-}"; warn=$((warn+1)); }

# ─── pre-flight ───────────────────────────────────────────────────────────────
state_require_jq || exit 1
if [[ ! -f "$addresses_file" ]]; then
  echo "error: ${addresses_file} not found — run make deploy-all first." >&2
  exit 1
fi
set -a; source "$addresses_file"; set +a

# ─── helpers ──────────────────────────────────────────────────────────────────

# Verify the on-chain WASM sha256 matches the stored build fingerprint.
# Falls back to a warning if stellar-contract-fetch is unavailable.
check_wasm_hash() {
  local alias="$1" contract_id="$2"
  local key="wasm-hash:${alias}"

  local expected
  expected="$(state_get "$alias" "wasm_sha256")"
  if [[ -z "$expected" ]]; then
    check_warn "$key" "not in state — redeploy with new deploy-all.sh to pin"
    return
  fi

  local tmp_wasm; tmp_wasm="$(mktemp /tmp/astrion-verify-XXXXXX.wasm)"
  trap 'rm -f "$tmp_wasm"' RETURN

  if stellar contract fetch \
       --id      "$contract_id" \
       --network "$network" \
       --out     "$tmp_wasm" \
       2>/dev/null
  then
    local actual
    actual="$(sha256sum "$tmp_wasm" | awk '{print $1}')"
    if [[ "$actual" == "$expected" ]]; then
      check_pass "$key" "${actual:0:20}…"
    else
      check_fail "$key" "expected ${expected:0:16}… got ${actual:0:16}…"
    fi
  else
    check_warn "$key" "stellar contract fetch unavailable — stored: ${expected:0:20}…"
  fi
}

# Try a contract invocation and report FAIL on error.
invoke_strict() {
  local alias="$1" contract_id="$2"
  shift 2
  local key="${alias}:$*"
  if stellar contract invoke \
       --id     "$contract_id" \
       --network "$network" \
       --source  "$source_account" \
       -- "$@" &>/dev/null
  then
    check_pass "$key"
  else
    check_fail "$key" "invocation failed"
  fi
}

# Try a contract invocation and report WARN on error (for scaffold contracts).
invoke_soft() {
  local alias="$1" contract_id="$2"
  shift 2
  local key="${alias}:$*"
  if stellar contract invoke \
       --id      "$contract_id" \
       --network  "$network" \
       --source   "$source_account" \
       -- "$@" &>/dev/null
  then
    check_pass "$key"
  else
    check_warn "$key" "invocation failed (contract may be scaffold)"
  fi
}

# ─── banner ───────────────────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  Astrion Deployment Verification                            ║"
printf '║  Network: %-51s║\n' "$network"
echo "╚══════════════════════════════════════════════════════════════╝"

# ─── oracle-adapter (production) ─────────────────────────────────────────────
echo ""
printf '\033[1moracle-adapter\033[0m  %s\n' "${ORACLE_ADAPTER_ID:-NOT DEPLOYED}"
if [[ -z "${ORACLE_ADAPTER_ID:-}" ]]; then
  check_fail "oracle-adapter:deployed" "no contract ID in addresses.env"
else
  check_wasm_hash "oracle-adapter" "$ORACLE_ADAPTER_ID"
  invoke_strict "oracle-adapter" "$ORACLE_ADAPTER_ID" admin
  invoke_strict "oracle-adapter" "$ORACLE_ADAPTER_ID" default_max_staleness
fi

# ─── rate-model (production) ─────────────────────────────────────────────────
echo ""
printf '\033[1mrate-model\033[0m      %s\n' "${RATE_MODEL_ID:-NOT DEPLOYED}"
if [[ -z "${RATE_MODEL_ID:-}" ]]; then
  check_fail "rate-model:deployed" "no contract ID in addresses.env"
else
  check_wasm_hash "rate-model" "$RATE_MODEL_ID"
  invoke_strict "rate-model" "$RATE_MODEL_ID" admin
  invoke_strict "rate-model" "$RATE_MODEL_ID" config
  invoke_strict "rate-model" "$RATE_MODEL_ID" \
    get_borrow_rate --utilization_wad 500000000000000000
fi

# ─── core-pool (scaffold) ─────────────────────────────────────────────────────
echo ""
printf '\033[1mcore-pool\033[0m       %s\n' "${CORE_POOL_ID:-NOT DEPLOYED}"
if [[ -z "${CORE_POOL_ID:-}" ]]; then
  check_fail "core-pool:deployed" "no contract ID in addresses.env"
else
  check_wasm_hash "core-pool" "$CORE_POOL_ID"
  invoke_soft "core-pool" "$CORE_POOL_ID" admin
fi

# ─── liquidation-engine (scaffold) ───────────────────────────────────────────
echo ""
printf '\033[1mliquidation-engine\033[0m  %s\n' "${LIQUIDATION_ENGINE_ID:-NOT DEPLOYED}"
if [[ -z "${LIQUIDATION_ENGINE_ID:-}" ]]; then
  check_fail "liquidation-engine:deployed" "no contract ID in addresses.env"
else
  check_wasm_hash "liquidation-engine" "$LIQUIDATION_ENGINE_ID"
  invoke_soft "liquidation-engine" "$LIQUIDATION_ENGINE_ID" admin
fi

# ─── market (scaffold) ───────────────────────────────────────────────────────
echo ""
printf '\033[1mmarket\033[0m          %s\n' "${MARKET_ID:-NOT DEPLOYED}"
if [[ -z "${MARKET_ID:-}" ]]; then
  check_fail "market:deployed" "no contract ID in addresses.env"
else
  check_wasm_hash "market" "$MARKET_ID"
fi

# ─── market-factory (scaffold) ───────────────────────────────────────────────
echo ""
printf '\033[1mmarket-factory\033[0m  %s\n' "${MARKET_FACTORY_ID:-NOT DEPLOYED}"
if [[ -z "${MARKET_FACTORY_ID:-}" ]]; then
  check_fail "market-factory:deployed" "no contract ID in addresses.env"
else
  check_wasm_hash "market-factory" "$MARKET_FACTORY_ID"
  invoke_soft "market-factory" "$MARKET_FACTORY_ID" admin
fi

# ─── test tokens (non-mainnet only) ──────────────────────────────────────────
if [[ "$network" != "mainnet" ]]; then
  for _tok in "test-usdc:TEST_USDC_ID" "test-wbtc:TEST_WBTC_ID"; do
    _alias="${_tok%%:*}"
    _var="${_tok##*:}"
    _id="${!_var:-}"

    echo ""
    printf '\033[1m%s\033[0m  %s\n' "$_alias" "${_id:-NOT DEPLOYED}"
    if [[ -z "$_id" ]]; then
      check_fail "${_alias}:deployed" "no contract ID in addresses.env"
    else
      check_wasm_hash "$_alias" "$_id"
      invoke_strict "$_alias" "$_id" name
      invoke_strict "$_alias" "$_id" symbol
      invoke_strict "$_alias" "$_id" decimals
      invoke_strict "$_alias" "$_id" owner
    fi
  done
fi

# ─── summary ─────────────────────────────────────────────────────────────────
echo ""
echo "────────────────────────────────────────────────────────────────"
printf 'Results: \033[32m%d passed\033[0m  \033[33m%d warnings\033[0m  \033[31m%d failed\033[0m\n' \
  "$pass" "$warn" "$fail"
echo ""

if (( fail > 0 )); then
  echo "VERIFICATION FAILED — investigate failures before proceeding." >&2
  exit 1
fi

echo "All checks passed."
