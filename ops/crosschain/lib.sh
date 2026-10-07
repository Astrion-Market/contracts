#!/usr/bin/env bash
# Shared helpers for the CCTP testnet harness. Source from ops/crosschain/*.sh.
#
# Secrets never touch disk: STELLAR_SOURCE is a `stellar keys` alias and
# EVM_PRIVATE_KEY is read from the environment only. Checkpoints hold public
# data (tx hashes, signed envelopes, messages, attestations, balances).

set -euo pipefail

MANIFEST="deployments/crosschain/testnet.json"
RUNS_DIR="${RUNS_DIR:-ops/crosschain/runs}"
STELLAR_NETWORK="${STELLAR_NETWORK:-testnet}"
STELLAR_RPC_URL="${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org}"
HORIZON_URL="${HORIZON_URL:-https://horizon-testnet.stellar.org}"
IRIS_URL="${IRIS_URL:-https://iris-api-sandbox.circle.com}"
ATTESTATION_TIMEOUT_SECS="${ATTESTATION_TIMEOUT_SECS:-1800}"

die() { echo "error: $*" >&2; exit 1; }
log() { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }
now() { date -u +%Y-%m-%dT%H:%M:%SZ; }

require_tools() {
  for t in jq curl stellar cast; do command -v "$t" >/dev/null || die "$t is required"; done
}

# ─── manifest ────────────────────────────────────────────────────────────────
m() { jq -er "$1" "$MANIFEST"; }

evm_network_for() {  # base-sepolia | ethereum-sepolia
  case "${1:-base-sepolia}" in
    base-sepolia|ethereum-sepolia) echo "$1" ;;
    *) die "unsupported EVM testnet '$1' (base-sepolia | ethereum-sepolia)" ;;
  esac
}

evm_rpc_for() {
  case "$1" in
    base-sepolia) echo "${BASE_SEPOLIA_RPC_URL:?set BASE_SEPOLIA_RPC_URL}" ;;
    ethereum-sepolia) echo "${ETH_SEPOLIA_RPC_URL:?set ETH_SEPOLIA_RPC_URL}" ;;
  esac
}

# ─── checkpoints ─────────────────────────────────────────────────────────────
ckpt_file() { echo "${RUNS_DIR}/${RUN_ID}.json"; }

ckpt_init() {
  mkdir -p "$RUNS_DIR"
  local f; f="$(ckpt_file)"
  if [[ -f "$f" ]]; then
    local existing; existing="$(jq -r .direction "$f")"
    [[ "$existing" == "$1" ]] || die "run ${RUN_ID} already exists with direction ${existing}"
    return 0
  fi
  jq -n --arg id "$RUN_ID" --arg dir "$1" --arg ts "$(now)" \
    '{runId:$id, direction:$dir, createdAt:$ts, updatedAt:$ts, steps:{}}' > "$f"
}

ckpt_get() { jq -r "$1 // empty" "$(ckpt_file)"; }

# ckpt_set <jq path> <json value>
ckpt_set() {
  local f tmp; f="$(ckpt_file)"; tmp="${f}.tmp"
  jq --argjson v "$2" --arg ts "$(now)" "$1 = \$v | .updatedAt = \$ts" "$f" > "$tmp"
  mv "$tmp" "$f"   # atomic replace: a crash never leaves a half-written checkpoint
}

ckpt_set_str() { ckpt_set "$1" "$(jq -n --arg v "$2" '$v')"; }

