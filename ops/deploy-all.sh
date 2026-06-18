#!/usr/bin/env bash
# Deploy all Astrion contracts to the target network.
#
# Usage:
#   ops/deploy-all.sh [network] [source_account]
#   DRYRUN=1 ops/deploy-all.sh testnet deployer   # preview without executing
#
# Outputs:
#   deployments/{network}/addresses.env   — contract IDs
#   deployments/{network}/state.json      — deployment state + WASM hashes
#   deployments/{network}/checksums.sha256 — local WASM sha256 fingerprints
#   deployments/{network}/report-*.md    — human-readable deployment report

set -euo pipefail

network="${1:-testnet}"
source_account="${2:-deployer}"
dry_run="${DRYRUN:-0}"

wasm_dir="${WASM_DIR:-target/wasm32v1-none/release}"
deploy_dir="${DEPLOY_DIR:-deployments/${network}}"
addresses_file="${deploy_dir}/addresses.env"
checksum_file="${deploy_dir}/checksums.sha256"

# ─── dependencies ─────────────────────────────────────────────────────────────
if ! command -v jq &>/dev/null; then
  echo "error: 'jq' is required. Install it: https://jqlang.github.io/jq/" >&2
  exit 1
fi

# ─── state management ─────────────────────────────────────────────────────────
export DEPLOY_DIR="$deploy_dir"
source ops/state.sh

mkdir -p "$deploy_dir"
state_init "$network" "$source_account"

# ─── deployment guard ─────────────────────────────────────────────────────────
# If all core protocol contracts are already live, block re-deployment.
# Re-deploying creates NEW addresses and silently breaks every integration.
# Use FORCE=1 only when intentionally setting up a fresh environment.
_all_core_deployed() {
  local core=("oracle-adapter" "rate-model" "core-pool" "liquidation-engine" "market-factory")
  for a in "${core[@]}"; do
    local s; s="$(state_status "$a" 2>/dev/null || echo "pending")"
    [[ "$s" != "deployed" && "$s" != "initialized" ]] && return 1
  done
  return 0
}

if [[ "${FORCE:-0}" != "1" && "$dry_run" != "1" ]] && _all_core_deployed; then
  echo ""
  echo "╔══════════════════════════════════════════════════════════════╗"
  echo "║  CONTRACTS ALREADY DEPLOYED — USE UPGRADE INSTEAD           ║"
  echo "╚══════════════════════════════════════════════════════════════╝"
  echo ""
  echo "All core protocol contracts are already live on '${network}'."
  echo "Re-deploying creates NEW addresses and breaks every integration"
  echo "pointed at the current ones."
  echo ""
  echo "  Upgrade all contracts (same addresses, new WASM only):"
  printf '    make upgrade-all  NETWORK=%s SOURCE=%s\n' "$network" "$source_account"
  echo ""
  echo "  Upgrade a single contract:"
  printf '    make upgrade CONTRACT=oracle-adapter      NETWORK=%s\n' "$network"
  printf '    make upgrade CONTRACT=rate-model          NETWORK=%s\n' "$network"
  printf '    make upgrade CONTRACT=core-pool           NETWORK=%s\n' "$network"
  printf '    make upgrade CONTRACT=liquidation-engine  NETWORK=%s\n' "$network"
  printf '    make upgrade CONTRACT=market-factory      NETWORK=%s\n' "$network"
  echo ""
  echo "  Current deployment state:"
  state_print
  echo ""
  echo "  Force a fresh deployment (DESTRUCTIVE — new addresses, new environment):"
  printf '    FORCE=1 ops/deploy-all.sh %s %s\n' "$network" "$source_account"
  echo ""
  exit 1
fi

# ─── dry-run banner ───────────────────────────────────────────────────────────
if [[ "$dry_run" == "1" ]]; then
  echo ""
  echo "╔══════════════════════════════════════════════════════════╗"
  echo "║  DRY RUN — no contracts will be deployed                ║"
  echo "╚══════════════════════════════════════════════════════════╝"
  echo ""
fi

# ─── build ────────────────────────────────────────────────────────────────────
if [[ "$dry_run" != "1" ]]; then
  echo "Building contracts..."
  stellar contract build
else
  echo "[dry-run] would run: stellar contract build"
fi

# ─── pin WASM checksums ───────────────────────────────────────────────────────
echo "Computing WASM checksums..."
if [[ "$dry_run" != "1" ]]; then
  find "$wasm_dir" -maxdepth 1 -name '*.wasm' -exec sha256sum {} \; \
    | sed "s|${wasm_dir}/||" \
    | sort > "$checksum_file"
  echo "Checksums pinned → ${checksum_file}"
fi

