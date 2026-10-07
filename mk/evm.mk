# ─── EVM execution layer (Foundry, evm/) ─────────────────────────────────────
# Kept separate from the Soroban targets in build.mk: `make test` stays Rust-only
# and `make evm-test` stays EVM-only, so a failure in one never hides the other.
EVM_DIR = evm
FORGE ?= forge

.PHONY: evm-deps evm-build evm-test evm-fork-test evm-fmt evm-fmt-check evm-clean

evm-deps:
	git submodule update --init $(EVM_DIR)/lib/forge-std $(EVM_DIR)/lib/openzeppelin-contracts

evm-build:
	cd $(EVM_DIR) && $(FORGE) build --sizes

# Unit tests only; never touches the network.
evm-test:
	cd $(EVM_DIR) && $(FORGE) test --no-match-path 'test/fork/*'

# Fork suites. Without BASE_RPC_URL / ETH_RPC_URL they report SKIPPED.
evm-fork-test:
	cd $(EVM_DIR) && $(FORGE) test --match-path 'test/fork/*' -vv

evm-fmt:
	cd $(EVM_DIR) && $(FORGE) fmt

evm-fmt-check:
	cd $(EVM_DIR) && $(FORGE) fmt --check

evm-clean:
	cd $(EVM_DIR) && $(FORGE) clean

# ─── alpha deployment (manifest-driven; mainnet needs release-approval.json) ──
.PHONY: evm-deploy evm-smoke route-gates
evm-deploy:
	@test -n "$(NETWORK_ID)" || (echo "Usage: make evm-deploy ASTRION_ENV=testnet NETWORK_ID=base-sepolia RPC=<url>" && exit 1)
	cd $(EVM_DIR) && ASTRION_ENV=$(ASTRION_ENV) NETWORK=$(NETWORK_ID) GIT_COMMIT=$$(git rev-parse HEAD) \
	  $(FORGE) script script/Deploy.s.sol --rpc-url $(RPC) --broadcast

evm-smoke:
	cd $(EVM_DIR) && NETWORK=$(NETWORK_ID) $(FORGE) script script/Smoke.s.sol --rpc-url $(RPC)

route-gates:
	ops/crosschain/route-gates.sh
