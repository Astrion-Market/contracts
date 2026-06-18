#!/usr/bin/env bash
# Initialize all deployed Astrion contracts.
#
# Must be run after ops/deploy-all.sh.
# Idempotent: already-initialized contracts are skipped.
#
# Usage:
#   ops/init-all.sh [network] [source] [config] [addresses]

set -euo pipefail

network="${1:-testnet}"
source_account="${2:-deployer}"
config_file="${3:-deployments/${network}/config.env}"
addresses_file="${4:-deployments/${network}/addresses.env}"
deploy_dir="$(dirname "$addresses_file")"

export DEPLOY_DIR="$deploy_dir"
source ops/state.sh

# ─── pre-flight checks ────────────────────────────────────────────────────────
if [[ ! -f "$addresses_file" ]]; then
  echo "error: ${addresses_file} not found. Run make deploy-all first." >&2
  exit 1
fi
if [[ ! -f "$config_file" ]]; then
  echo "error: ${config_file} not found. Copy deployments/config.env.example and edit it." >&2
  exit 1
fi

set -a
source "$addresses_file"
source "$config_file"
set +a

# ─── helpers ──────────────────────────────────────────────────────────────────
require_var() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "error: missing required config: ${name}" >&2
    exit 1
  fi
}

invoke() {
  stellar contract invoke --network "$network" --source "$source_account" "$@"
}

already_initialized() {
  local alias="$1"
  local status; status="$(state_status "$alias" 2>/dev/null || echo "pending")"
  [[ "$status" == "initialized" ]]
}