# ─── helpers ──────────────────────────────────────────────────────────────────
_wasm_path() {
  local wasm="$1"
  if [[ -f "${wasm_dir}/${wasm}.optimized.wasm" ]]; then
    echo "${wasm_dir}/${wasm}.optimized.wasm"
  else
    echo "${wasm_dir}/${wasm}.wasm"
  fi
}

_wasm_sha256() {
  local path="$1"
  sha256sum "$path" | awk '{print $1}'
}

# ─── deploy function ──────────────────────────────────────────────────────────
# Deploy a contract whose constructor takes (admin, decimals, name, symbol).
# Used for test-token instances.  Status is set to "initialized" immediately
# because the constructor handles all setup — init-all.sh has nothing to do.
deploy_token_ctor() {
  local alias="$1" var_name="$2" name="$3" symbol="$4" admin_addr="$5"

  local current_status
  current_status="$(state_status "$alias" 2>/dev/null || echo "pending")"
  if [[ "$current_status" == "deployed" || "$current_status" == "initialized" ]]; then
    local existing_id
    existing_id="$(state_get "$alias" "contract_id")"
    echo "${alias}: already deployed (${existing_id:0:8}…), skipping"
    printf '%s=%q\n' "$var_name" "$existing_id" >> "$addresses_file.tmp"
    return 0
  fi

  local wasm_path
  wasm_path="$(_wasm_path "test_token")"

  if [[ "$dry_run" == "1" ]]; then
    printf '[dry-run] stellar contract deploy --wasm %s --alias %s -- --admin %s --decimals 7 --name "%s" --symbol "%s"\n' \
      "$wasm_path" "$alias" "${admin_addr:0:10}…" "$name" "$symbol"
    printf '%s=%s\n' "$var_name" "DRY_RUN_PLACEHOLDER" >> "$addresses_file.tmp"
    return 0
  fi

  local sha256
  sha256="$(_wasm_sha256 "$wasm_path")"
  printf 'Deploying %-25s  sha256: %s…\n' "${alias}" "${sha256:0:16}"

  local id
  id="$(
    stellar -q contract deploy \
      --wasm    "$wasm_path" \
      --network "$network" \
      --source  "$source_account" \
      --alias   "$alias" \
      -- \
      --admin    "$admin_addr" \
      --decimals 7 \
      --name     "$name" \
      --symbol   "$symbol" \
      | tail -n 1
  )"

  if [[ -z "$id" ]]; then
    echo "error: failed to capture contract id for ${alias}" >&2
    state_set "$alias" "status" "failed"
    exit 1
  fi

  state_set "$alias" "contract_id" "$id"
  state_set "$alias" "wasm_sha256"  "$sha256"
  state_set "$alias" "wasm_file"    "$(basename "$wasm_path")"
  state_set "$alias" "status"       "initialized"
  state_set "$alias" "deployed_at"  "$(_ts)"

  printf '%s=%q\n' "$var_name" "$id" >> "$addresses_file.tmp"
  echo "${alias}: ${id}"
}

deploy_contract() {
  local alias="$1"
  local wasm="$2"
  local var_name="$3"

  # idempotency: skip if already deployed or initialized
  local current_status
  current_status="$(state_status "$alias" 2>/dev/null || echo "pending")"
  if [[ "$current_status" == "deployed" || "$current_status" == "initialized" ]]; then
    local existing_id
    existing_id="$(state_get "$alias" "contract_id")"
    echo "${alias}: already deployed (${existing_id:0:8}…), skipping"
    printf '%s=%q\n' "$var_name" "$existing_id" >> "$addresses_file.tmp"
    return 0
  fi

  local wasm_path
  wasm_path="$(_wasm_path "$wasm")"

  if [[ "$dry_run" == "1" ]]; then
    printf '[dry-run] stellar contract deploy --wasm %s --network %s --source %s --alias %s\n' \
      "$wasm_path" "$network" "$source_account" "$alias"
    printf '%s=%s\n' "$var_name" "DRY_RUN_PLACEHOLDER" >> "$addresses_file.tmp"
    return 0
  fi

  local sha256
  sha256="$(_wasm_sha256 "$wasm_path")"

  printf 'Deploying %-25s  sha256: %s…\n' "${alias}" "${sha256:0:16}"
  local id
  id="$(
    stellar -q contract deploy \
      --wasm     "$wasm_path" \
      --network  "$network" \
      --source   "$source_account" \
      --alias    "$alias" \
      | tail -n 1
  )"

  if [[ -z "$id" ]]; then
    echo "error: failed to capture contract id for ${alias}" >&2
    state_set "$alias" "status" "failed"
    exit 1
  fi

  state_set "$alias" "contract_id" "$id"
  state_set "$alias" "wasm_sha256"  "$sha256"
  state_set "$alias" "wasm_file"    "$(basename "$wasm_path")"
  state_set "$alias" "status"       "deployed"
  state_set "$alias" "deployed_at"  "$(_ts)"

  printf '%s=%q\n' "$var_name" "$id" >> "$addresses_file.tmp"
  echo "${alias}: ${id}"
}

