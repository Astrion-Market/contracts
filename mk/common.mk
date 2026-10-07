NETWORK ?= testnet
SOURCE ?= deployer
WASM_DIR = target/wasm32v1-none/release
DEPLOY_DIR = deployments/$(NETWORK)
CONFIG ?= $(DEPLOY_DIR)/config.env
ADDRESSES ?= $(DEPLOY_DIR)/addresses.env
DEPLOY_FLAGS = --network $(NETWORK) --source $(SOURCE)

# ─── legacy-engine guard ──────────────────────────────────────────────────────
# Mutating legacy targets (deploy/init/upgrade/rotate) require an explicit
# GENERATION=legacy-soroban and an explicitly given NETWORK; a NETWORK that only
# comes from the default above is not forwarded. See ops/lib/legacy-guard.sh.
GENERATION ?=
ifneq ($(GENERATION),)
export ASTRION_GENERATION := $(GENERATION)
endif
ifneq ($(filter command line environment,$(origin NETWORK)),)
export ASTRION_TARGET_NETWORK := $(NETWORK)
endif
LEGACY_GUARD = bash -c 'source ops/lib/legacy-guard.sh && require_legacy_target "$$1"' _
