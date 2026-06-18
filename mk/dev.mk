.PHONY: dev-wallets dev-flow

dev-wallets:
	ops/dev-flow.sh wallets $(NETWORK)

dev-flow:
	ops/dev-flow.sh flow $(NETWORK) $(SOURCE) $(CONFIG) $(ADDRESSES)
