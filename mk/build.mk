.PHONY: build build-contract test optimize fmt clippy clean

build:
	stellar contract build

build-optimized optimize:
	stellar contract build --optimize

build-contract:
	@test -n "$(CONTRACT)" || (echo "Usage: make build-contract CONTRACT=oracle-adapter" && exit 1)
	cargo build -p $(CONTRACT) --target wasm32v1-none --release

test:
	cargo test

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

clean:
	cargo clean
