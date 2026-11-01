# Contributing

Thank you for your interest. This crate is a small, hand-written client. We keep it that way.

## Set up

You need Rust 1.85 or newer.

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps
```

`make test` and `make lint` run the same checks.

## Rules

- No test may use the network. Tests run against a local mock server.
- Request parameters are typed strictly. Response types keep unknown keys in `extra`.
- Never rename, reorder or clean data from a response.
- Never put a real API token in code, tests, docs or fixtures. Use `klc_your_token_here`.
- The fixtures in `tests/fixtures/` are copied from the shared Klaracase conformance pack. Do not edit
  them by hand.
- Keep commits small. Use a semantic commit message such as `feat(corpus): add facets`.

## Reporting a problem

Open an issue for a bug. For a security problem, read `SECURITY.md` first.
