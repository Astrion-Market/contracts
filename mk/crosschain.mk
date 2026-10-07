# ─── cross-chain manifests and CCTP harness (deployments/crosschain, ops/crosschain)
ENV ?= testnet
WRITE ?=

.PHONY: crosschain-verify-manifest

# Record on-chain code hashes for manifest addresses. Report-only unless WRITE=1.
crosschain-verify-manifest:
	ops/crosschain/verify-manifest.sh $(ENV) $(if $(WRITE),--write,)

# ─── TypeScript SDK (sdk/) ────────────────────────────────────────────────────
.PHONY: sdk-test crosschain-test

sdk-test:
	cd sdk && bun test

# Rust spec/codec tests + TS SDK tests over the shared vectors.
crosschain-test:
	cargo test -p astrion-crosschain-types
	$(MAKE) sdk-test

# ─── CCTP testnet harness (real networks; operator wallets from env) ──────────
RUN ?=
.PHONY: cctp-stellar-to-evm cctp-evm-to-stellar cctp-status cctp-evidence

cctp-stellar-to-evm:
	@test -n "$(RUN)" || (echo "Usage: make cctp-stellar-to-evm RUN=<id> AMOUNT=<7dp raw> [MAX_FEE=] [EVM_NETWORK=base-sepolia]" && exit 1)
	AMOUNT=$(AMOUNT) MAX_FEE=$(MAX_FEE) ops/crosschain/cctp-transfer.sh stellar-to-evm $(RUN)

cctp-evm-to-stellar:
	@test -n "$(RUN)" || (echo "Usage: make cctp-evm-to-stellar RUN=<id> AMOUNT=<6dp raw> [MAX_FEE=] [STELLAR_RECIPIENT=]" && exit 1)
	AMOUNT=$(AMOUNT) MAX_FEE=$(MAX_FEE) ops/crosschain/cctp-transfer.sh evm-to-stellar $(RUN)

cctp-status:
	ops/crosschain/cctp-transfer.sh status $(RUN)

cctp-evidence:
	ops/crosschain/evidence.sh $(RUN)

.PHONY: sdk-abi sdk-vectors
sdk-abi:
	cd sdk && bun install --frozen-lockfile && bun scripts/gen-abi.ts

sdk-vectors:
	cd sdk && bun scripts/gen-vectors.ts
