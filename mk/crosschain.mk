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