mark_initialized() {
  local alias="$1"
  state_set "$alias" "status"         "initialized"
  state_set "$alias" "initialized_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}

# ─── required config ──────────────────────────────────────────────────────────
require_var ADMIN_ADDRESS
require_var TREASURY_ADDRESS
require_var ORACLE_ADAPTER_ID
require_var RATE_MODEL_ID
require_var CORE_POOL_ID
require_var LIQUIDATION_ENGINE_ID
require_var MARKET_FACTORY_ID

# DEFAULT_ORACLE_ID only required when oracle-adapter is not yet initialized.
if ! already_initialized "oracle-adapter"; then
  require_var DEFAULT_ORACLE_ID
fi

# Test-token and mock-oracle IDs are only required on non-mainnet networks.
if [[ "$network" != "mainnet" ]]; then
  require_var TEST_USDC_ID
  require_var TEST_WBTC_ID
  require_var MOCK_ORACLE_ID
fi

# ─── defaults ─────────────────────────────────────────────────────────────────
: "${DEFAULT_MAX_STALENESS:=300}"
: "${BASE_RATE:=10000000000000000}"
: "${SLOPE1:=40000000000000000}"
: "${SLOPE2:=750000000000000000}"
: "${OPTIMAL_UTILIZATION:=800000000000000000}"
: "${RESERVE_FACTOR:=100000000000000000}"
: "${CLOSE_FACTOR:=500000000000000000}"

echo ""
echo "Initializing Astrion contracts on ${network}…"
echo ""

# ─── 1. oracle-adapter ────────────────────────────────────────────────────────
if already_initialized "oracle-adapter"; then
  echo "oracle-adapter: already initialized, skipping"
else
  echo "Initializing oracle-adapter…"
  invoke --id "$ORACLE_ADAPTER_ID" -- initialize \
    --admin               "$ADMIN_ADDRESS" \
    --default_oracle      "$DEFAULT_ORACLE_ID" \
    --default_max_staleness "$DEFAULT_MAX_STALENESS"
  mark_initialized "oracle-adapter"
fi

# ─── 2. rate-model ────────────────────────────────────────────────────────────
if already_initialized "rate-model"; then
  echo "rate-model: already initialized, skipping"
else
  echo "Initializing rate-model…"
  invoke --id "$RATE_MODEL_ID" -- initialize \
    --admin  "$ADMIN_ADDRESS" \
    --config "{\"base_rate\":\"${BASE_RATE}\",\"slope1\":\"${SLOPE1}\",\"slope2\":\"${SLOPE2}\",\"optimal_utilization\":\"${OPTIMAL_UTILIZATION}\",\"reserve_factor\":\"${RESERVE_FACTOR}\"}"
  mark_initialized "rate-model"
fi

# ─── 3. core-pool ─────────────────────────────────────────────────────────────
if already_initialized "core-pool"; then
  echo "core-pool: already initialized, skipping"
else
  echo "Initializing core-pool…"
  invoke --id "$CORE_POOL_ID" -- initialize \
    --admin          "$ADMIN_ADDRESS" \
    --oracle_adapter "$ORACLE_ADAPTER_ID" \
    --rate_model     "$RATE_MODEL_ID" \
    --treasury       "$TREASURY_ADDRESS"
  mark_initialized "core-pool"
fi

# ─── 4. liquidation-engine ────────────────────────────────────────────────────
if already_initialized "liquidation-engine"; then
  echo "liquidation-engine: already initialized, skipping"
else
  echo "Initializing liquidation-engine…"
  invoke --id "$LIQUIDATION_ENGINE_ID" -- initialize \
    --admin          "$ADMIN_ADDRESS" \
    --core_pool      "$CORE_POOL_ID" \
    --oracle_adapter "$ORACLE_ADAPTER_ID" \
    --close_factor   "$CLOSE_FACTOR"
  mark_initialized "liquidation-engine"
fi

# ─── 5. market-factory (upload market WASM + initialize) ──────────────────────
if already_initialized "market-factory"; then
  echo "market-factory: already initialized, skipping"
else
  echo "Uploading market WASM for factory…"
  market_wasm_path="target/wasm32v1-none/release/market.wasm"
  if [[ -f "target/wasm32v1-none/release/market.optimized.wasm" ]]; then
    market_wasm_path="target/wasm32v1-none/release/market.optimized.wasm"
  fi
  market_wasm_hash="$(
    stellar -q contract upload \
      --wasm     "$market_wasm_path" \
      --network  "$network" \
      --source   "$source_account" \
      | tail -n 1
  )"
  state_set "market-factory" "market_wasm_hash" "$market_wasm_hash"

  echo "Initializing market-factory…"
  invoke --id "$MARKET_FACTORY_ID" -- initialize \
    --admin           "$ADMIN_ADDRESS" \
    --market_wasm_hash "$market_wasm_hash"
  mark_initialized "market-factory"
fi

