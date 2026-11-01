# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Paging: `Pager` with `next_page()`, from `decisions().search(..).pages(max_pages)` and
  `decisions().citations(..).pages(max_pages)`. `max_pages` is required and clamped (10 for a search,
  20 for a citation graph) as a guard against a runaway loop. Paging the same query within a credit
  period costs no extra credits; a changed query, filter or mode is a new charge.

- Async client `Klaracase` for the Klaracase REST API v1 (API release 0.95.0): decisions, cross-type
  search, norms, data packages, citation checks, citation check documents, corpus coverage and facets.
- Typed request parameters. `None` is never sent; `Some(false)` and `Some(0)` are sent.
- Typed responses that keep every unknown key in `extra`, wrapped in `Response<T>` with `meta`.
- One `Error` enum with a variant per HTTP status class, plus transport, timeout, configuration and
  document errors.
- Retries on 429 and 503 for every operation, and on transport errors for GET only.
- `Sleeper` and `Clock` traits for tests, and a deprecation hook.
- Features `rustls` (default), `native-tls`, `tokio` (default) and `log`.
