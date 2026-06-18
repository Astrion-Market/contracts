#!/usr/bin/env bash
# Upgrade all deployed Astrion contracts to the current build.
#
# Flow:
#   1. Build all contracts
#   2. Compare local WASM sha256 against state.json for each deployed contract
#   3. Print a diff table — show what changed vs what's already current
#   4. Single confirmation prompt
#   5. Upgrade only the contracts with changed WASMs (in dependency order)
#   6. Summary
#
# Contracts with an identical WASM sha256 are silently skipped.
# Run `make status` after to confirm the new hashes are recorded.
#
# Usage:
#   ops/upgrade-all.sh [network] [source]
#   make upgrade-all

set -euo pipefail

network="${1:-testnet}"
source_account="${2:-deployer}"
wasm_dir="${WASM_DIR:-target/wasm32v1-none/release}"
deploy_dir="${DEPLOY_DIR:-deployments/${network}}"
addresses_file="${deploy_dir}/addresses.env"

export DEPLOY_DIR="$deploy_dir"
source ops/state.sh

state_require_jq || exit 1

if [[ ! -f "$addresses_file" ]]; then
  echo "error: ${addresses_file} not found — run make deploy-all first." >&2
  exit 1
fi

# ─── contract list (dependency order) ────────────────────────────────────────
ALIASES=(
  "oracle-adapter"
  "rate-model"
  "core-pool"
  "liquidation-engine"
  "market"
  "market-factory"
)
if [[ "$network" != "mainnet" ]]; then
  ALIASES+=("test-usdc" "test-wbtc")
fi

wasm_stem_of() {
  case "$1" in
    oracle-adapter)      echo "oracle_adapter"      ;;
    rate-model)          echo "interest_rate_model"  ;;
    core-pool)           echo "core_pool"            ;;
    liquidation-engine)  echo "liquidation_engine"   ;;
    market)              echo "market"               ;;
    market-factory)      echo "market_factory"       ;;
    test-usdc|test-wbtc) echo "test_token"           ;;
  esac
}

# ─── banner ───────────────────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  Astrion Contract Upgrade                                   ║"
printf '║  Network: %-51s║\n' "$network"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# ─── build ────────────────────────────────────────────────────────────────────
echo "Building contracts…"
stellar contract build
echo ""

# ─── diff table ───────────────────────────────────────────────────────────────
echo "Comparing deployed vs local WASM…"
echo ""
printf '  %-22s  %-18s  %-18s  %s\n' \
  "CONTRACT" "DEPLOYED SHA256" "LOCAL SHA256" "ACTION"
printf '  %-22s  %-18s  %-18s  %s\n' \
  "──────────────────────" "──────────────────" "──────────────────" "──────────────"

TO_UPGRADE=()

for alias in "${ALIASES[@]}"; do
  status="$(state_status "$alias" 2>/dev/null || echo "pending")"
  if [[ "$status" == "pending" || "$status" == "failed" ]]; then
    printf '  %-22s  %-18s  %-18s  \033[2m─ not deployed\033[0m\n' "$alias" "" ""
    continue
  fi

  stem="$(wasm_stem_of "$alias")"
  wasm_path="${wasm_dir}/${stem}.wasm"
  [[ -f "${wasm_dir}/${stem}.optimized.wasm" ]] && wasm_path="${wasm_dir}/${stem}.optimized.wasm"

  if [[ ! -f "$wasm_path" ]]; then
    printf '  %-22s  %-18s  %-18s  \033[31m✗ WASM missing\033[0m\n' "$alias" "" ""
    continue
  fi

  old_sha="$(state_get "$alias" "wasm_sha256" 2>/dev/null || echo "")"
  new_sha="$(sha256sum "$wasm_path" | awk '{print $1}')"

  if [[ "$new_sha" == "$old_sha" ]]; then
    printf '  %-22s  %s…  %s…  \033[32m✓ up to date\033[0m\n' \
      "$alias" "${old_sha:0:14}" "${new_sha:0:14}"
  else
    printf '  %-22s  %s…  %s…  \033[33m↑ upgrade\033[0m\n' \
      "$alias" "${old_sha:0:14}" "${new_sha:0:14}"
    TO_UPGRADE+=("$alias")
  fi
done

echo ""

# ─── early exit if nothing to do ──────────────────────────────────────────────
if [[ ${#TO_UPGRADE[@]} -eq 0 ]]; then
  echo "All deployed contracts are already at the current build. Nothing to do."
  echo ""
  exit 0
fi

printf '%d contract(s) queued for upgrade:\n' "${#TO_UPGRADE[@]}"
for a in "${TO_UPGRADE[@]}"; do printf '  • %s\n' "$a"; done
echo ""
echo "Addresses are preserved — only the on-chain WASM is replaced."
echo ""

read -r -p "Upgrade on ${network}? [y/N] " confirm
if [[ "${confirm,,}" != "y" ]]; then
  echo "Aborted. No changes made."
  exit 0
fi

# ─── upgrade loop ─────────────────────────────────────────────────────────────
upgraded=0
failed=0

for alias in "${TO_UPGRADE[@]}"; do
  echo ""
  echo "────────────────────────────────────────────────────────────────"
  printf 'Upgrading %s…\n' "$alias"
  if FORCE=1 WASM_DIR="$wasm_dir" DEPLOY_DIR="$deploy_dir" \
       ops/upgrade-contract.sh "$alias" "$network" "$source_account"
  then
    upgraded=$((upgraded+1))
  else
    echo "FAILED: ${alias}" >&2
    failed=$((failed+1))
  fi
done

# ─── summary ─────────────────────────────────────────────────────────────────
echo ""
echo "════════════════════════════════════════════════════════════════"
printf 'Results: \033[32m%d upgraded\033[0m  \033[31m%d failed\033[0m  \033[2m%d skipped\033[0m\n' \
  "$upgraded" "$failed" "$(( ${#ALIASES[@]} - upgraded - failed ))"
echo ""
state_print

if (( failed > 0 )); then
  echo "Some upgrades failed — investigate before proceeding." >&2
  exit 1
fi
