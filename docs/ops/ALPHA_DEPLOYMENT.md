# Alpha deployment runbook

Staged deployments of the EVM execution layer. Everything is driven by
`deployments/crosschain/<env>.json`. Outputs go to
`deployments/crosschain/deployed/<network>.json`.

## 0. Preconditions

- [ ] Clean checkout at the release commit; `make evm-deps evm-build evm-test sdk-test crosschain-test relayer-test` pass.
- [ ] `make crosschain-verify-manifest ENV=<env> WRITE=1` recorded code hashes. Review the diff.
- [ ] `make route-gates` produced `docs/evidence/ROUTE_GATES.md` for this commit.
- [ ] Route guardian is a multisig. The deployer key holds gas only.
- [ ] `STELLAR_FORWARDER_BYTES32` derived from the manifest: `stellar strkey decode <CctpForwarder>`.

## 1. Testnet (staged)

```bash
export ROUTE_GUARDIAN=0x...                # testnet multisig or test key
export STELLAR_FORWARDER_BYTES32=0x$(stellar strkey decode CA66Q2WFBND6V4UEB7RD4SAXSVIWMD6RA4X3U32ELVFGXV5PJK4T4VSZ | jq -r .contract)
make evm-deploy ASTRION_ENV=testnet NETWORK_ID=base-sepolia RPC=$BASE_SEPOLIA_RPC_URL
make evm-smoke NETWORK_ID=base-sepolia RPC=$BASE_SEPOLIA_RPC_URL
```

On testnets the manifest lists no protocol targets, so this deploys the
transport alpha: policy, factory, return modules. Lending modules are proven on
mainnet forks instead (see `docs/evidence/COMPONENTS.md`).

## 2. Run the transport evidence

Follow `docs/evidence/CCTP_TESTNET.md` with the deployed account as the
`EVM_RECIPIENT`. Publish both directions.

## 3. Mainnet (gated)

`Deploy.s.sol` refuses chain IDs 1 and 8453 unless
`deployments/crosschain/release-approval.json` exists. That file must name the
exact `GIT_COMMIT`, the approved chain IDs, and a security review link (see
`release-approval.example.json`). Creating that file **is** the release
decision. It is never automated.

After deploying: smoke test, record deployed addresses in the manifest,
enable routes one at a time (`enabled: true`, `status: verified`) with the
route limits named in the approval, and announce only those routes.

## 4. Rollback

Contracts are immutable. To roll back:
1. The guardian pauses affected routes (`RoutePolicy.setPaused`). Owners keep
   direct exits.
2. Set `enabled: false` in the manifest and redeploy the interface config.
3. Stop the relayer if needed. Users continue with
   `docs/ops/DIRECT_OWNER_ACTIONS.md`.
