#!/usr/bin/env bash
# Render the six-way route release-gate report for mainnet routes:
# fork-suite result per protocol/chain, or the visible disabled reason.
#
# Usage: ops/crosschain/route-gates.sh            (runs fork suites; needs BASE_RPC_URL/ETH_RPC_URL)
# Output: docs/evidence/ROUTE_GATES.md

set -euo pipefail

manifest="deployments/crosschain/mainnet.json"
out="docs/evidence/ROUTE_GATES.md"
command -v jq >/dev/null || { echo "error: jq required" >&2; exit 1; }
command -v forge >/dev/null || { echo "error: forge required" >&2; exit 1; }

suites_for() {
  case "$1" in
    aave-v3:base) echo "AaveV3BaseForkTest AaveV3LifecycleForkTest" ;;
    morpho-blue:base) echo "MorphoBlueBaseForkTest MorphoBlueLifecycleForkTest" ;;
    compound-v3:base) echo "CompoundV3BaseForkTest CompoundV3LifecycleForkTest" ;;
    *) echo "" ;;
  esac
}

run_suite() {
  local contract="$1" json passed failed skipped
  json="$(cd evm && forge test --match-contract "^${contract}\$" --json 2>/dev/null || true)"
  passed="$(echo "$json" | jq '[.. | objects | select(has("status")) | select(.status=="Success")] | length' 2>/dev/null || echo 0)"
  failed="$(echo "$json" | jq '[.. | objects | select(has("status")) | select(.status=="Failure")] | length' 2>/dev/null || echo 0)"
  skipped="$(echo "$json" | jq '[.. | objects | select(has("status")) | select(.status=="Skipped")] | length' 2>/dev/null || echo 0)"
  if [[ "$failed" != "0" ]]; then echo "FAIL (${failed} failed)"
  elif [[ "$passed" != "0" ]]; then echo "PASS (${passed})"
  elif [[ "$skipped" != "0" ]]; then echo "SKIPPED (no RPC)"
  else echo "NOT RUN"; fi
}

commit="$(git rev-parse --short HEAD)"
{
  echo "# Route release gates (mainnet)"
  echo ""
  echo "Generated $(date -u +%Y-%m-%dT%H:%M:%SZ) at \`${commit}\` by \`ops/crosschain/route-gates.sh\`."
  echo "A route may be enabled only when its fork evidence passes, its manifest"
  echo "status is \`verified\`, and the mainnet release approval names this commit."
  echo ""
  echo "| Route | Manifest status | Enabled | Fork evidence | Disabled reason |"
  echo "|---|---|---|---|---|"
  while IFS=$'\t' read -r id status enabled reason; do
    suites="$(suites_for "$id")"
    if [[ -z "$suites" ]]; then
      evidence="no fork suite yet"
    else
      evidence=""
      for s in $suites; do evidence+="${s}: $(run_suite "$s")<br>"; done
    fi
    echo "| \`${id}\` | ${status} | ${enabled} | ${evidence} | ${reason} |"
  done < <(jq -r '.routes[] | [.id, .status, (.enabled|tostring), .reason] | @tsv' "$manifest")
} > "$out"
echo "Route gates → ${out}"
