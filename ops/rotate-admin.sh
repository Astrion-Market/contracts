#!/usr/bin/env bash
# Transfer admin rights to a new address on all Astrion contracts.
#
# Use this after mainnet launch to transfer control from the deployer key
# to a multisig wallet or governance contract.
#
# Both oracle-adapter and rate-model implement:
#   transfer_admin(env, new_admin: Address) — requires current admin auth
#
# Usage:
#   ops/rotate-admin.sh <new_admin_address> [network] [source]
#
# Example:
#   ops/rotate-admin.sh GCNEWADMIN... mainnet current-admin-key

set -euo pipefail

new_admin="${1:?Usage: ops/rotate-admin.sh <new_admin_address> [network] [source]}"
network="${2:-testnet}"
source_account="${3:-deployer}"

deploy_dir="${DEPLOY_DIR:-deployments/${network}}"
addresses_file="${deploy_dir}/addresses.env"

export DEPLOY_DIR="$deploy_dir"
source ops/state.sh

state_require_jq || exit 1

if [[ ! -f "$addresses_file" ]]; then
  echo "error: ${addresses_file} not found — run make deploy-all first." >&2
  exit 1
fi
set -a; source "$addresses_file"; set +a

# ─── validate new admin looks like a Stellar address ─────────────────────────
if [[ ! "$new_admin" =~ ^G[A-Z0-9]{55}$ ]]; then
  echo "error: '${new_admin}' does not look like a valid Stellar public key (G...)" >&2
  exit 1
fi

echo ""
echo "Admin Rotation"
echo "  Network:    ${network}"
echo "  New admin:  ${new_admin}"
echo "  Signer:     ${source_account}"
echo ""
echo "Contracts to rotate:"
for alias in oracle-adapter rate-model core-pool liquidation-engine market-factory; do
  id_var="${alias//-/_}_ID"
  id_var="${id_var^^}"
  id="${!id_var:-}"
  printf '  %-25s %s\n' "$alias" "${id:-NOT DEPLOYED}"
done
echo ""

read -r -p "Transfer admin on ALL contracts on ${network}? [y/N] " confirm
if [[ "${confirm,,}" != "y" ]]; then
  echo "Aborted."
  exit 0
fi

# ─── rotate helper ────────────────────────────────────────────────────────────
pass=0; fail=0

rotate() {
  local alias="$1" contract_id="$2"
  if [[ -z "$contract_id" ]]; then
    printf '  %-25s SKIP (not deployed)\n' "$alias"
    return
  fi
  if stellar contract invoke \
       --id      "$contract_id" \
       --network  "$network" \
       --source   "$source_account" \
       -- transfer_admin \
       --new_admin "$new_admin" &>/dev/null
  then
    printf '  %-25s \033[32m✓ transferred\033[0m\n' "$alias"
    state_set "$alias" "admin" "$new_admin"
    (( pass++ ))
  else
    printf '  %-25s \033[31m✗ FAILED\033[0m\n' "$alias"
    (( fail++ ))
  fi
}

echo "Rotating admin…"
rotate "oracle-adapter"     "${ORACLE_ADAPTER_ID:-}"
rotate "rate-model"         "${RATE_MODEL_ID:-}"
rotate "core-pool"          "${CORE_POOL_ID:-}"
rotate "liquidation-engine" "${LIQUIDATION_ENGINE_ID:-}"
rotate "market-factory"     "${MARKET_FACTORY_ID:-}"

echo ""
printf 'Done: %d succeeded, %d failed\n' "$pass" "$fail"

if (( fail > 0 )); then
  echo "WARNING: some rotations failed — verify admin state manually." >&2
  exit 1
fi

echo ""
echo "Admin is now: ${new_admin}"
echo "Run 'make verify' to confirm contracts are still responsive."
