# Direct owner actions (no relayer)

Every action a relayer can submit can also be executed by the account owner
directly. This needs no signature and no Astrion service, and application
pauses do not apply. Use it for emergency repayment, for exits, or when the
relayer is down.

```bash
export OWNER_KEY=0x...            # the account owner's key
export ACCOUNT=0x...              # scoped execution account
export MODULE=0x...               # module from the deployment manifest
export ACTION=$(cast abi-encode "f(uint8,uint256)" 6 0)   # see table
forge script evm/script/AccountOps.s.sol --root evm --rpc-url $BASE_RPC_URL --broadcast
```

The script asks the module for its plan, checks that the module protocol
matches the account, checks that `OWNER_KEY` is the owner, and then calls
`executeBatch`.

## Action encodings

`max` = `115792089237316195423570985008687907853269984665640564039457584007913129639935`.

| Module | Action | `cast abi-encode` |
|---|---|---|
| Aave V3 | supply USDC | `"f(uint8,address,uint256)" 0 <usdc> <amount>` |
| Aave V3 | supply collateral | `"f(uint8,address,uint256)" 1 <collateral> <amount>` |
| Aave V3 | withdraw (max allowed) | `"f(uint8,address,uint256)" 2 <asset> <amount\|max>` |
| Aave V3 | borrow | `"f(uint8,address,uint256)" 3 <usdc> <amount>` |
| Aave V3 | repay / repay all / repay available | `"f(uint8,address,uint256)" 4\|5\|6 <usdc> <amount\|0>` |
| Morpho Blue | supply / collateral / withdraw / withdraw collateral / borrow | `"f(uint8,uint256)" 0\|1\|2\|3\|4 <amount\|max>` |
| Morpho Blue | repay / repay all / repay available | `"f(uint8,uint256)" 5\|6\|7 <amount\|0>` |
| Compound III | supply base / withdraw base | `"f(uint8,uint256)" 0\|1 <amount>` |
| Compound III | supply / withdraw collateral | `"f(uint8,uint256)" 2\|3 <amount\|max>` |
| Compound III | repay all / repay available | `"f(uint8,uint256)" 4\|5 0` |
| CCTP return | burn to Stellar | `"f(uint256,uint256,uint32,string)" <amount> <maxFee> 2000 <G…/C…/M…>` |

Without Foundry, the same calls can be made from any wallet: call the
module's `plan(account, marketScope, recipient, action)` (a view call), then
send `executeBatch` with the returned calls from the owner address.
