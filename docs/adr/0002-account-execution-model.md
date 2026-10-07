# ADR-0002: Execution account model and bounded intents

- Status: accepted (alpha)
- Date: 2026-10-07
- Code: `evm/src/account/`, `evm/src/interfaces/IActionModule.sol`

## Decision

**Accounts.** `AstrionAccountFactory` deploys one `AstrionAccount` per
`(owner, chainId, protocol, marketScope, version)` with CREATE2. The owner is a
constructor argument and part of the salt, so deployment and ownership are
atomic and front-running can only deploy the rightful owner's account. Owner
is immutable for alpha (no transfer, no initializer). The factory has no admin
and holds no assets.

**Execution context.** Every protocol call is a plain `CALL` from the account,
so aTokens, Morpho positions, Comet balances and debt belong to the account.
**There is no delegatecall anywhere.** Modules cannot touch account storage.

**Modules plan; the account executes.** A module is a stateless contract whose
configuration is in immutables. `plan(account, marketScope, recipient, action)`
returns calls, the tokens it approves, and whether the action increases risk.

**Intents.** A relayer submits an EIP-712 `ExecutionIntent` signed by the owner
(ECDSA or ERC-1271). The domain is `("AstrionAccount", "1", chainId, account)`,
so a signature is valid for one chain and one account only. The owner signs:

| Field | Prevents |
|---|---|
| `module`, `moduleCodeHash` | module substitution or upgrade; EXTCODEHASH pins logic + immutables |
| `target` | calls to any protocol contract other than the one approved |
| `actionHash` | changing amounts, market or action |
| `recipient` | redirecting token transfers out of the account |
| `relayer` (optional) | submission by others |
| `feeToken`, `maxFee` | unbounded fees; the relayer passes `fee ≤ maxFee` |
| `nonce` (unordered) | replay; the owner can `revokeNonce` |
| `deadline` | late execution |
| `transferId` | executing against an unrelated bridge transfer (C10) |

**Enforced on every planned call:**
- the target is the signed `target` with a module-allowed selector, OR one of
  the plan's approval tokens with `approve(target, …)` or
  `transfer(recipient, …)` only;
- no ETH value;
- never the account or the factory itself;
- after execution, every approval token's allowance to `target` is **zero**
  (exact approvals).

**Pauses.** `RoutePolicy` (guardian, `Ownable2Step`) pauses a route
`keccak256(chainId, protocol, marketScope)`. A pause blocks only plans marked
`increasesRisk` (supply, borrow, collateral withdrawal). Repayment through
intents and **all owner direct calls** are never blocked. The guardian cannot
move funds.

## Rejected alternatives

- **Delegatecall modules**: would need frozen storage layouts and full trust in
  module code. Rejected for alpha.
- **Registry-selected modules**: an admin could swap the code that decides
  calls. Instead, the owner signs the module code hash.
- **Shared router with credit delegation**: a router would own or control
  positions across users. Rejected.

## Consequences

- A module bug can at worst produce calls within the signed target, approved
  tokens and the signed recipient. It cannot reach other users or other scopes.
- Module upgrades are new deployments with new code hashes. Old intents stop
  validating.
- Tests: `evm/test/account/ExecutionIntent.t.sol` covers wrong owner, chain,
  account and market, replay, revocation, expiry, module/target/recipient
  substitution, excess allowance, self-calls, reentrancy, fee caps and pauses.
