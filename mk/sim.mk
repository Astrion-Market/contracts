.PHONY: sim-setup sim-run sim-loop sim-status sim-reset

# ─── sim-setup ────────────────────────────────────────────────────────────────
# Generate wallets, deploy SACs, mint tokens.
sim-setup:
	@bash sim/setup.sh $(NETWORK)

# ─── sim-run ─────────────────────────────────────────────────────────────────
# Execute one simulation round (role rotation + oracle + rate + protocol calls).
sim-run:
	@bash sim/run.sh $(NETWORK) $(ADDRESSES)

# ─── sim-loop ────────────────────────────────────────────────────────────────
# Run N rounds with DELAY seconds between each.
# Usage: make sim-loop ROUNDS=10 DELAY=30
ROUNDS ?= 5
DELAY  ?= 15

sim-loop:
	@r=0; \
	while [ $$r -lt $(ROUNDS) ]; do \
	  r=$$((r+1)); \
	  printf '\n[sim-loop] round %d of $(ROUNDS)\n' $$r; \
	  bash sim/run.sh $(NETWORK) $(ADDRESSES); \
	  if [ $$r -lt $(ROUNDS) ]; then \
	    printf '[sim-loop] waiting $(DELAY)s…\n'; \
	    sleep $(DELAY); \
	  fi; \
	done; \
	printf '\n[sim-loop] done — ran $(ROUNDS) rounds.\n'

# ─── sim-status ───────────────────────────────────────────────────────────────
# Print current sim state (round counter, wallets, token IDs).
sim-status:
	@echo ""; \
	echo "── Simulation state ──────────────────────────────────────────"; \
	if [ -f sim/state.env ];   then cat sim/state.env;   fi; \
	echo ""; \
	if [ -f sim/wallets.env ]; then cat sim/wallets.env; fi; \
	echo ""; \
	if [ -f sim/tokens.env ];  then cat sim/tokens.env;  fi; \
	echo ""

# ─── sim-reset ────────────────────────────────────────────────────────────────
# Reset the round counter (does NOT delete wallets or SACs).
sim-reset:
	@if [ -f sim/state.env ]; then \
	  sed -i 's/^SIM_ROUND=.*/SIM_ROUND=0/' sim/state.env; \
	  echo "Round counter reset to 0."; \
	else \
	  echo "sim/state.env not found — nothing to reset."; \
	fi
