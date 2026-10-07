#!/usr/bin/env bash
# Guard for every script that writes to a network with the legacy Soroban
# lending engine (core-pool, liquidation-engine, market, vault, adapters).
#
# The legacy engine is frozen (docs/legacy/DEPLOYMENT_INVENTORY.md). It must
# never be deployed, upgraded or re-seeded by accident from the new cross-chain
# release path, so mutating scripts require an explicit generation AND an
# explicit network that matches the one the script resolved:
#
#   ASTRION_GENERATION=legacy-soroban ASTRION_TARGET_NETWORK=testnet ops/deploy-all.sh testnet deployer
#
# Through make:  make deploy-all GENERATION=legacy-soroban NETWORK=testnet
#
# Usage inside a script (after `network` is resolved):
#   source ops/lib/legacy-guard.sh
#   require_legacy_target "$network"

require_legacy_target() {
  local network="$1"
  # A dry run touches nothing, so it needs no explicit generation.
  if [[ "${DRYRUN:-0}" == "1" ]]; then
    return 0
  fi
  if [[ "${ASTRION_GENERATION:-}" != "legacy-soroban" ]]; then
    echo "error: refusing to touch the legacy Soroban lending engine without an explicit generation." >&2
    echo "       The legacy engine is frozen; see docs/legacy/DEPLOYMENT_INVENTORY.md." >&2
    echo "       Set ASTRION_GENERATION=legacy-soroban (make: GENERATION=legacy-soroban) to proceed." >&2
    exit 2
  fi
  if [[ -z "${ASTRION_TARGET_NETWORK:-}" ]]; then
    echo "error: no explicit target network. Set ASTRION_TARGET_NETWORK=${network} (make: NETWORK=${network})." >&2
    exit 2
  fi
  if [[ "${ASTRION_TARGET_NETWORK}" != "${network}" ]]; then
    echo "error: ASTRION_TARGET_NETWORK=${ASTRION_TARGET_NETWORK} does not match resolved network '${network}'." >&2
    exit 2
  fi
}
