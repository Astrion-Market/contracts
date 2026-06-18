.PHONY: deploy-one deploy-contract deploy-all init-all \
        dry-run status verify upgrade upgrade-all rotate-admin \
        deploy-testnet init-testnet verify-testnet \
        deploy-mainnet init-mainnet verify-mainnet

deploy-one:
	@test -n "$(CONTRACT)" || (echo "Usage: make deploy-one CONTRACT=oracle-adapter SOURCE=deployer NETWORK=testnet" && exit 1)
	@case "$(CONTRACT)" in \
		oracle-adapter) $(MAKE) deploy-contract CONTRACT=oracle-adapter WASM=oracle_adapter ALIAS=oracle-adapter NETWORK=$(NETWORK) SOURCE=$(SOURCE) ;; \
		interest-rate-model|rate-model) $(MAKE) deploy-contract CONTRACT=interest-rate-model WASM=interest_rate_model ALIAS=rate-model NETWORK=$(NETWORK) SOURCE=$(SOURCE) ;; \
		core-pool) $(MAKE) deploy-contract CONTRACT=core-pool WASM=core_pool ALIAS=core-pool NETWORK=$(NETWORK) SOURCE=$(SOURCE) ;; \
		liquidation-engine) $(MAKE) deploy-contract CONTRACT=liquidation-engine WASM=liquidation_engine ALIAS=liquidation-engine NETWORK=$(NETWORK) SOURCE=$(SOURCE) ;; \
		market) $(MAKE) deploy-contract CONTRACT=market WASM=market ALIAS=market NETWORK=$(NETWORK) SOURCE=$(SOURCE) ;; \
		market-factory) $(MAKE) deploy-contract CONTRACT=market-factory WASM=market_factory ALIAS=market-factory NETWORK=$(NETWORK) SOURCE=$(SOURCE) ;; \
		hello-world) $(MAKE) deploy-contract CONTRACT=hello-world WASM=hello_world ALIAS=hello-world NETWORK=$(NETWORK) SOURCE=$(SOURCE) ;; \
		*) echo "Unknown CONTRACT=$(CONTRACT). Use oracle-adapter, interest-rate-model, core-pool, liquidation-engine, market, market-factory, or hello-world."; exit 1 ;; \
	esac

deploy-contract: build-contract
	@test -n "$(WASM)" || (echo "Usage: make deploy-contract CONTRACT=oracle-adapter WASM=oracle_adapter ALIAS=oracle-adapter" && exit 1)
	@test -n "$(ALIAS)" || (echo "Usage: make deploy-contract CONTRACT=oracle-adapter WASM=oracle_adapter ALIAS=oracle-adapter" && exit 1)
	@wasm="$(WASM_DIR)/$(WASM).wasm"; \
	if [ -f "$(WASM_DIR)/$(WASM).optimized.wasm" ]; then wasm="$(WASM_DIR)/$(WASM).optimized.wasm"; fi; \
	stellar contract deploy --wasm "$$wasm" $(DEPLOY_FLAGS) --alias $(ALIAS)

deploy-all:
	ops/deploy-all.sh $(NETWORK) $(SOURCE)

init-all:
	ops/init-all.sh $(NETWORK) $(SOURCE) $(CONFIG) $(ADDRESSES)

# Preview what deploy-all would do — no contracts are touched.
dry-run:
	DRYRUN=1 ops/deploy-all.sh $(NETWORK) $(SOURCE)

# Show the current deployment state for NETWORK (default: testnet).
status:
	DEPLOY_DIR=$(DEPLOY_DIR) ops/state.sh $(NETWORK)

# Verify deployed contracts: WASM hashes + health-check invocations.
verify:
	ops/verify-deploy.sh $(NETWORK) $(SOURCE)

# Upgrade a single contract in-place (requires CONTRACT=<alias>).
upgrade:
	@test -n "$(CONTRACT)" || (echo "Usage: make upgrade CONTRACT=oracle-adapter [NETWORK=testnet] [SOURCE=deployer]" && exit 1)
	ops/upgrade-contract.sh $(CONTRACT) $(NETWORK) $(SOURCE)

# Upgrade all deployed contracts — builds, diffs WASMs, single confirm, loops.
# Contracts already at the current build are skipped automatically.
upgrade-all:
	ops/upgrade-all.sh $(NETWORK) $(SOURCE)

# Transfer admin to a new address on all contracts (requires NEW_ADMIN=G...).
rotate-admin:
	@test -n "$(NEW_ADMIN)" || (echo "Usage: make rotate-admin NEW_ADMIN=G... [NETWORK=testnet] [SOURCE=deployer]" && exit 1)
	ops/rotate-admin.sh $(NEW_ADMIN) $(NETWORK) $(SOURCE)

# ─── named network targets ────────────────────────────────────────────────────
# Testnet: deploys protocol contracts + test tokens, then initializes all.
deploy-testnet:
	ops/deploy-all.sh testnet $(SOURCE)

init-testnet:
	ops/init-all.sh testnet $(SOURCE) \
	  deployments/testnet/config.env \
	  deployments/testnet/addresses.env

verify-testnet:
	ops/verify-deploy.sh testnet $(SOURCE)

# Mainnet: no test tokens, requires NETWORK env guard inside ops scripts.
# Use GitHub Actions cd-mainnet workflow for production; this target is for
# manual emergency use only.
deploy-mainnet:
	@echo "┌──────────────────────────────────────────┐"
	@echo "│  MAINNET DEPLOYMENT — are you sure?      │"
	@echo "│  Press Enter to continue or Ctrl-C abort │"
	@echo "└──────────────────────────────────────────┘"
	@read _
	ops/deploy-all.sh mainnet $(SOURCE)

init-mainnet:
	ops/init-all.sh mainnet $(SOURCE) \
	  deployments/mainnet/config.env \
	  deployments/mainnet/addresses.env

verify-mainnet:
	ops/verify-deploy.sh mainnet $(SOURCE)