# ─── write address file header ────────────────────────────────────────────────
cat > "$addresses_file.tmp" <<EOF
# Generated by ops/deploy-all.sh
# Network:  ${network}
# Deployer: ${source_account}
# Date:     $(date -u +%Y-%m-%dT%H:%M:%SZ)
# Commit:   $(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
EOF

# ─── deploy all contracts in dependency order ─────────────────────────────────
deploy_contract "oracle-adapter"      "oracle_adapter"      "ORACLE_ADAPTER_ID"
deploy_contract "rate-model"          "interest_rate_model" "RATE_MODEL_ID"
deploy_contract "core-pool"           "core_pool"           "CORE_POOL_ID"
deploy_contract "liquidation-engine"  "liquidation_engine"  "LIQUIDATION_ENGINE_ID"
deploy_contract "market"              "market"              "MARKET_ID"
deploy_contract "market-factory"      "market_factory"      "MARKET_FACTORY_ID"

# ─── testnet-only contracts ───────────────────────────────────────────────────
# Mock oracle + test tokens: never deployed to mainnet.
if [[ "$network" != "mainnet" ]]; then
  echo ""
  echo "Deploying testnet-only contracts (${network})…"

  deploy_contract "mock-oracle" "mock_oracle" "MOCK_ORACLE_ID"

  echo ""
  echo "Deploying test tokens (${network} only)…"

  # Resolve admin: prefer ADMIN_ADDRESS from config.env if available.
  _token_admin="$(stellar keys address "$source_account" 2>/dev/null || echo "")"
  _cfg="${deploy_dir}/config.env"
  if [[ -f "$_cfg" ]]; then
    _cfg_admin="$(grep -E '^ADMIN_ADDRESS=' "$_cfg" | cut -d= -f2- | tr -d '"' | xargs)"
    [[ -n "$_cfg_admin" ]] && _token_admin="$_cfg_admin"
  fi

  deploy_token_ctor "test-usdc" "TEST_USDC_ID" "Test USD Coin"       "USDC" "$_token_admin"
  deploy_token_ctor "test-wbtc" "TEST_WBTC_ID" "Test Wrapped Bitcoin" "WBTC" "$_token_admin"
fi

# ─── commit outputs ───────────────────────────────────────────────────────────
if [[ "$dry_run" != "1" ]]; then
  mv "$addresses_file.tmp" "$addresses_file"
  echo ""
  echo "Wrote ${addresses_file}"
else
  rm -f "$addresses_file.tmp"
  echo ""
  echo "[dry-run] addresses.env and state.json were NOT modified."
  exit 0
fi

# ─── deployment report ────────────────────────────────────────────────────────
state_file="$(_state_file)"
report_file="${deploy_dir}/report-$(date -u +%Y%m%d-%H%M%S).md"

{
  echo "# Astrion Protocol Deployment Report"
  echo ""
  echo "| | |"
  echo "|---|---|"
  echo "| **Network**  | ${network} |"
  echo "| **Deployer** | ${source_account} |"
  echo "| **Date**     | $(date -u) |"
  echo "| **Commit**   | \`$(git rev-parse HEAD 2>/dev/null || echo 'unknown')\` |"
  echo ""
  echo "## Contract Addresses"
  echo ""
  echo "| Contract | Address | WASM SHA-256 |"
  echo "|----------|---------|:-------------|"
  while IFS= read -r contract; do
    addr="$(jq    -r --arg c "$contract" '.contracts[$c].contract_id // "—"' "$state_file")"
    wasm_hash="$(jq -r --arg c "$contract" '.contracts[$c].wasm_sha256 // "—"' "$state_file")"
    echo "| \`${contract}\` | \`${addr}\` | \`${wasm_hash:0:20}…\` |"
  done < <(jq -r '.contracts | keys[]' "$state_file")
  echo ""
  echo "## Next Steps"
  echo ""
  echo "1. Copy \`deployments/config.env.example\` → \`${deploy_dir}/config.env\` and fill in admin/treasury/oracle."
  echo "2. Run \`make init-all NETWORK=${network}\`"
  echo "3. Run \`make verify  NETWORK=${network}\`"
} > "$report_file"

echo "Deployment report → ${report_file}"
echo ""
state_print