step_done() { [[ "$(ckpt_get ".steps.\"$1\".status")" == "done" ]]; }
mark_done() { ckpt_set ".steps.\"$1\".status" '"done"'; ckpt_set_str ".steps.\"$1\".at" "$(now)"; }

# ─── amounts (raw integers; i64 range, bash arithmetic is 64-bit) ────────────
require_uint() { [[ "$1" =~ ^(0|[1-9][0-9]{0,18})$ ]] || die "invalid amount '$1'"; }

# ─── Stellar ─────────────────────────────────────────────────────────────────
stellar_rpc() {  # stellar_rpc <method> <params json>
  curl -fsS "$STELLAR_RPC_URL" -H 'content-type: application/json' \
    -d "$(jq -n --arg m "$1" --argjson p "$2" '{jsonrpc:"2.0",id:1,method:$m,params:$p}')"
}

stellar_tx_status() {  # SUCCESS | FAILED | NOT_FOUND
  stellar_rpc getTransaction "$(jq -n --arg h "$1" '{hash:$h}')" | jq -r '.result.status'
}

stellar_address() { stellar keys address "$STELLAR_SOURCE"; }

contract_hex() { stellar strkey decode "$1" | jq -er '.contract'; }

# Ordered argument names of a contract function, read from the on-chain spec
# via the CLI's generated help. Circle documents the order, not the names.
contract_fn_args() {
  stellar contract invoke --id "$1" --network "$STELLAR_NETWORK" --source-account "$STELLAR_SOURCE" \
    -- "$2" --help 2>/dev/null \
    | grep -oE '^\s+--[a-z0-9_]+' | sed 's/^ *--//' | grep -vx 'help'
}

# Build + simulate + sign a Soroban invocation; prints the signed envelope XDR.
stellar_build_signed() {
  local id="$1"; shift
  stellar contract invoke --id "$id" --network "$STELLAR_NETWORK" \
      --source-account "$STELLAR_SOURCE" --build-only -- "$@" \
    | stellar tx simulate --network "$STELLAR_NETWORK" --source-account "$STELLAR_SOURCE" \
    | stellar tx sign --network "$STELLAR_NETWORK" --sign-with-key "$STELLAR_SOURCE"
}

# Idempotent submit for step <name>: the signed envelope and its hash are
# checkpointed BEFORE sending, so a rerun re-sends the same transaction (same
# sequence number, cannot execute twice) or just confirms it landed.
stellar_submit_step() {
  local step="$1"; shift
  local envelope hash status
  envelope="$(ckpt_get ".steps.\"$step\".envelope")"
  if [[ -z "$envelope" ]]; then
    envelope="$(stellar_build_signed "$@")"
    hash="$(echo "$envelope" | stellar tx hash --network "$STELLAR_NETWORK")"
    ckpt_set_str ".steps.\"$step\".envelope" "$envelope"
    ckpt_set_str ".steps.\"$step\".txHash" "$hash"
  fi
  hash="$(ckpt_get ".steps.\"$step\".txHash")"
  status="$(stellar_tx_status "$hash")"
  if [[ "$status" == "NOT_FOUND" ]]; then
    log "submitting ${step} (${hash})"
    echo "$envelope" | stellar tx send --network "$STELLAR_NETWORK" >/dev/null || true
    for _ in $(seq 1 30); do
      sleep 2
      status="$(stellar_tx_status "$hash")"
      [[ "$status" != "NOT_FOUND" ]] && break
    done
  fi
  case "$status" in
    SUCCESS) ckpt_set_str ".steps.\"$step\".ledgerStatus" "SUCCESS" ;;
    FAILED) die "${step} transaction ${hash} FAILED on Stellar; inspect it before any retry" ;;
    *) die "${step} transaction ${hash} not yet visible (status ${status}); rerun to resume" ;;
  esac
  echo "$hash"
}

# USDC balance (7-decimal raw) of a G/C address via the SAC.
stellar_usdc_balance() {
  stellar contract invoke --id "$(m '.networks["stellar-testnet"].usdc.contract')" \
    --network "$STELLAR_NETWORK" --source-account "$STELLAR_SOURCE" --send=no \
    -- balance --id "$1" | tr -d '"'
}

# A G account needs a USDC trustline before the forwarder can pay it.
require_stellar_trustline() {
  local recipient="$1" asset issuer
  [[ "$recipient" == C* ]] && return 0
  if [[ "$recipient" == M* ]]; then
    local key; key="$(stellar strkey decode "$recipient" | jq -r '.muxed_account_ed25519.ed25519')"
    recipient="$(stellar strkey encode "{\"public_key_ed25519\":\"${key}\"}")"
  fi
  asset="$(m '.networks["stellar-testnet"].usdc.asset')"
  issuer="${asset#USDC:}"
  curl -fsS "${HORIZON_URL}/accounts/${recipient}" \
    | jq -e --arg i "$issuer" '.balances[] | select(.asset_code=="USDC" and .asset_issuer==$i)' >/dev/null \
    || die "MissingTrustline: ${recipient} has no USDC trustline (issuer ${issuer}); add it before burning"
}

