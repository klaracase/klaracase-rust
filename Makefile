.PHONY: test lint fmt doc smoke msrv

test:
	cargo test

lint:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

fmt:
	cargo fmt

doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

# Checks the minimum supported Rust version. Needs `rustup toolchain install 1.85`.
msrv:
	cargo +1.85 test --locked

# Calls the two free endpoints against the real API. Needs KLARACASE_API_TOKEN. Not part of CI.
smoke:
	@test -n "$$KLARACASE_API_TOKEN" || (echo "Set KLARACASE_API_TOKEN first." >&2; exit 1)
	cargo run --example smoke
