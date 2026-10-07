#!/usr/bin/env bash
# Render a redacted, publishable evidence page from a run checkpoint.
#
# Usage: ops/crosschain/evidence.sh <run-id>
# Output: docs/evidence/cctp/<run-id>.md
#
# Tx hashes, domains, nonces and amounts are public and kept. Wallet
# addresses are shortened. Signed envelopes, raw txs, messages and
# attestations stay in the (gitignored) checkpoint.

set -euo pipefail
source ops/crosschain/lib.sh

RUN_ID="${1:?usage: evidence.sh <run-id>}"
f="$(ckpt_file)"
[[ -f "$f" ]] || die "no run ${RUN_ID}"
jq -e '.steps.reconcile.status == "done"' "$f" >/dev/null \
  || die "run ${RUN_ID} is not reconciled; only completed runs produce evidence"

short() { local s="$1"; [[ ${#s} -gt 12 ]] && echo "${s:0:6}…${s: -4}" || echo "$s"; }
explorer() {  # explorer <network> <hash>
  case "$1" in
    stellar-testnet) echo "https://stellar.expert/explorer/testnet/tx/${2#0x}" ;;
    base-sepolia) echo "https://sepolia.basescan.org/tx/$2" ;;
    ethereum-sepolia) echo "https://sepolia.etherscan.io/tx/$2" ;;
  esac
}

direction="$(jq -r .direction "$f")"
evm_net="$(jq -r .params.evmNetwork "$f")"
if [[ "$direction" == "stellar-to-evm" ]]; then
  src_net="stellar-testnet"; dst_net="$evm_net"
  sender="$(jq -r .params.stellarSource "$f")"; recipient="$(jq -r .params.evmRecipient "$f")"
else
  src_net="$evm_net"; dst_net="stellar-testnet"
  sender="$(jq -r .params.evmSender "$f")"; recipient="$(jq -r .params.stellarRecipient "$f")"
fi

row() {  # row <step> <network>
  local h; h="$(jq -r ".steps.\"$1\".txHash // empty" "$f")"
  local note; note="$(jq -r ".steps.\"$1\".note // empty" "$f")"
  if [[ -n "$h" ]]; then echo "| $1 | ${2} | [\`$(short "$h")\`]($(explorer "$2" "$h")) | ${note} |"
  else echo "| $1 | ${2} | — | ${note} |"; fi
}

out="docs/evidence/cctp/${RUN_ID}.md"
mkdir -p "$(dirname "$out")"
{
  echo "# CCTP testnet evidence: ${RUN_ID}"
  echo ""
  echo "| | |"
  echo "|---|---|"
  echo "| Direction | \`${direction}\` (${src_net} → ${dst_net}) |"
  echo "| Sender | \`$(short "$sender")\` |"
  echo "| Recipient | \`$(short "$recipient")\` |"
  echo "| CCTP nonce | \`$(msg_nonce "$(jq -r .steps.attest.message "$f")")\` |"
  echo "| Started / reconciled | $(jq -r .createdAt "$f") / $(jq -r .steps.reconcile.at "$f") |"
  echo "| Harness commit | \`$(git rev-parse --short HEAD 2>/dev/null || echo unknown)\` |"
  echo ""
  echo "## Transactions"
  echo ""
  echo "| Step | Network | Tx | Note |"
  echo "|---|---|---|---|"
  row approve "$src_net"
  row burn "$src_net"
  row mint "$dst_net"
  echo ""
  echo "## Amount reconciliation"
  echo ""
  echo '```json'
  jq '.reconciliation' "$f"
  echo '```'
  echo ""
  echo "Testnet transport evidence only. It says nothing about lending, mainnet"
  echo "liquidity or fees. Lending behaviour is covered separately by fork tests."
} > "$out"
echo "Evidence written → ${out}"
