# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Async client `Klaracase` for the Klaracase REST API v1 (API release 0.95.0): decisions, cross-type
  search, norms, data packages, citation checks, citation check documents, corpus coverage and facets.
- Typed request parameters. `None` is never sent; `Some(false)` and `Some(0)` are sent.
- Typed responses that keep every unknown key in `extra`, wrapped in `Response<T>` with `meta`.
- One `Error` enum with a variant per HTTP status class, plus transport, timeout, configuration and
  document errors.
- Retries on 429 and 503 for every operation, and on transport errors for GET only.
- `Sleeper` and `Clock` traits for tests, and a deprecation hook.
- Features `rustls` (default), `native-tls`, `tokio` (default) and `log`.
