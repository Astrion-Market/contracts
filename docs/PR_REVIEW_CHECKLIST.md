# PR Review Checklist

- Scope is focused and does not include unrelated refactors.
- Public API changes are intentional and documented.
- New behavior has unit or integration tests.
- `cargo fmt --all -- --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes or warnings are justified.
- `cargo test --workspace` passes.
- WASM build succeeds for changed contracts.
- Auth, pause, initialization, and parameter guards are covered.
- Storage keys and TTL behavior are reviewed.
- Events are present for state-changing protocol actions.
- README or CONTRIBUTING updates are included when workflows change.
