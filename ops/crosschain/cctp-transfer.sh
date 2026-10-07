#!/usr/bin/env bash
# Reproducible, resumable CCTP V2 USDC transfers: Stellar testnet <-> Base
# Sepolia (or Ethereum Sepolia). Real networks only; there is no mock mode.
#
# Usage:
#   ops/crosschain/cctp-transfer.sh stellar-to-evm <run-id>
#   ops/crosschain/cctp-transfer.sh evm-to-stellar <run-id>
#   ops/crosschain/cctp-transfer.sh status <run-id>
#
# Rerunning the same <run-id> resumes from the last checkpoint. Signed
# transactions are checkpointed before they are sent, so a rerun re-sends the
# SAME transaction (or confirms it landed). It never creates a second burn or
# mint. A burn cannot be cancelled: once burned, the run continues to delivery.
#
# Environment (operator-provided; never written to disk):
#   STELLAR_SOURCE      stellar keys alias of the Stellar test wallet
#   EVM_PRIVATE_KEY     EVM test wallet key
#   BASE_SEPOLIA_RPC_URL / ETH_SEPOLIA_RPC_URL
# Parameters:
#   EVM_NETWORK         base-sepolia (default) | ethereum-sepolia
#   AMOUNT              raw USDC in SOURCE units (Stellar: 7 decimals, EVM: 6)
#   MAX_FEE             raw, source units (default 0 for standard transfer)
#   MIN_FINALITY        2000 standard (default) | 1000 fast
#   EVM_RECIPIENT       stellar-to-evm mint recipient (default: EVM wallet)
#   STELLAR_RECIPIENT   evm-to-stellar final recipient G/C/M (default: Stellar wallet)

set -euo pipefail
source ops/crosschain/lib.sh

cmd="${1:?usage: cctp-transfer.sh <stellar-to-evm|evm-to-stellar|status> <run-id>}"
RUN_ID="${2:?run id required}"
[[ "$RUN_ID" =~ ^[A-Za-z0-9._-]+$ ]] || die "run id must be [A-Za-z0-9._-]"

STELLAR_DOMAIN=27

