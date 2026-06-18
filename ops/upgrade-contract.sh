#!/usr/bin/env bash
# Upgrade a deployed Astrion contract to a new WASM build.
#
# Steps:
#   1. Build (or reuse) the contract WASM
#   2. Upload the new WASM to the network (stellar contract upload)
#   3. Call the contract's `upgrade(new_wasm_hash)` function
#   4. Update state.json with the new WASM hash
#   5. Run a quick health check to confirm the upgrade succeeded
#
# Both oracle-adapter and rate-model expose:
#   upgrade(env, new_wasm_hash: BytesN<32>) — requires admin auth
#
# Usage:
#   ops/upgrade-contract.sh <contract-alias> [network] [source]
#
# Example:
#   ops/upgrade-contract.sh oracle-adapter testnet deployer

set -euo pipefail

contract_alias="${1:?Usage: ops/upgrade-contract.sh <alias> [network] [source]}"
network="${2:-testnet}"
source_account="${3:-deployer}"

wasm_dir="${WASM_DIR:-target/wasm32v1-none/release}"
deploy_dir="${DEPLOY_DIR:-deployments/${network}}"
addresses_file="${deploy_dir}/addresses.env"

export DEPLOY_DIR="$deploy_dir"
source ops/state.sh

state_require_jq || exit 1

# ─── map alias → WASM stem & contract ID var ─────────────────────────────────
case "$contract_alias" in
  oracle-adapter)     wasm_stem="oracle_adapter";      id_var="ORACLE_ADAPTER_ID" ;;
  rate-model|\
  interest-rate-model) wasm_stem="interest_rate_model"; id_var="RATE_MODEL_ID"     ;;
  core-pool)          wasm_stem="core_pool";           id_var="CORE_POOL_ID"       ;;
  liquidation-engine) wasm_stem="liquidation_engine";  id_var="LIQUIDATION_ENGINE_ID" ;;
  market)             wasm_stem="market";              id_var="MARKET_ID"          ;;
  market-factory)     wasm_stem="market_factory";      id_var="MARKET_FACTORY_ID"  ;;
  mock-oracle)        wasm_stem="mock_oracle";          id_var="MOCK_ORACLE_ID"      ;;
  test-usdc)          wasm_stem="test_token";           id_var="TEST_USDC_ID"        ;;
  test-wbtc)          wasm_stem="test_token";           id_var="TEST_WBTC_ID"        ;;
  *)
    echo "error: unknown contract alias '${contract_alias}'" >&2
    echo "       choose from: oracle-adapter, rate-model, core-pool, liquidation-engine, market, market-factory, mock-oracle, test-usdc, test-wbtc" >&2
    exit 1
    ;;
esac

# ─── resolve contract ID ──────────────────────────────────────────────────────
if [[ ! -f "$addresses_file" ]]; then
  echo "error: ${addresses_file} not found — run make deploy-all first." >&2
  exit 1
fi
set -a; source "$addresses_file"; set +a

contract_id="${!id_var:-}"
if [[ -z "$contract_id" ]]; then
  echo "error: ${id_var} not set in ${addresses_file}" >&2
  exit 1
fi

# ─── resolve WASM path ───────────────────────────────────────────────────────
wasm_path="${wasm_dir}/${wasm_stem}.wasm"
if [[ -f "${wasm_dir}/${wasm_stem}.optimized.wasm" ]]; then
  wasm_path="${wasm_dir}/${wasm_stem}.optimized.wasm"
fi
if [[ ! -f "$wasm_path" ]]; then
  echo "WASM not found at ${wasm_path}. Building…"
  cargo build -p "$contract_alias" --target wasm32v1-none --release
fi

new_sha256="$(sha256sum "$wasm_path" | awk '{print $1}')"
old_sha256="$(state_get "$contract_alias" "wasm_sha256" || echo "unknown")"

echo ""
echo "Upgrading: ${contract_alias}"
echo "  Contract ID:  ${contract_id}"
echo "  WASM:         ${wasm_path}"
echo "  Old sha256:   ${old_sha256:0:20}…"
echo "  New sha256:   ${new_sha256:0:20}…"
echo ""

if [[ "$new_sha256" == "$old_sha256" ]]; then
  echo "New WASM is identical to the currently deployed version. Nothing to do."
  exit 0
fi

# FORCE=1 skips the prompt — used by upgrade-all.sh which confirms once for all.
if [[ "${FORCE:-0}" != "1" ]]; then
  read -r -p "Confirm upgrade on ${network}? [y/N] " confirm
  if [[ "${confirm,,}" != "y" ]]; then
    echo "Aborted."
    exit 0
  fi
fi

# ─── 1. upload new WASM ───────────────────────────────────────────────────────
echo "Uploading WASM to ${network}…"
new_wasm_hash="$(
  stellar -q contract upload \
    --wasm    "$wasm_path" \
    --network "$network" \
    --source  "$source_account" \
    | tail -n 1
)"

if [[ -z "$new_wasm_hash" ]]; then
  echo "error: failed to upload WASM" >&2
  exit 1
fi
echo "Uploaded. WASM hash: ${new_wasm_hash}"

# ─── 2. call upgrade on the contract ─────────────────────────────────────────
echo "Calling upgrade…"
stellar contract invoke \
  --id     "$contract_id" \
  --network "$network" \
  --source  "$source_account" \
  -- upgrade \
  --new_wasm_hash "$new_wasm_hash"

# ─── 3. update state ──────────────────────────────────────────────────────────
state_set "$contract_alias" "wasm_sha256"  "$new_sha256"
state_set "$contract_alias" "wasm_file"    "$(basename "$wasm_path")"
state_set "$contract_alias" "upgraded_at"  "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

echo ""
echo "Upgrade complete. Verifying…"

# ─── 4. quick health check ───────────────────────────────────────────────────
if stellar contract invoke \
     --id     "$contract_id" \
     --network "$network" \
     --source  "$source_account" \
     -- admin &>/dev/null
then
  echo "Health check passed — contract is responsive after upgrade."
else
  echo "WARNING: health check failed after upgrade. Investigate immediately." >&2
  exit 1
fi

echo "Done."
