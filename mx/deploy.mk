# Deployment workflows.

DRY_RUN ?= 0

.PHONY: deploy deploy-all deploy-contract deploy-force deploy-testnet deploy-mainnet deploy-local addresses

deploy: deploy-all

deploy-all: preflight
	CONTRACT="$(CONTRACT)" DRY_RUN="$(DRY_RUN)" bash scripts/deploy.sh "$(NETWORK)" "$(SOURCE)"

deploy-force: preflight
	CONTRACT="$(CONTRACT)" FORCE=1 DRY_RUN="$(DRY_RUN)" bash scripts/deploy.sh "$(NETWORK)" "$(SOURCE)"

deploy-contract: preflight build
	@test -n "$(CONTRACT)" || { printf '%s\n' 'Usage: make deploy-contract CONTRACT=reader NETWORK=testnet SOURCE=deployer'; exit 1; }
	@test -f "$(WASM_DIR)/$(CONTRACT).wasm" || { printf 'Missing wasm: %s/%s.wasm\n' "$(WASM_DIR)" "$(CONTRACT)"; exit 1; }
	wasm_hash="$$(stellar contract upload --wasm "$(WASM_DIR)/$(CONTRACT).wasm" --source "$(SOURCE)" --network "$(NETWORK)")"
	contract_id="$$(stellar contract deploy --wasm-hash "$$wasm_hash" --source "$(SOURCE)" --network "$(NETWORK)")"
	printf '%s deployed at %s\n' "$(CONTRACT)" "$$contract_id"
	printf '%s\n' 'This standalone deploy did not update $(DEPLOY_ENV). Initialize and wire it manually, or use upgrade-contract for an existing protocol deployment.'

deploy-testnet:
	$(MAKE) deploy-all NETWORK=testnet SOURCE="$(SOURCE)"

deploy-mainnet:
	$(MAKE) deploy-all NETWORK=mainnet SOURCE="$(SOURCE)"

deploy-local:
	$(MAKE) deploy-all NETWORK=local SOURCE="$(SOURCE)"

addresses:
	@test -f "$(DEPLOY_ENV)" || { printf 'Missing %s. Run make deploy-all first.\n' "$(DEPLOY_ENV)"; exit 1; }
	@sed -n '1,220p' "$(DEPLOY_ENV)"