stellar_to_evm() {
  require_tools
  : "${STELLAR_SOURCE:?set STELLAR_SOURCE}"
  local evm_net rpc evm_domain usdc_c tmm mt_evm
  evm_net="$(evm_network_for "${EVM_NETWORK:-base-sepolia}")"
  rpc="$(evm_rpc_for "$evm_net")"
  evm_domain="$(m ".networks[\"$evm_net\"].cctpDomain")"
  usdc_c="$(m '.networks["stellar-testnet"].usdc.contract')"
  tmm="$(m '.networks["stellar-testnet"].cctp.tokenMessengerMinter')"
  mt_evm="$(m ".networks[\"$evm_net\"].cctp.messageTransmitterV2")"
  ckpt_init "stellar-to-evm"

  # 1. preflight: freeze parameters and starting balances once.
  if ! step_done preflight; then
    local amount="${AMOUNT:?set AMOUNT (7-decimal Stellar raw units)}" max_fee="${MAX_FEE:-0}"
    require_uint "$amount"; require_uint "$max_fee"
    local burned=$(( amount / 10 )) dust=$(( amount % 10 ))
    (( burned > 0 )) || die "DustOnly: ${amount} is below one 6-decimal unit"
    (( max_fee < amount )) || die "FeeExceedsAmount"
    local source recipient
    source="$(stellar_address)"
    recipient="${EVM_RECIPIENT:-$(evm_address)}"
    [[ "$recipient" =~ ^0x[0-9a-fA-F]{40}$ ]] || die "invalid EVM_RECIPIENT"
    ckpt_set .params "$(jq -n --arg net "$evm_net" --arg src "$source" --arg rcp "$recipient" \
      --arg a "$amount" --arg f "$max_fee" --arg b "$burned" --arg d "$dust" \
      --argjson fin "${MIN_FINALITY:-2000}" --argjson dom "$evm_domain" \
      '{evmNetwork:$net, destinationDomain:$dom, stellarSource:$src, evmRecipient:$rcp,
        amount7:$a, maxFee7:$f, expectedBurned6:$b, retainedDust7:$d, minFinality:$fin}')"
    ckpt_set .before "$(jq -n --arg s "$(stellar_usdc_balance "$source")" \
      --arg e "$(evm_usdc_balance "$evm_net" "$recipient")" '{stellarSource7:$s, evmRecipient6:$e}')"
    mark_done preflight
  fi
  local source recipient amount max_fee fin
  source="$(ckpt_get .params.stellarSource)"; recipient="$(ckpt_get .params.evmRecipient)"
  amount="$(ckpt_get .params.amount7)"; max_fee="$(ckpt_get .params.maxFee7)"; fin="$(ckpt_get .params.minFinality)"

  # 2. approve exactly AMOUNT to TokenMessengerMinter.
  if ! step_done approve; then
    local ledger; ledger="$(stellar_rpc getLatestLedger '{}' | jq -r '.result.sequence')"
    stellar_submit_step approve "$usdc_c" approve --from "$source" --spender "$tmm" \
      --amount "$amount" --expiration_ledger "$(( ledger + 1000 ))" >/dev/null
    mark_done approve
  fi

  # 3. burn. Argument ORDER is documented by Circle; names come from the spec.
  if ! step_done burn; then
    mapfile -t names < <(contract_fn_args "$tmm" deposit_for_burn)
    (( ${#names[@]} == 8 )) || die "deposit_for_burn has ${#names[@]} args (expected 8): ${names[*]}"
    local mint_recipient="000000000000000000000000${recipient#0x}"
    local values=("$source" "$amount" "$evm_domain" "${mint_recipient,,}" "$usdc_c" \
                  "0000000000000000000000000000000000000000000000000000000000000000" "$max_fee" "$fin")
    local args=() i
    for i in "${!names[@]}"; do args+=("--${names[$i]}" "${values[$i]}"); done
    ckpt_set .steps.burn.argNames "$(printf '%s\n' "${names[@]}" | jq -R . | jq -s .)"
    stellar_submit_step burn "$tmm" deposit_for_burn "${args[@]}" >/dev/null
    mark_done burn
  fi

  # 4. attestation (resumable wait).
  if ! step_done attest; then
    await_attestation attest "$STELLAR_DOMAIN" "$(ckpt_get .steps.burn.txHash)"
    local msg; msg="$(ckpt_get .steps.attest.message)"
    [[ "$(msg_destination_domain "$msg")" == "$evm_domain" ]] || die "ReconciliationMismatch: destination domain"
    [[ "$(msg_amount "$msg")" == "$(ckpt_get .params.expectedBurned6)" ]] || die "ReconciliationMismatch: burned amount"
    local want="000000000000000000000000${recipient#0x}"
    [[ "$(msg_hex "$msg" $(( 148 + 36 )) 32)" == "${want,,}" ]] || die "ReconciliationMismatch: mintRecipient"
    mark_done attest
  fi

  # 5. mint on EVM, unless the nonce is already used (minted by anyone).
  if ! step_done mint; then
    local msg att nonce used
    msg="$(ckpt_get .steps.attest.message)"; att="$(ckpt_get .steps.attest.attestation)"
    nonce="$(msg_nonce "$msg")"
    used="$(cast call --rpc-url "$rpc" "$mt_evm" "usedNonces(bytes32)(uint256)" "$nonce" | awk '{print $1}')"
    if [[ "$used" != "0" && -z "$(ckpt_get .steps.mint.txHash)" ]]; then
      log "nonce ${nonce} already used: mint completed by another submitter; not resubmitting"
      ckpt_set_str .steps.mint.note "nonce already used before this run submitted a mint"
    else
      evm_submit_step mint "$rpc" "$mt_evm" "receiveMessage(bytes,bytes)" "$msg" "$att" >/dev/null
    fi
    mark_done mint
  fi

  # 6. reconcile actual balances against the message.
  if ! step_done reconcile; then
    local msg fee burned dust before_s after_s before_e after_e
    msg="$(ckpt_get .steps.attest.message)"
    fee="$(msg_fee_executed "$msg")"; burned="$(msg_amount "$msg")"
    dust="$(ckpt_get .params.retainedDust7)"
    before_s="$(ckpt_get .before.stellarSource7)"; after_s="$(stellar_usdc_balance "$source")"
    before_e="$(ckpt_get .before.evmRecipient6)"; after_e="$(evm_usdc_balance "$(ckpt_get .params.evmNetwork)" "$recipient")"
    ckpt_set .reconciliation "$(jq -n \
      --arg sent "$amount" --arg burned "$burned" --arg dust "$dust" --arg fee "$fee" \
      --arg expS "$(( burned * 10 ))" --arg obsS "$(( before_s - after_s ))" \
      --arg expE "$(( burned - fee ))" --arg obsE "$(( after_e - before_e ))" \
      '{sent7:$sent, burned6:$burned, retainedDust7:$dust, feeExecuted6:$fee,
        expectedStellarDebit7:$expS, observedStellarDebit7:$obsS,
        expectedEvmReceived6:$expE, observedEvmReceived6:$obsE,
        matches:(($expS==$obsS) and ($expE==$obsE)),
        note:"Observed deltas include any unrelated transfers during the run."}')"
    mark_done reconcile
  fi
  status
}

evm_to_stellar() {
  require_tools
  : "${STELLAR_SOURCE:?set STELLAR_SOURCE}"
  local evm_net rpc evm_domain usdc_e tm_evm fwd mt_s
  evm_net="$(evm_network_for "${EVM_NETWORK:-base-sepolia}")"
  rpc="$(evm_rpc_for "$evm_net")"
  evm_domain="$(m ".networks[\"$evm_net\"].cctpDomain")"
  usdc_e="$(m ".networks[\"$evm_net\"].usdc.address")"
  tm_evm="$(m ".networks[\"$evm_net\"].cctp.tokenMessengerV2")"
  fwd="$(m '.networks["stellar-testnet"].cctp.cctpForwarder')"
  mt_s="$(m '.networks["stellar-testnet"].cctp.messageTransmitter')"
  ckpt_init "evm-to-stellar"

  # 1. preflight: destination checks happen BEFORE any burn.
  if ! step_done preflight; then
    local amount="${AMOUNT:?set AMOUNT (6-decimal EVM raw units)}" max_fee="${MAX_FEE:-0}"
    require_uint "$amount"; require_uint "$max_fee"
    (( amount > 0 && max_fee < amount )) || die "FeeExceedsAmount or zero amount"
    local recipient sender balance_holder
    recipient="${STELLAR_RECIPIENT:-$(stellar_address)}"
    stellar strkey decode "$recipient" >/dev/null 2>&1 || die "InvalidStrkey: STELLAR_RECIPIENT"
    [[ "$recipient" =~ ^[GCM] ]] || die "UnsupportedStrkeyKind: STELLAR_RECIPIENT"
    require_stellar_trustline "$recipient"
    balance_holder="$recipient"
    if [[ "$recipient" == M* ]]; then
      balance_holder="$(stellar strkey encode "{\"public_key_ed25519\":\"$(stellar strkey decode "$recipient" | jq -r '.muxed_account_ed25519.ed25519')\"}")"
    fi
    sender="$(evm_address)"
    local rbytes hook
    rbytes="$(printf '%s' "$recipient" | od -An -tx1 | tr -d ' \n')"
    hook="0x$(printf '%048d' 0)00000000$(printf '%08x' "${#recipient}")${rbytes}"
    ckpt_set .params "$(jq -n --arg net "$evm_net" --arg snd "$sender" --arg rcp "$recipient" \
      --arg hold "$balance_holder" --arg a "$amount" --arg f "$max_fee" --arg hook "$hook" \
      --arg fwd "0x$(contract_hex "$fwd")" --argjson fin "${MIN_FINALITY:-2000}" --argjson dom "$evm_domain" \
      '{evmNetwork:$net, sourceDomain:$dom, evmSender:$snd, stellarRecipient:$rcp, balanceHolder:$hold,
        amount6:$a, maxFee6:$f, hookData:$hook, forwarderBytes32:$fwd, minFinality:$fin}')"
    ckpt_set .before "$(jq -n --arg e "$(evm_usdc_balance "$evm_net" "$sender")" \
      --arg s "$(stellar_usdc_balance "$balance_holder")" '{evmSender6:$e, stellarRecipient7:$s}')"
    mark_done preflight
  fi
  local amount max_fee fin hook fwd32
  amount="$(ckpt_get .params.amount6)"; max_fee="$(ckpt_get .params.maxFee6)"
  fin="$(ckpt_get .params.minFinality)"; hook="$(ckpt_get .params.hookData)"
  fwd32="$(ckpt_get .params.forwarderBytes32)"

  # 2. approve exactly AMOUNT to TokenMessengerV2.
  if ! step_done approve; then
    evm_submit_step approve "$rpc" "$usdc_e" "approve(address,uint256)" "$tm_evm" "$amount" >/dev/null
    mark_done approve
  fi

  # 3. burn with forwarder as BOTH mintRecipient and destinationCaller.
  if ! step_done burn; then
    evm_submit_step burn "$rpc" "$tm_evm" \
      "depositForBurnWithHook(uint256,uint32,bytes32,address,bytes32,uint256,uint32,bytes)" \
      "$amount" "$STELLAR_DOMAIN" "$fwd32" "$usdc_e" "$fwd32" "$max_fee" "$fin" "$hook" >/dev/null
    mark_done burn
  fi

  # 4. attestation.
  if ! step_done attest; then
    await_attestation attest "$evm_domain" "$(ckpt_get .steps.burn.txHash)"
    local msg; msg="$(ckpt_get .steps.attest.message)"
    [[ "$(msg_destination_domain "$msg")" == "$STELLAR_DOMAIN" ]] || die "ReconciliationMismatch: destination domain"
    [[ "0x$(msg_hex "$msg" $(( 148 + 36 )) 32)" == "$fwd32" ]] || die "ReconciliationMismatch: mintRecipient is not the forwarder"
    [[ "0x$(msg_hex "$msg" 108 32)" == "$fwd32" ]] || die "ReconciliationMismatch: destinationCaller is not the forwarder"
    mark_done attest
  fi

  # 5. mint_and_forward on Stellar, unless the nonce is already used.
  if ! step_done mint; then
    local msg att nonce
    msg="$(ckpt_get .steps.attest.message)"; att="$(ckpt_get .steps.attest.attestation)"
    nonce="$(msg_nonce "$msg")"
    mapfile -t nargs < <(contract_fn_args "$mt_s" is_nonce_used)
    local used="unknown"
    if (( ${#nargs[@]} == 1 )); then
      used="$(stellar contract invoke --id "$mt_s" --network "$STELLAR_NETWORK" --source-account "$STELLAR_SOURCE" \
        --send=no -- is_nonce_used "--${nargs[0]}" "${nonce#0x}" 2>/dev/null || echo unknown)"
    else
      log "is_nonce_used signature has ${#nargs[@]} args; skipping pre-check (the forwarder rejects reuse)"
    fi
    if [[ "$used" == "true" && -z "$(ckpt_get .steps.mint.txHash)" ]]; then
      log "nonce ${nonce} already used: delivered by another submitter; not resubmitting"
      ckpt_set_str .steps.mint.note "nonce already used before this run submitted mint_and_forward"
    else
      stellar_submit_step mint "$(m '.networks["stellar-testnet"].cctp.cctpForwarder')" mint_and_forward \
        --message "${msg#0x}" --attestation "${att#0x}" >/dev/null
    fi
    mark_done mint
  fi

  # 6. reconcile.
  if ! step_done reconcile; then
    local msg fee burned holder before_s after_s before_e after_e sender
    msg="$(ckpt_get .steps.attest.message)"
    fee="$(msg_fee_executed "$msg")"; burned="$(msg_amount "$msg")"
    holder="$(ckpt_get .params.balanceHolder)"; sender="$(ckpt_get .params.evmSender)"
    before_s="$(ckpt_get .before.stellarRecipient7)"; after_s="$(stellar_usdc_balance "$holder")"
    before_e="$(ckpt_get .before.evmSender6)"; after_e="$(evm_usdc_balance "$(ckpt_get .params.evmNetwork)" "$sender")"
    ckpt_set .reconciliation "$(jq -n \
      --arg sent "$amount" --arg burned "$burned" --arg fee "$fee" \
      --arg expE "$burned" --arg obsE "$(( before_e - after_e ))" \
      --arg expS "$(( (burned - fee) * 10 ))" --arg obsS "$(( after_s - before_s ))" \
      '{sent6:$sent, burned6:$burned, feeExecuted6:$fee,
        expectedEvmDebit6:$expE, observedEvmDebit6:$obsE,
        expectedStellarReceived7:$expS, observedStellarReceived7:$obsS,
        matches:(($expE==$obsE) and ($expS==$obsS)),
        note:"Observed deltas include any unrelated transfers during the run."}')"
    mark_done reconcile
  fi
  status
}

status() {
  local f; f="$(ckpt_file)"
  [[ -f "$f" ]] || die "no run ${RUN_ID}"
  jq '{runId, direction, updatedAt,
       steps: (.steps | with_entries(.value |= {status, txHash, note} | del(..|nulls))),
       reconciliation}' "$f"
}

case "$cmd" in
  stellar-to-evm) stellar_to_evm ;;
  evm-to-stellar) evm_to_stellar ;;
  status) status ;;
  *) die "unknown command ${cmd}" ;;
esac
