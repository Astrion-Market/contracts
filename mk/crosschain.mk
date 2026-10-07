# ─── cross-chain manifests and CCTP harness (deployments/crosschain, ops/crosschain)
ENV ?= testnet
WRITE ?=

.PHONY: crosschain-verify-manifest

# Record on-chain code hashes for manifest addresses. Report-only unless WRITE=1.
crosschain-verify-manifest:
	ops/crosschain/verify-manifest.sh $(ENV) $(if $(WRITE),--write,)
