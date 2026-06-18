NETWORK ?= testnet
SOURCE ?= deployer
WASM_DIR = target/wasm32v1-none/release
DEPLOY_DIR = deployments/$(NETWORK)
CONFIG ?= $(DEPLOY_DIR)/config.env
ADDRESSES ?= $(DEPLOY_DIR)/addresses.env
DEPLOY_FLAGS = --network $(NETWORK) --source $(SOURCE)
