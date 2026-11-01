.PHONY: test test-native-tls lint fmt doc smoke msrv sync-check

test:
	cargo test --all-features

# The same tests on the platform TLS library.
test-native-tls:
	cargo test --no-default-features --features native-tls,tokio

lint:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

fmt:
	cargo fmt

doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

# Checks the minimum supported Rust version. Needs `rustup toolchain install 1.85`.
msrv:
	cargo +1.85 check --locked

# Fails when the fixtures in this repo differ from the shared contract folder next to it. Run it
# in the klaracase-sdks workspace. After a contract change, copy the files and run it again.
sync-check:
	@test -d ../_contract || { echo "../_contract not found: run this in the klaracase-sdks workspace." >&2; exit 1; }
	diff -r -x validate.py tests/fixtures/conformance ../_contract/conformance
	diff -r tests/fixtures/examples ../_contract/examples
	diff tests/fixtures/contract/CONTRACT.md ../_contract/CONTRACT.md
	@echo "fixtures match ../_contract"

# Calls the two free endpoints against the real API. Needs KLARACASE_API_TOKEN. Not part of CI.
smoke:
	@test -n "$$KLARACASE_API_TOKEN" || (echo "Set KLARACASE_API_TOKEN first." >&2; exit 1)
	cargo run --example smoke
