#!/usr/bin/env bash
# Astrion deployment state management.
#
# Source this file from other scripts:
#   DEPLOY_DIR=deployments/testnet source ops/state.sh
#
# Run directly to print the status table:
#   ops/state.sh [network]

_state_file() { echo "${DEPLOY_DIR}/state.json"; }
_ts()         { date -u +%Y-%m-%dT%H:%M:%SZ; }

state_require_jq() {
  if ! command -v jq &>/dev/null; then
    echo "error: 'jq' is required. Install it: https://jqlang.github.io/jq/" >&2
    return 1
  fi
}

# Create state.json if it does not exist.
state_init() {
  state_require_jq || return 1
  local network="$1" deployer="$2"
  local file; file="$(_state_file)"
  mkdir -p "$(dirname "$file")"
  [[ -f "$file" ]] && return 0
  jq -n \
    --arg net "$network" \
    --arg dep "$deployer" \
    --arg ts "$(_ts)" \
    '{network:$net, deployer:$dep, created_at:$ts, updated_at:$ts, contracts:{}}' \
    > "$file"
}

# Read a field from a contract's state entry.
state_get() {
  local contract="$1" field="$2"
  local file; file="$(_state_file)"
  [[ -f "$file" ]] || { echo ""; return 0; }
  jq -r --arg c "$contract" --arg f "$field" '.contracts[$c][$f] // empty' "$file"
}

# Write a field to a contract's state entry.
state_set() {
  local contract="$1" field="$2" value="$3"
  local file; file="$(_state_file)"
  local tmp="${file}.tmp"
  jq --arg c "$contract" --arg f "$field" --arg v "$value" --arg ts "$(_ts)" \
    '.contracts[$c] //= {} | .contracts[$c][$f] = $v | .updated_at = $ts' \
    "$file" > "$tmp" && mv "$tmp" "$file"
}

# Shorthand: get the deployment status of a contract.
state_status() { state_get "$1" "status"; }

# Print a formatted table of the current deployment state.
state_print() {
  state_require_jq || return 1
  local file; file="$(_state_file)"
  if [[ ! -f "$file" ]]; then
    echo "No state file at $file — run 'make deploy-all' first."
    return 0
  fi

  local network deployer updated
  network="$(jq -r '.network'    "$file")"
  deployer="$(jq -r '.deployer'  "$file")"
  updated="$(jq  -r '.updated_at' "$file")"

  echo ""
  printf 'Astrion Deployment State\n'
  printf 'Network: %-12s  Deployer: %-18s  Updated: %s\n\n' \
    "$network" "$deployer" "$updated"
  printf '%-25s %-57s %-13s\n' "CONTRACT" "ADDRESS" "STATUS"
  printf '%-25s %-57s %-13s\n' \
    "─────────────────────────" \
    "─────────────────────────────────────────────────────────" \
    "─────────────"

  local contracts
  contracts="$(jq -r '.contracts | keys[]' "$file" 2>/dev/null || true)"
  if [[ -z "$contracts" ]]; then
    echo "  (no contracts recorded)"
  else
    while IFS= read -r contract; do
      local addr status
      addr="$(jq  -r --arg c "$contract" '.contracts[$c].contract_id // "—"' "$file")"
      status="$(jq -r --arg c "$contract" '.contracts[$c].status // "unknown"' "$file")"
      printf '%-25s %-57s %-13s\n' "$contract" "$addr" "$status"
    done <<< "$contracts"
  fi
  echo ""
}

# ─── direct-execution mode ───────────────────────────────────────────────────
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  state_require_jq || exit 1
  network="${1:-${NETWORK:-testnet}}"
  export DEPLOY_DIR="${DEPLOY_DIR:-deployments/${network}}"
  state_print
fi