# ─── 6–9. testnet-only setup ─────────────────────────────────────────────────
if [[ "$network" != "mainnet" ]]; then

  # ─── 6. mock-oracle ─────────────────────────────────────────────────────────
  if already_initialized "mock-oracle"; then
    echo "mock-oracle: already initialized, skipping"
  else
    echo "Initializing mock-oracle…"
    invoke --id "$MOCK_ORACLE_ID" -- initialize \
      --admin "$ADMIN_ADDRESS"
    mark_initialized "mock-oracle"
  fi

  # ─── 7. mock-oracle prices ──────────────────────────────────────────────────
  echo "Setting mock-oracle prices…"
  invoke --id "$MOCK_ORACLE_ID" -- set_price \
    --asset "{\"Stellar\":\"${TEST_USDC_ID}\"}" \
    --price 10000000
  echo "  USDC → \$1.00"

  invoke --id "$MOCK_ORACLE_ID" -- set_price \
    --asset "{\"Stellar\":\"${TEST_WBTC_ID}\"}" \
    --price 600000000000
  echo "  WBTC → \$60,000"

  # ─── 8. wire mock-oracle into oracle-adapter ────────────────────────────────
  echo "Wiring mock-oracle into oracle-adapter…"

  # Set as default so any asset without a specific override uses mock-oracle.
  invoke --id "$ORACLE_ADAPTER_ID" -- set_default_oracle \
    --new_oracle "$MOCK_ORACLE_ID"
  echo "  oracle-adapter default oracle → mock-oracle"

  # Per-asset overrides with generous staleness for testnet.
  invoke --id "$ORACLE_ADAPTER_ID" -- set_asset_oracle \
    --asset    "{\"Stellar\":\"${TEST_USDC_ID}\"}" \
    --oracle   "$MOCK_ORACLE_ID" \
    --max_staleness 9999999
  echo "  USDC → mock-oracle"

  invoke --id "$ORACLE_ADAPTER_ID" -- set_asset_oracle \
    --asset    "{\"Stellar\":\"${TEST_WBTC_ID}\"}" \
    --oracle   "$MOCK_ORACLE_ID" \
    --max_staleness 9999999
  echo "  WBTC → mock-oracle"

  # ─── 9. add USDC and WBTC markets to CorePool ───────────────────────────────
  echo "Configuring CorePool markets…"

  # USDC — stablecoin: higher LTV (85%), tight liquidation bonus (3%)
  if stellar contract invoke --network "$network" --source "$source_account" \
       --id "$CORE_POOL_ID" -- get_market_config \
       --asset "$TEST_USDC_ID" 2>/dev/null | grep -q '"asset"'; then
    echo "  USDC market already exists"
  else
    invoke --id "$CORE_POOL_ID" -- add_market \
      --config "{\"asset\":\"${TEST_USDC_ID}\",\"ltv\":\"850000000000000000\",\"liquidation_threshold\":\"900000000000000000\",\"liquidation_bonus\":\"30000000000000000\",\"reserve_factor\":\"100000000000000000\",\"supply_cap\":\"0\",\"borrow_cap\":\"0\",\"is_active\":true,\"is_borrowable\":true}"
    echo "  USDC market added (LTV 85%, liq threshold 90%)"
  fi

  # WBTC — volatile: lower LTV (70%), higher liquidation bonus (5%)
  if stellar contract invoke --network "$network" --source "$source_account" \
       --id "$CORE_POOL_ID" -- get_market_config \
       --asset "$TEST_WBTC_ID" 2>/dev/null | grep -q '"asset"'; then
    echo "  WBTC market already exists"
  else
    invoke --id "$CORE_POOL_ID" -- add_market \
      --config "{\"asset\":\"${TEST_WBTC_ID}\",\"ltv\":\"700000000000000000\",\"liquidation_threshold\":\"800000000000000000\",\"liquidation_bonus\":\"50000000000000000\",\"reserve_factor\":\"100000000000000000\",\"supply_cap\":\"0\",\"borrow_cap\":\"0\",\"is_active\":true,\"is_borrowable\":true}"
    echo "  WBTC market added (LTV 70%, liq threshold 80%)"
  fi

fi

# ─── optional: add CorePool market ────────────────────────────────────────────
if [[ -n "${CORE_MARKET_ASSET:-}" ]]; then
  : "${CORE_MARKET_LTV:=700000000000000000}"
  : "${CORE_MARKET_LIQUIDATION_THRESHOLD:=800000000000000000}"
  : "${CORE_MARKET_LIQUIDATION_BONUS:=50000000000000000}"
  : "${CORE_MARKET_RESERVE_FACTOR:=$RESERVE_FACTOR}"
  : "${CORE_MARKET_SUPPLY_CAP:=0}"
  : "${CORE_MARKET_BORROW_CAP:=0}"
  : "${CORE_MARKET_IS_ACTIVE:=true}"
  : "${CORE_MARKET_IS_BORROWABLE:=true}"

  echo "Adding CorePool market ${CORE_MARKET_ASSET}…"
  invoke --id "$CORE_POOL_ID" -- add_market \
    --config "{\"asset\":\"${CORE_MARKET_ASSET}\",\"ltv\":\"${CORE_MARKET_LTV}\",\"liquidation_threshold\":\"${CORE_MARKET_LIQUIDATION_THRESHOLD}\",\"liquidation_bonus\":\"${CORE_MARKET_LIQUIDATION_BONUS}\",\"reserve_factor\":\"${CORE_MARKET_RESERVE_FACTOR}\",\"supply_cap\":\"${CORE_MARKET_SUPPLY_CAP}\",\"borrow_cap\":\"${CORE_MARKET_BORROW_CAP}\",\"is_active\":${CORE_MARKET_IS_ACTIVE},\"is_borrowable\":${CORE_MARKET_IS_BORROWABLE}}"
