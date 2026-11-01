# klaracase

The official Rust client for the [Klaracase](https://klaracase.de) REST API. Klaracase holds German court decisions and German statutes, linked by a resolved citation graph.

Klaracase covers German law only. It is a research tool. It gives no legal advice and does not replace a lawyer's review. A `not_found` verdict means the citation was not found in the corpus; it is not a claim that the citation is fabricated.

This crate is a thin, hand-written client for API v1. It is async, built on `reqwest`, and covers all twelve operations. It does not implement MCP. If you build an agent, connect your MCP client to `https://klaracase.de/mcp` instead. The same token works for both.

## Install

```sh
cargo add klaracase
cargo add tokio --features macros,rt-multi-thread
```

Features:

| Feature | Default | Effect |
|---|---|---|
| `rustls` | yes | TLS through rustls. |
| `native-tls` | no | TLS through the platform library (OpenSSL, Secure Transport, SChannel). |
| `tokio` | yes | The default sleeper uses `tokio::time`. Without it you must pass your own sleeper to the builder. |
| `log` | no | The default deprecation hook uses `log::warn!`. |
| `tracing` | no | A `debug!` event per retry and a `warn!` on a deprecation. See Tracing. |
| `schemars` | no | `JsonSchema` for the request parameters and request enums. See Agent tools. |

The default TLS provider is rustls with aws-lc-rs. aws-lc-rs compiles C code, so the build needs a C compiler, and on some platforms CMake or NASM. If that is a problem for you, use the platform TLS library:

```toml
klaracase = { version = "0.1", default-features = false, features = ["native-tls", "tokio"] }
```

## Get a token

Create an API token at <https://klaracase.de/settings/api-tokens>. You need an account with a verified email address.

- A token has up to five abilities: `search`, `read`, `graph`, `datapackage` and `citation-check`. All five are ticked by default.
- A token is bound to the organization that is open when you create it. Every call is billed to that organization.
- A token expires after 365 days by default. The server accepts 1 to 365 days.

Keep the token secret. Do not commit it. The client reads it from the argument or from the `KLARACASE_API_TOKEN` environment variable. This README uses `klc_your_token_here` as a placeholder.

## Quick start

```rust,no_run
use klaracase::Klaracase;

#[tokio::main]
async fn main() -> Result<(), klaracase::Error> {
    // Reads KLARACASE_API_TOKEN. Use Klaracase::new("klc_your_token_here") to pass it in code.
    let client = Klaracase::from_env()?;

    let found = client
        .decisions()
        .search("Vermögensbetreuungspflicht Untreue")
        .court("BGH")
        .send()
        .await?;

    for hit in &found.results {
        println!("{:?} {:?} {:?}", hit.court, hit.aktenzeichen, hit.date);
    }
    println!("API release {:?}", found.meta.api_release);
    Ok(())
}
```

Search, read and graph calls come in two forms. The short form takes what you must give and returns a request with setters: `client.decisions().search("Untreue").court("BGH").send().await`. `client.decisions().get(id)` and `client.norms().get(reference)` take no parameters. The long form takes a ready parameter struct: `search_with(params)`, `get_with(id, params)`. Use it when you build the parameters elsewhere, or read them from JSON.

A response dereferences to its typed body, so `found.results` works. `found.meta` holds the HTTP status, the API release and `request_duration`, the time of the final HTTP attempt (earlier attempts and the waits between them are not counted). `found.credits` says what the call cost. Keys the crate does not model stay in the `extra` map of the struct that held them. `found.raw()` returns the whole answer as `serde_json::Value`, as the server sent it.

## Builders and validation

Every parameter struct is built with `new(...)` or `default()` and chained setters. A parameter you do not set is not sent. `limit(0)` and `bridge(false)` are values and are sent. Every parameter struct is `#[non_exhaustive]`, so a new parameter in a later release does not break your code. Each one implements `Serialize` and `Deserialize` and refuses unknown keys, so a misspelled key in JSON is an error and not a silent drop.

```rust
use klaracase::{Branch, Mode, SearchDecisionsParams, Statute};

let params = SearchDecisionsParams::new("Vermögensbetreuungspflicht Untreue")
    .court("BGH")
    .branch(Branch::Ordentlich)
    .cited_statute(Statute::StGB)
    .date_from("2015-01-01")
    .mode(Mode::Hybrid)
    .limit(5);
assert!(params.validate().is_ok());
```

A setter for a closed list takes the typed value (`Branch::Ordentlich`) or the server's own spelling (`"ordentlich"`). The string form is the way to send a value that this crate does not know yet.

Before it sends a request, the client checks the shape of your input and refuses a bad one with `Error::ClientValidation`. It sends nothing and charges nothing. It checks:

- a query that is blank,
- a date that is not `YYYY-MM-DD` (`date_from`, `date_to`, `date`, `reference_date`),
- `date_from` later than `date_to`, and `rn_from` greater than `rn_to`,
- no ids, or more than ten ids, for a data package,
- both or neither of `text` and `document_id`,
- a blank file name, and a path parameter that cannot be encoded safely.

It never checks a number the server owns, such as the maximum `limit`, the length of a query or a page. The server answers those with a 422, which is `Error::Validation`. To let the server judge every call, build the client with `.client_validation(false)`.

## Pagination

A search and a citation graph have pages. `pages(max_pages)` returns a `Pager`. Call `next_page()` in a loop:

```rust,no_run
use klaracase::Klaracase;

#[tokio::main]
async fn main() -> Result<(), klaracase::Error> {
    let client = Klaracase::from_env()?;

    let mut pager = client.decisions().search("Untreue").court("BGH").pages(3);
    while let Some(page) = pager.next_page().await? {
        for hit in &page.results {
            println!("{:?} {:?}", hit.court, hit.aktenzeichen);
        }
    }
    Ok(())
}
```

`max_pages` is required. It is a guard against a runaway loop, and it is clamped to what the server serves: 10 pages for a search and 20 for a citation graph. At depth 2 the server serves page 1 only. The walk stops at the first empty page, which it does not return.

Paging the same query within a credit period costs no extra credits. A changed query, filter or mode is a new charge. Check `credits` on each response. Every page is still one request against the rate limit of 60 requests per minute per token.

After an error, the pager keeps its place. Calling `next_page()` again asks for the same page. This crate has no stream dependency, so `Pager` is a plain async method and not a `Stream`.

## Check the citations in a text

```rust,no_run
use klaracase::{CreateCitationCheckParams, Klaracase, Open, Verdict};

#[tokio::main]
async fn main() -> Result<(), klaracase::Error> {
    let client = Klaracase::from_env()?;
    let text = "Der Bundesgerichtshof hat im Urteil vom 15.11.2001 zum Az. 1 StR 185/01 \
                entschieden, dass § 266 StGB eine Vermögensbetreuungspflicht voraussetzt.";

    let run = client
        .citation_checks()
        .create(CreateCitationCheckParams::from_text(text).reference_date("2024-05-01"))
        .await?;

    for finding in &run.findings {
        let text = finding.raw_text.as_deref().unwrap_or("-");
        match finding.verdict.as_ref().and_then(Open::known) {
            Some(Verdict::VerifiedCorpus) => println!("exists in the corpus: {text}"),
            Some(Verdict::NotFound) => println!("not found in the corpus: {text}"),
            Some(Verdict::OutOfCoverage) => println!("outside the corpus: {text}"),
            Some(Verdict::Ambiguous) => println!("ambiguous: {text}"),
            other => println!("{other:?}: {text}"),
        }
    }
    println!("{} unique citations", run.unique_citations.len());
    Ok(())
}
```

The call is synchronous. The answer is the finished run. The closed set of verdicts is `verified_corpus`, `verified_external`, `not_found`, `out_of_coverage`, `ambiguous`, `unverifiable` and `discrepancy`. They are the variants of `Verdict`. A verdict this crate does not know yet stays readable as a string: the field is an `Open<Verdict>`, and `.known()` returns the variant if there is one.

Read the verdicts with care:

- `verified_corpus` shows that the cited decision or statute exists in the corpus. It does not show that a quote or a legal proposition is correct.
- `not_found` shows absence of evidence in the corpus. It is not a claim that the citation is fabricated. The API check uses the corpus only, so `not_found` is weaker evidence here than in the Klaracase web app.

Offsets in a finding are Unicode codepoint offsets into the exact string you sent. They are not byte offsets.

You give either `text` or `document_id`, never both. If you give both or neither, the call fails on the client with `Error::ClientValidation` (code `client_validation`) before any request.

## Uploads and polling

To check a file, call `check_file`. It reads the file, uploads it, waits until the text is extracted, and then runs the check:

```rust,no_run
use std::time::Duration;
use klaracase::Klaracase;

#[tokio::main]
async fn main() -> Result<(), klaracase::Error> {
    let client = Klaracase::from_env()?;

    let run = client
        .citation_checks()
        .check_file("schriftsatz.pdf")
        .timeout(Duration::from_secs(90))
        .reference_date("2024-05-01")
        .send()
        .await?;
    println!("{} findings", run.findings.len());
    Ok(())
}
```

An upload accepts PDF, DOCX, Markdown and plain text up to 25 MB. It is free. The wait is free too. Only the check costs credits, and it runs only when the upload is `ready`. If extraction fails or time runs out, no check runs and nothing is charged.

The steps are also available one by one:

```rust,no_run
use std::time::Duration;
use klaracase::{CreateCitationCheckParams, Klaracase, UploadParams};

#[tokio::main]
async fn main() -> Result<(), klaracase::Error> {
    let client = Klaracase::from_env()?;

    let receipt = client
        .citation_check_documents()
        .upload(UploadParams::from_path("schriftsatz.pdf")?)
        .await?;
    let ready = client
        .citation_check_documents()
        .wait_until_ready(&receipt.id, Duration::from_secs(120))
        .await?;
    println!("a check costs {:?} credits", ready.quoted_credits);

    let run = client
        .citation_checks()
        .create(CreateCitationCheckParams::from_document(&ready.id))
        .await?;
    println!("{} findings", run.findings.len());
    Ok(())
}
```

`UploadParams::from_path` reads the file with `std::fs::read`, which blocks the thread for the time the disk takes. It takes the file name from the path and guesses the media type from the extension: `pdf`, `docx`, `md` or `txt`. For any other extension, use `UploadParams::new(bytes, filename, content_type)`.

`wait_until_ready(id, timeout)` polls every 2 seconds. Use `wait_until_ready_with(id, WaitParams)` to change the interval. It returns `Error::DocumentFailed` when extraction fails and `Error::DocumentTimeout` when time runs out. The status `ocr_pending` is not final, so the loop keeps going.

## Handle errors

One `Error` enum covers every failure. It is `#[non_exhaustive]`, so end a `match` with a catch-all arm.

```rust,no_run
use klaracase::{Error, Klaracase};

#[tokio::main]
async fn main() {
    let client = Klaracase::from_env().expect("token");

    match client.decisions().get("1 StR 185/01").await {
        Ok(decision) => println!("{:?} chars", decision.text_length),
        Err(Error::NotFound(problem)) => println!("no such decision: {}", problem.detail),
        Err(Error::InsufficientCredits { cost, balance, .. }) => {
            println!("the call costs {cost:?} credits, the balance is {balance:?}");
        }
        Err(Error::TokenLimitReached { cap, resets_at, .. }) => {
            println!("this API key reached its limit of {cap:?} credits, it resets at {resets_at:?}");
        }
        Err(Error::RateLimit { retry_after_seconds, .. }) => {
            println!("rate limited, retry in {retry_after_seconds:?} seconds");
        }
        Err(Error::Permission(problem)) => {
            println!("token lacks {:?}", problem.required_ability());
        }
        Err(error) if error.is_transport() => println!("network problem: {error}"),
        Err(other) => println!("{} ({:?}): {other}", other.status().unwrap_or(0), other.code()),
    }
}
```

| Variant | When |
|---|---|
| `Authentication` | 401 |
| `Permission` | 403. `Problem::required_ability()` names the missing ability. |
| `NotFound` | 404 |
| `Conflict` | 409 |
| `Validation` | 422. `Problem::errors()` holds the field errors. |
| `InsufficientCredits { cost, balance, reason, top_up_url, problem }` | 402 `insufficient_credits`. See Credits, API budget and limits. |
| `TokenLimitReached { unit, cap, spent, resets_at, settings_url, problem }` | 402 `token_limit_reached`. It has no `cost` and no `balance`. |
| `RateLimit { retry_after_seconds, problem }` | 429, including `volume_cap` |
| `ServiceUnavailable` | 503 |
| `Api` | any other status, and any error body that is not a problem |
| `Transport`, `Timeout` | no answer came |
| `Decode` | a 2xx answer that is not the expected JSON |
| `Configuration { code, message }` | a mistake in the client setup, found before any request |
| `ClientValidation { code, message }` | a mistake in the arguments of a call, found before any request |
| `DocumentFailed`, `DocumentTimeout` | `wait_until_ready` |

The API answers errors as RFC 9457 `application/problem+json`. `Error::problem()` returns the `Problem` with `type_`, `title`, `status`, `detail`, `code`, `api_release` and `extra`. `extra` holds every other member of the body. `Error::code()` returns the machine code. The HTTP status decides the variant.

A body that is not a problem (an HTML 502 from a proxy, for example) becomes `Error::Api` with `code` set to `unknown` and the first 1,000 characters of the body in `detail`.

Four helpers cover most handling, so you need not match on every variant:

- `error.is_retryable()` is true for `Transport`, `Timeout`, `RateLimit` and `ServiceUnavailable`. The client has already retried as often as `max_retries` allows when it returns one of them.
- `error.retry_after()` returns the wait the server asked for, as a `Duration`.
- `error.status()` and `error.code()` return the HTTP status and the machine code.
- `error.headers()` returns the response headers. `Authorization` is never in them.

`Problem` also has typed members for the answers that carry data: `candidates` (422 `ambiguous_aktenzeichen`, the decisions to choose from), `successor` (404 `statute_superseded`), and `unit`, `cap` and `resets_at` (429 `volume_cap` and 402 `token_limit_reached`). `Problem::extra` still holds every member. The 402 members `reason`, `top_up_url`, `spent` and `settings_url` are typed on the two 402 variants.

`Error` implements `Serialize`. It writes `{"code", "status", "detail", "retry_after_seconds"}`, which is the form to hand to an agent or a log.

## Retries

- The client retries a 429 and a 503, up to `max_retries` times (default 2). Both statuses guarantee that nothing ran and nothing was charged, so a retry is safe for every operation, including POST.
- It never retries 402, any other 4xx, 500 or 502.
- The wait is `retry_after_seconds` from the body, else the `Retry-After` header, else exponential backoff with full jitter (up to 0.5, 1, 2 seconds, capped at 8).
- A server-stated wait above 30 seconds is not slept. The client returns the error at once and you decide. A `volume_cap` answer is of this kind.
- A transport error (connection, DNS, TLS, timeout) is retried for GET only. A POST whose connection dropped after the request was sent is not retried. Repeating it by hand is free in the same period, because one charge covers each operation, organization and subject per credit period.

## Configure the client

```rust,no_run
use std::time::Duration;
use klaracase::Klaracase;

fn main() -> Result<(), klaracase::Error> {
    let client = Klaracase::builder()
        .token("klc_your_token_here")
        .timeout(Duration::from_secs(60))
        .max_retries(4)
        .user_agent_suffix("my-app/1.2")
        .build()?;
    println!("{}", client.user_agent());
    Ok(())
}
```

The builder also takes `base_url`, your own `reqwest::Client` (`http_client`, for a proxy or a custom root certificate) and an `on_deprecation` hook. When the API deprecates an endpoint, it announces this with `Deprecation` and `Sunset` headers. The client calls the hook once per process with the raw header values. The default hook writes one line to stderr, or one `log::warn!` if you enable the `log` feature.

To test your own code without the network, hand the builder a `reqwest::Client` that points at a mock server (see Testing).

## Credits, API budget and limits

Calls are billed in credits, to the organization the token is bound to. Search, norm and decision reads cost 1 credit. The citation graph costs 2. A data package costs 5. A citation check costs `max(2, ceil(chars / 5000))`. Coverage, facets, uploads and reading a finished run are free. Prices are on <https://klaracase.de/preise>.

One charge covers each operation, organization and subject per credit period. The same call again in the same credit period costs nothing more, on REST or on MCP. Every response carries `credits` with `metered`, `cost` and `credits_remaining`.

Credits renew per credit period: the UTC calendar month for free organizations, the paid billing period for subscribers. Unused credits do not roll over.

The API allows 60 requests per minute per token. The three POST operations (`datapackages`, `citation_checks` and `citation_check_documents` upload) are limited to 10 per minute.

### Plan credits and the API budget

The plan gives an organization a number of plan credits for each credit period. `credits_remaining` is the number of plan credits left after the call. It never counts the API budget.

Organizations on the Professional and Enterprise plans can also load a prepaid API budget. It is optional. The budget pays for API and MCP calls after the plan credits are used up. It does not expire. The web app never draws on it. A call is paid from the budget in whole, never in part. `cost` counts plan credits and budget credits together.

When the organization holds a budget, or may load one, `credits.api_budget` is set:

```rust,no_run
use klaracase::Klaracase;

#[tokio::main]
async fn main() {
    let client = Klaracase::from_env().expect("token");
    let found = client
        .decisions()
        .search("Untreue")
        .send()
        .await
        .expect("search");

    if let Some(budget) = found.credits.as_ref().and_then(|credits| credits.api_budget.as_ref()) {
        println!(
            "budget: {:?} cents, {:?} cents per credit, about {:?} more credits",
            budget.cents, budget.rate_cents_per_credit, budget.credits_at_rate
        );
    }
}
```

`api_budget` is `None` when the organization has no budget and may not load one. The key is then absent from the answer. `cents` can be negative after a lost payment. `rate_cents_per_credit` is the rate for your organization. `credits_at_rate` is `0` when the budget is empty or on hold.

### Refusals

Three answers say that a call did not run. Nothing was charged for any of them. The client never retries a 402, and it does not open any link in an answer.

| Error | What it means | What a program should do |
|---|---|---|
| `InsufficientCredits` with `reason` `ApiBudgetEmpty` | The plan credits are used up. The organization may load an API budget, and what it holds does not cover the call. | Stop. Report to your user that an organization admin can top up the API budget. |
| `InsufficientCredits` with `reason` `ApiBudgetNotAvailable` | The plan credits are used up. The organization has no API budget to draw on, or the budget is on hold. | Stop. Report to your user that the plan credits are used up. |
| `InsufficientCredits` without `reason` | The plan credits are used up and the answer says no more. | Stop. Report to your user. |
| `TokenLimitReached` | The credit limit set for this one API key is used up for the period. | Stop. Report to your user that the key is at its limit until `resets_at`. |

`reason` is an `Open<InsufficientCreditsReason>`. It also has a reserved value, `PlanCreditsExhausted`, that the API does not send today. A reason this crate does not know stays a string in `Open::Other`. Treat it like `ApiBudgetNotAvailable`.

`top_up_url` (on `InsufficientCredits`) and `settings_url` (on `TokenLimitReached`) are for a human organization admin. Show them to your user. A program or an AI agent must never open them or follow them. The `detail` sentence of the answer can also name the page.

Each API key can have its own credit limit for the period, set by an organization admin. `TokenLimitReached` carries `unit` (always `credits`), `cap` (the limit), `spent` (the credits the key used in the period, plan credits and budget together) and `resets_at` (an RFC 3339 time, as the server sent it). It has no `cost` and no `balance`, and any of its members is `None` when the answer leaves it out.

```rust,no_run
use klaracase::{Error, InsufficientCreditsReason, Klaracase, Open};

#[tokio::main]
async fn main() {
    let client = Klaracase::from_env().expect("token");

    match client.decisions().search("Untreue").send().await {
        Ok(found) => println!("{} results", found.results.len()),
        Err(Error::InsufficientCredits { reason, top_up_url, .. }) => {
            match reason {
                Some(Open::Known(InsufficientCreditsReason::ApiBudgetEmpty)) => {
                    println!("The API budget is empty. Ask an organization admin to top it up.");
                    println!("Page for the admin: {top_up_url:?}");
                }
                _ => println!("The plan credits are used up."),
            }
            // Stop here. Do not retry. Do not open the link.
        }
        Err(Error::TokenLimitReached { cap, spent, resets_at, .. }) => {
            println!("This API key used {spent:?} of {cap:?} credits. It resets at {resets_at:?}.");
        }
        Err(other) => println!("{other}"),
    }
}
```

## Tracing

Enable the `tracing` feature to see what the client does. It emits a `debug!` event for each retry with the method, the path (never the query), the status, the number of the retry and the wait in milliseconds. It emits one `warn!` when the API announces a deprecation. The token and the headers are never in an event.

```toml
klaracase = { version = "0.1", features = ["tracing"] }
```

Without the feature the client emits nothing. Set a subscriber in your program as usual, for example with `tracing-subscriber` and `RUST_LOG=klaracase=debug`.

## Agent tools

The `schemars` feature derives `JsonSchema` for every request parameter struct and every request enum. You can turn the schemas into tool definitions for a model, and run the model's calls with the same types. A schema lists the values this crate knows for a closed list, so a model sees the choices. The server may accept more.

`examples/agent_tools.rs` is a complete, small example. It builds five tool definitions from the schemas, runs a call by name, and keeps the answers short: `get_decision` returns at most 4000 characters of text unless the model asks for more, and `check_citations` returns the verdict counts, the unique citations and one short entry per finding. A failed call becomes `{"error": {"code", "status", "detail", "retry_after_seconds"}}`, which a model can read.

```sh
cargo run --example agent_tools --features schemars
```

If you want an MCP server and no code of your own, connect your client to `https://klaracase.de/mcp`.

## Testing

The builder takes your own `reqwest::Client` and a `base_url`. Point them at a local mock server in your tests. The client then sends real HTTP to the mock, with all headers and the retry rules in place.

```rust,no_run
use klaracase::Klaracase;

fn client_for_tests(mock_server_url: &str) -> Result<Klaracase, klaracase::Error> {
    Klaracase::builder()
        .token("klc_your_token_here")
        .base_url(mock_server_url)
        .http_client(reqwest::Client::new())
        .max_retries(0)
        .build()
}
```

Set `max_retries(0)` when a test expects the first error, so the test does not wait for a retry. This crate's own tests run against a mock server on a raw socket, with the 106 shared conformance cases from `tests/fixtures/conformance`.

## Requirements

- Rust 1.85 or newer (edition 2024). CI tests the minimum version.
- tokio only. The client is async and its default sleeper uses `tokio::time`. It needs a tokio runtime. Other runtimes are not supported. Without the `tokio` feature you must pass your own sleeper, which is meant for tests.
- TLS: rustls (default) or the platform library (`native-tls`). See Install for the build note.

## Versioning

This SDK is 0.x and tracks API release 0.96.0. Until 1.0, a minor version (0.1 to 0.2) may change the public surface, and a patch version (0.1.0 to 0.1.1) will not break your code. CI runs `cargo semver-checks` against the previous release tag to catch an accidental break. Every change is listed in [CHANGELOG.md](CHANGELOG.md). All Klaracase SDKs bump together when the API contract changes.

## Privacy

The SDK sends no telemetry. The only thing it adds to a request is the User-Agent header `klaracase-rust/<version> rust/<version>`, for example `klaracase-rust/0.1.0 rust/1.85.0`.

Klaracase is not affiliated with any court or with the federal government.

## More

- API reference: <https://klaracase.de/docs/api>
- MCP server for agents: <https://klaracase.de/mcp>, tools guide at <https://klaracase.de/docs/mcp>
- Support: support@klaracase.de

Run `make test` for the tests and `make lint` for formatting, clippy and docs. `make sync-check` compares the test fixtures with the shared contract folder. `make smoke` calls the two free endpoints with a real token. It is not part of CI.

## License

MIT. Copyright (c) 2026 SPARQ VENTURES UG.
