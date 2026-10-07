#!/usr/bin/env bash
# Record on-chain code hashes for every EVM address in a cross-chain manifest.
#
# Usage:
#   ops/crosschain/verify-manifest.sh <mainnet|testnet> [--write]
#
# RPC URLs come from the environment only:
#   mainnet: BASE_RPC_URL, ETH_RPC_URL
#   testnet: BASE_SEPOLIA_RPC_URL, ETH_SEPOLIA_RPC_URL
#
# Without --write it prints a report. With --write it sets codeHash, checkedAt
# and status "bytecode-verified" on records whose addresses all have code.
# Having code is necessary, not sufficient: parameters, caps, liquidity and
# market approval are still reviewed by a person before any route is enabled.

set -euo pipefail

env_name="${1:?usage: verify-manifest.sh <mainnet|testnet> [--write]}"
write="${2:-}"
manifest="deployments/crosschain/${env_name}.json"
[[ -f "$manifest" ]] || { echo "error: $manifest not found" >&2; exit 1; }
command -v cast >/dev/null || { echo "error: cast (Foundry) is required" >&2; exit 1; }
command -v jq >/dev/null || { echo "error: jq is required" >&2; exit 1; }

rpc_for() {
  case "$1" in
    base) echo "${BASE_RPC_URL:-}" ;;
    ethereum) echo "${ETH_RPC_URL:-}" ;;
    base-sepolia) echo "${BASE_SEPOLIA_RPC_URL:-}" ;;
    ethereum-sepolia) echo "${ETH_SEPOLIA_RPC_URL:-}" ;;
    *) echo "" ;;
  esac
}

now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
tmp="$(mktemp)"
cp "$manifest" "$tmp"
status=0

# (jq record path, network) for every EVM record that carries "verification".
while IFS=$'\t' read -r path net; do
  rpc="$(rpc_for "$net")"
  if [[ -z "$rpc" ]]; then
    echo "SKIPPED  ${net}  ${path}  (no RPC URL in environment)"
    continue
  fi
  hashes=()
  missing=0
  while read -r addr; do
    code_hash="$(cast codehash "$addr" --rpc-url "$rpc" 2>/dev/null || echo "error")"
    if [[ "$code_hash" == "error" || "$code_hash" == "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470" || "$code_hash" == "0x0000000000000000000000000000000000000000000000000000000000000000" ]]; then
      echo "NO CODE  ${net}  ${addr}"
      missing=1
      status=1
    else
      echo "OK       ${net}  ${addr}  ${code_hash}"
      hashes+=("${addr}=${code_hash}")
    fi
  done < <(jq -r "${path} | .. | strings | select(test(\"^0x[0-9a-fA-F]{40}\$\"))" "$manifest" | sort -u)

  if [[ "$write" == "--write" && "$missing" == "0" && "${#hashes[@]}" -gt 0 ]]; then
    joined="$(IFS=,; echo "${hashes[*]}")"
    jq --arg now "$now" --arg h "$joined" \
      "(${path}.verification) |= (. + {status: \"bytecode-verified\", checkedAt: \$now, codeHash: \$h})" \
      "$tmp" > "${tmp}.next" && mv "${tmp}.next" "$tmp"
  fi
done < <(jq -r '
  (.networks | to_entries[] | select(.value.kind == "evm") | .key as $k |
     ([".networks[\"" + $k + "\"].usdc", $k], [".networks[\"" + $k + "\"].cctp", $k])),
  (.routes | to_entries[] | select(.value.verification != null) |
     [".routes[" + (.key | tostring) + "]", .value.network])
  | @tsv' "$manifest")

if [[ "$write" == "--write" ]]; then
  mv "$tmp" "$manifest"
  echo "Updated ${manifest}"
else
  rm -f "$tmp"
fi
exit "$status"
