#!/usr/bin/env bash
set -euo pipefail

mode="${1:-}"
network="${2:-testnet}"
source_account="${3:-deployer}"
config_file="${4:-deployments/${network}/config.env}"
addresses_file="${5:-deployments/${network}/addresses.env}"

wallet_prefix="${WALLET_PREFIX:-astrion-dev}"
lender="${LENDER_IDENTITY:-${wallet_prefix}-lender}"
borrower="${BORROWER_IDENTITY:-${wallet_prefix}-borrower}"
keeper="${KEEPER_IDENTITY:-${wallet_prefix}-keeper}"

generate_wallet() {
  local name="$1"
  if stellar keys address "$name" >/dev/null 2>&1; then
    echo "${name}: $(stellar keys address "$name")"
  else
    stellar keys generate "$name" --network "$network" --fund
    echo "${name}: $(stellar keys address "$name")"
  fi
}

case "$mode" in
  wallets)
    generate_wallet "$lender"
    generate_wallet "$borrower"
    generate_wallet "$keeper"
    ;;
  flow)
    if [[ ! -f "$addresses_file" || ! -f "$config_file" ]]; then
      echo "Missing ${addresses_file} or ${config_file}. Run deploy/init first." >&2
      exit 1
    fi
    set -a
    source "$addresses_file"
    source "$config_file"
    set +a

    echo "Reading current rate snapshot"
    stellar contract invoke \
      --id "$RATE_MODEL_ID" \
      --network "$network" \
      --source "$source_account" \
      -- get_rates \
      --total_borrowed "${SIM_TOTAL_BORROWED:-80000000000000000000}" \
      --total_supplied "${SIM_TOTAL_SUPPLIED:-100000000000000000000}"

    if [[ -n "${SIM_ASSET:-}" && -n "${SIM_SUPPLY_AMOUNT:-}" ]]; then
      lender_address="$(stellar keys address "$lender")"
      echo "Supplying ${SIM_SUPPLY_AMOUNT} of ${SIM_ASSET} as ${lender}"
      stellar contract invoke \
        --id "$CORE_POOL_ID" \
        --network "$network" \
        --source "$lender" \
        -- supply \
        --supplier "$lender_address" \
        --asset "$SIM_ASSET" \
        --amount "$SIM_SUPPLY_AMOUNT"
    fi

    if [[ -n "${SIM_ASSET:-}" && -n "${SIM_BORROW_AMOUNT:-}" ]]; then
      borrower_address="$(stellar keys address "$borrower")"
      echo "Borrowing ${SIM_BORROW_AMOUNT} of ${SIM_ASSET} as ${borrower}"
      stellar contract invoke \
        --id "$CORE_POOL_ID" \
        --network "$network" \
        --source "$borrower" \
        -- borrow \
        --borrower "$borrower_address" \
        --asset "$SIM_ASSET" \
        --amount "$SIM_BORROW_AMOUNT"
    fi

    echo "Rewards are not implemented in the current contracts; skipping reward claim."
    ;;
  *)
    echo "Usage:"
    echo "  ops/dev-flow.sh wallets <network>"
    echo "  ops/dev-flow.sh flow <network> <source> <config> <addresses>"
    exit 1
    ;;
esac
