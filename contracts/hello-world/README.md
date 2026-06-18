# Hello World Tutorial

`hello-world` is the smallest Soroban contract in this workspace. Use it to verify
your Rust, WASM target, Stellar CLI, and test environment before touching the
protocol contracts.

## Build

```bash
cargo build -p hello-world --target wasm32v1-none --release
```

## Test

```bash
cargo test -p hello-world
```

The test suite includes a basic contract invocation and a token transfer demo
using `soroban-sdk` test utilities.

## Deploy To Testnet

```bash
stellar contract upload \
  --wasm target/wasm32v1-none/release/hello_world.wasm \
  --network testnet \
  --source <SOURCE_ACCOUNT>

stellar contract deploy \
  --wasm-hash <WASM_HASH> \
  --network testnet \
  --source <SOURCE_ACCOUNT>

stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source <SOURCE_ACCOUNT> \
  -- hello \
  --to Dev
```
