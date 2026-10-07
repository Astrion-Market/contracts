#!/usr/bin/env bash
# Read-only inventory of the frozen legacy Soroban lending deployment.
#
# Records, per network, every contract in state.json plus the on-chain state
# that decides whether retirement is safe: market totals, vault totals, and the
# token balances each contract holds. Nothing is signed or submitted
# (`--send=no` simulation only).
#
# Usage:
#   ops/legacy-inventory.sh [network] [source]
#   make legacy-inventory NETWORK=testnet
#
# Output: deployments/{network}/legacy-inventory-<UTC timestamp>.md

set -euo pipefail

network="${1:-testnet}"
source_account="${2:-deployer}"
deploy_dir="${DEPLOY_DIR:-deployments/${network}}"
state_file="${deploy_dir}/state.json"
addresses_file="${deploy_dir}/addresses.env"

command -v jq >/dev/null || { echo "error: jq is required" >&2; exit 1; }
command -v stellar >/dev/null || { echo "error: stellar CLI is required" >&2; exit 1; }
[[ -f "$state_file" ]] || { echo "error: ${state_file} not found" >&2; exit 1; }

if [[ -f "$addresses_file" ]]; then
  set -a; source "$addresses_file"; set +a
fi

view() {
  # Simulation only; prints "unavailable" instead of failing the inventory.
  stellar -q contract invoke --network "$network" --source "$source_account" \
    --send=no --id "$1" -- "${@:2}" 2>/dev/null || echo "unavailable"
}

balance_of() {
  local token="$1" holder="$2"
  [[ -n "$token" ]] || { echo "n/a"; return; }
  view "$token" balance --id "$holder"
}

out="${deploy_dir}/legacy-inventory-$(date -u +%Y%m%d-%H%M%S).md"
usdc="${TEST_USDC_ID:-}"
wbtc="${TEST_WBTC_ID:-}"

{
  echo "# Legacy Soroban deployment inventory (${network})"
  echo ""
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ) · commit \`$(git rev-parse --short HEAD 2>/dev/null || echo unknown)\`"
  echo ""
  echo "Read-only simulation. \`unavailable\` means the call failed (archived"
  echo "entry, missing method, or RPC error) and must be checked by hand before"
  echo "any retirement decision."
  echo ""
  echo "## Contracts"
  echo ""
  echo "| Alias | Contract | Status | test-USDC balance | test-WBTC balance |"
  echo "|---|---|---|---|---|"
  while IFS=$'\t' read -r alias id status; do
    echo "| \`${alias}\` | \`${id}\` | ${status} | $(balance_of "$usdc" "$id") | $(balance_of "$wbtc" "$id") |"
  done < <(jq -r '.contracts | to_entries[] | [.key, (.value.contract_id // "-"), (.value.status // "-")] | @tsv' "$state_file")
  echo ""

  echo "## Isolated markets"
  echo ""
  markets=()
  [[ -n "${MARKET_FACTORY_ID:-}" ]] && \
    mapfile -t markets < <(view "$MARKET_FACTORY_ID" get_markets | jq -r '.[]?' 2>/dev/null || true)
  [[ -n "${DEMO_MARKET_ID:-}" ]] && markets+=("$DEMO_MARKET_ID")
  [[ -n "${REVERSE_MARKET_ID:-}" ]] && markets+=("$REVERSE_MARKET_ID")
  for m in $(printf '%s\n' "${markets[@]}" | sort -u); do
    echo "### \`${m}\`"
    echo ""
    echo '```json'
    view "$m" get_market_state
    echo '```'
    echo ""
  done

  echo "## Vaults"
  echo ""
  if [[ -n "${DEMO_VAULT_ID:-}" ]]; then
    echo "- \`${DEMO_VAULT_ID}\` total_assets=$(view "$DEMO_VAULT_ID" total_assets) total_supply=$(view "$DEMO_VAULT_ID" total_supply)"
  else
    echo "- none recorded"
  fi
  echo ""

  echo "## Legacy core pool"
  echo ""
  if [[ -n "${CORE_POOL_ID:-}" ]]; then
    echo "- \`${CORE_POOL_ID}\` markets: $(view "$CORE_POOL_ID" get_markets)"
  else
    echo "- none recorded"
  fi
} > "$out"

echo "Inventory written → ${out}"
