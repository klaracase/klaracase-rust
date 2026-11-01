# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

The first release is not tagged yet, so these entries describe 0.1.0 as a whole. The list ends with
the changes made to the first draft.

### Added

- Async client `Klaracase` for the Klaracase REST API v1 (API release 0.95.0): decisions, cross-type
  search, norms, data packages, citation checks, citation check documents, corpus coverage and facets.
- Typed responses that keep every unknown key in `extra`, wrapped in `Response<T>` with `meta` and the
  lossless `raw()` JSON. A `null` or a missing key never fails a call that was paid for: lists read
  as empty and optional fields as `None`.
- `Open<E>` for every vocabulary the server may extend (verdicts, run and document statuses, error
  codes, reference types, jurisdictions and more). A known value is a variant, an unknown one stays a
  string and round-trips byte for byte.
- Request parameters as `#[non_exhaustive]` structs with `new()`, consuming setters, `Serialize` and
  `Deserialize` (unknown keys are refused). `None` is never sent; `Some(false)` and `Some(0)` are sent.
- Structural client validation before any request: a blank query, a date that is not `YYYY-MM-DD`, an
  inverted date or Randnummer range, no ids or more than ten, both or neither of `text` and
  `document_id`, a blank file name. Numbers the server owns are never checked.
  `ClientBuilder::client_validation(false)` turns it off.
- Short calls: `decisions().get(id)`, `norms().get(reference)`, `decisions().search(query)` and
  `decisions().citations(id)` as request builders with `send()`, `search(query)` on the client, and the
  long forms `search_with`, `get_with`, `citations_with`.
- Paging: `Pager` with `next_page()`, from `decisions().search(..).pages(max_pages)` and
  `decisions().citations(..).pages(max_pages)`. `max_pages` is required and clamped (10 for a search,
  20 for a citation graph) as a guard against a runaway loop. The walk stops at the first empty page.
- `UploadParams::from_path`, `wait_until_ready(id, timeout)` with `wait_until_ready_with(id,
  WaitParams)`, and `citation_checks().check_file(path)` that uploads, waits and checks.
- Error helpers: `is_retryable()`, `retry_after()`, `status()`, `code()` and `headers()` (never
  `Authorization`). `Error` implements `Serialize` as `{code, status, detail, retry_after_seconds}`.
  `Problem` has typed `candidates`, `successor`, `unit`, `cap` and `resets_at`.
- `Error::ClientValidation`, split from `Error::Configuration`.
- Retries on 429 and 503 for every operation, and on transport errors for GET only.
- A deprecation hook.
- Features `rustls` (default), `native-tls`, `tokio` (default), `log`, `tracing` and `schemars`.
  `tracing` emits a `debug!` event per retry (method, path, status, attempt, wait) and a `warn!` on a
  deprecation. The token is never logged. `schemars` derives `JsonSchema` for the request parameters
  and enums, with a hand implementation for `Open<E>`.
- `examples/agent_tools.rs`: tool definitions from the schemas, dispatch by name, short answers and
  errors as JSON.
- Tests: the 102 shared conformance cases, a round trip of every example answer, zero-request checks
  for validation, and a null-injection test over all response types.
- CI: a test matrix on Linux, macOS and Windows, the minimum Rust version, clippy and docs with all
  features, `native-tls`, `cargo semver-checks`, `cargo deny`, `cargo audit`, `cargo package`, a
  nightly docs.rs build and a nightly direct-minimal-versions run. `make sync-check` compares the
  fixtures with the shared contract folder.

### Changed

- `Klaracase::new("")` is a configuration error and does not read the environment. Only
  `Klaracase::from_env()` and `ClientBuilder::token_from_env()` do.
- `ResponseMeta::request_duration` covers the final HTTP attempt only, as the design says. The first
  draft documented and measured the whole call including retries.
- Paging the same query within a credit period costs no extra credits. A changed query, filter or
  mode is a new charge. The pager docs and the README say so; the first draft said that every page
  costs credits.
- `Sleeper`, `Clock` and their builder methods are hidden from the rendered docs. They remain
  available as test seams.
- Lower bounds of the dependencies in `Cargo.toml` are raised to versions that build and pass the tests
  (checked with `cargo update -Z direct-minimal-versions`).
- `missing_docs` and broken intra-doc links are denied. docs.rs builds with all features.