# ─── EVM ─────────────────────────────────────────────────────────────────────
evm_address() { cast wallet address --private-key "${EVM_PRIVATE_KEY:?set EVM_PRIVATE_KEY}"; }

# Idempotent EVM submit for step <name>: sign once (fixed nonce), checkpoint
# the raw tx and its hash, publish, wait. Rerun republishes the same raw tx.
evm_submit_step() {
  local step="$1" rpc="$2"; shift 2
  local raw hash status
  raw="$(ckpt_get ".steps.\"$step\".rawTx")"
  if [[ -z "$raw" ]]; then
    raw="$(cast mktx --rpc-url "$rpc" --private-key "${EVM_PRIVATE_KEY:?set EVM_PRIVATE_KEY}" "$@")"
    hash="$(cast keccak "$raw")"
    ckpt_set_str ".steps.\"$step\".rawTx" "$raw"
    ckpt_set_str ".steps.\"$step\".txHash" "$hash"
  fi
  hash="$(ckpt_get ".steps.\"$step\".txHash")"
  # --async: report "not found" instead of blocking on an unknown hash.
  if ! cast receipt "$hash" --async --rpc-url "$rpc" --json 2>/dev/null | jq -e '.status' >/dev/null; then
    log "publishing ${step} (${hash})"
    cast publish --rpc-url "$rpc" "$raw" >/dev/null 2>&1 || true
  fi
  status="$(cast receipt "$hash" --rpc-url "$rpc" --json --confirmations 2 | jq -r '.status')"
  [[ "$status" == "0x1" || "$status" == "1" ]] || die "${step} transaction ${hash} reverted; inspect it before any retry"
  ckpt_set_str ".steps.\"$step\".receiptStatus" "success"
  echo "$hash"
}

evm_usdc_balance() {  # evm_usdc_balance <network> <address>
  cast call --rpc-url "$(evm_rpc_for "$1")" "$(m ".networks[\"$1\"].usdc.address")" \
    "balanceOf(address)(uint256)" "$2" | awk '{print $1}'
}

# ─── Circle attestation (Iris) ───────────────────────────────────────────────
# Persists message + attestation for <step>. Delayed attestation just waits;
# a rerun resumes polling. Never burns again.
await_attestation() {
  local step="$1" source_domain="$2" tx_hash="$3"
  if [[ -n "$(ckpt_get ".steps.\"$step\".attestation")" ]]; then return 0; fi
  local deadline=$(( $(date +%s) + ATTESTATION_TIMEOUT_SECS )) body status
  while :; do
    body="$(curl -fsS "${IRIS_URL}/v2/messages/${source_domain}?transactionHash=${tx_hash}" || echo '{}')"
    status="$(echo "$body" | jq -r '.messages[0].status // "pending"')"
    if [[ "$status" == "complete" ]]; then
      ckpt_set_str ".steps.\"$step\".message" "$(echo "$body" | jq -r '.messages[0].message')"
      ckpt_set_str ".steps.\"$step\".attestation" "$(echo "$body" | jq -r '.messages[0].attestation')"
      ckpt_set ".steps.\"$step\".iris" "$(echo "$body" | jq -c '.messages[0] | del(.message, .attestation)')"
      return 0
    fi
    (( $(date +%s) < deadline )) || die "AttestationPending: still ${status} after ${ATTESTATION_TIMEOUT_SECS}s; rerun to keep waiting"
    log "attestation ${status}; waiting"
    sleep 10
  done
}

# Fields from a raw CCTP V2 message (hex, with or without 0x). Offsets match
# libs/crosschain-types/src/cctp.rs (header 148 bytes, burn body after it).
msg_hex() { local h="${1#0x}"; echo "${h:$(( $2 * 2 )):$(( $3 * 2 ))}"; }
msg_u32() { echo $(( 16#$(msg_hex "$1" "$2" 4) )); }
msg_u256() {
  local h; h="$(msg_hex "$1" "$2" 32)"
  [[ "${h:0:48}" =~ ^0+$ ]] || die "amount does not fit in 64 bits"
  echo $(( 16#${h:48:16} ))
}
msg_nonce() { echo "0x$(msg_hex "$1" 12 32)"; }
msg_amount() { msg_u256 "$1" $(( 148 + 68 )); }
msg_fee_executed() { msg_u256 "$1" $(( 148 + 164 )); }
msg_destination_domain() { msg_u32 "$1" 8; }
