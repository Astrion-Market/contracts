# Incident runbook

| Situation | First action | Never |
|---|---|---|
| Protocol incident (oracle, exploit, pause) | Guardian pauses the route. Check the protocol's own pause state via the lens | Ask users to bridge more funds to "save" positions |
| Bridge delay / attestation stuck | Leave it running. `cctp-transfer.sh` / the relayer keep polling. Publish an ETA only from Circle status | Re-burn. A burn cannot be cancelled and a consumed nonce is never retried |
| Funded action failed after mint | USDC sits in the user's account (`FundedActionFailed`). User signs a fresh intent bound to the same `transferId`, or exits directly | Move the funds anywhere on the user's behalf |
| Relayer outage | Users run `services/relayer` `self` mode or `AccountOps.s.sol` | Hand out relayer keys |
| Relayer key compromise | Rotate the key. Old key holds gas only. Intents bind `relayer` optionally | Assume user funds are at risk. They are not reachable by the relayer |
| Sponsorship drain attempt | Lower `OWNER_DAILY_BUDGET_WEI` / `GLOBAL_DAILY_BUDGET_WEI`. Forged jobs never reach signing | Disable signature checks to "unstick" jobs |
| Liquidation risk while repayment is in transit | Tell users to repay directly on EVM if they can. Lens shows live health | Treat the bridge ETA as a liquidation buffer |

Drills (run before each release): restart the relayer mid-submission, reorg
on a fork, a stuck attestation (kill the Iris poll and resume), and a direct
owner exit with the relayer stopped.