fi

# ─── optional: initialize standalone isolated market ─────────────────────────
if [[ -n "${MARKET_COLLATERAL_ASSET:-}" && -n "${MARKET_DEBT_ASSET:-}" ]]; then
  if already_initialized "market"; then
    echo "market: already initialized, skipping"
  else
    : "${MARKET_LTV:=700000000000000000}"
    : "${MARKET_LIQUIDATION_THRESHOLD:=800000000000000000}"
    : "${MARKET_LIQUIDATION_BONUS:=50000000000000000}"
    : "${MARKET_RESERVE_FACTOR:=$RESERVE_FACTOR}"
    : "${MARKET_SUPPLY_CAP:=0}"
    : "${MARKET_BORROW_CAP:=0}"

    echo "Initializing standalone isolated market…"
    invoke --id "$MARKET_ID" -- initialize \
      --config "{\"collateral_asset\":\"${MARKET_COLLATERAL_ASSET}\",\"debt_asset\":\"${MARKET_DEBT_ASSET}\",\"oracle_adapter\":\"${ORACLE_ADAPTER_ID}\",\"ltv\":\"${MARKET_LTV}\",\"liquidation_threshold\":\"${MARKET_LIQUIDATION_THRESHOLD}\",\"liquidation_bonus\":\"${MARKET_LIQUIDATION_BONUS}\",\"reserve_factor\":\"${MARKET_RESERVE_FACTOR}\",\"supply_cap\":\"${MARKET_SUPPLY_CAP}\",\"borrow_cap\":\"${MARKET_BORROW_CAP}\",\"rate_model\":\"${RATE_MODEL_ID}\",\"treasury\":\"${TREASURY_ADDRESS}\"}"
    mark_initialized "market"
  fi
fi

# ─── test tokens: transfer ownership to ADMIN_ADDRESS (non-mainnet) ──────────
# The constructor already set admin = deployer (or ADMIN_ADDRESS if config was
# present at deploy time).  This step is a no-op if they are the same key, and
# handles the common case where ADMIN_ADDRESS is a multisig set after deploy.
if [[ "$network" != "mainnet" ]]; then
  deployer_pubkey="$(stellar keys address "$source_account" 2>/dev/null || echo "")"

  for _token_alias in "test-usdc" "test-wbtc"; do
    _token_var="${_token_alias//-/_}"
    _token_var="${_token_var^^}_ID"
    _token_id="${!_token_var}"

    if [[ -z "$_token_id" ]]; then
      echo "${_token_alias}: ID not set — skipping ownership transfer"
      continue
    fi

    # Query current owner; skip transfer if already correct.
    _current_owner="$(
      stellar contract invoke \
        --network "$network" --source "$source_account" \
        --id "$_token_id" -- owner 2>/dev/null || echo ""
    )"
    # Strip surrounding quotes that stellar CLI sometimes adds
    _current_owner="${_current_owner//\"/}"

    if [[ "$_current_owner" == "$ADMIN_ADDRESS" ]]; then
      echo "${_token_alias}: owner already ${ADMIN_ADDRESS:0:8}…, skipping transfer"
    elif [[ -n "$_current_owner" && "$_current_owner" != "$ADMIN_ADDRESS" ]]; then
      echo "Transferring ${_token_alias} ownership → ${ADMIN_ADDRESS:0:8}…"
      invoke --id "$_token_id" -- transfer_ownership \
        --new_owner "$ADMIN_ADDRESS"
    else
      echo "${_token_alias}: could not read current owner — skipping transfer"
    fi
  done
fi

echo ""
echo "Initialization complete."
echo ""
state_print
