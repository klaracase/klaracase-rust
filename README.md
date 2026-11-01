# klaracase

The official Rust client for the [Klaracase](https://klaracase.de) REST API. Klaracase holds German court decisions and German statutes, linked by a resolved citation graph.

Klaracase covers German law only. It is a research tool. It gives no legal advice and does not replace a lawyer's review. A `not_found` verdict means the citation was not found in the corpus; it is not a claim that the citation is fabricated.

This crate is a thin, hand-written client for API v1. It is async, built on `reqwest`, and covers all twelve operations. It does not implement MCP. If you build an agent, connect your MCP client to `https://klaracase.de/mcp` instead. The same token works for both.

## Install

```sh
cargo add klaracase
cargo add tokio --features macros,rt-multi-thread
```

The crate needs Rust 1.85 or newer (edition 2024). It runs on tokio only. The default TLS provider is rustls with aws-lc-rs, which compiles C code, so it needs a C compiler and, on some platforms, CMake or NASM. If that build is a problem, use the `native-tls` feature (see TLS below).

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

Parameter structs are built with `new(...)` and chained setters. A parameter you do not set is not sent. `limit(0)` and `bridge(false)` are values and are sent. Every parameter struct is `#[non_exhaustive]`, so a new parameter in a later release does not break your code.

A response dereferences to its typed body, so `found.results` works. `found.meta` holds the HTTP status, the API release and `request_duration`, the time of the final HTTP attempt (earlier attempts and the waits between them are not counted). `found.credits` says what the call cost. Keys the crate does not model stay in the `extra` map of the struct that held them. `found.raw()` returns the whole answer as `serde_json::Value`, as the server sent it.

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

The call is synchronous. The answer is the finished run. The closed set of verdicts is `verified_corpus`, `verified_external`, `not_found`, `out_of_coverage`, `ambiguous`, `unverifiable` and `discrepancy`. They are `&str` constants on `Verdict`. A verdict this crate does not know yet stays readable as a string.

Read the verdicts with care:

- `verified_corpus` shows that the cited decision or statute exists in the corpus. It does not show that a quote or a legal proposition is correct.
- `not_found` shows absence of evidence in the corpus. It is not a claim that the citation is fabricated. The API check uses the corpus only, so `not_found` is weaker evidence here than in the Klaracase web app.

Offsets in a finding are Unicode codepoint offsets into the exact string you sent. They are not byte offsets.

You give either `text` or `document_id`, never both. If you give both or neither, the call fails on the client with `Error::ClientValidation` (code `client_validation`) before any request.

### Check an uploaded file

```rust,no_run
use std::time::Duration;
use klaracase::{CreateCitationCheckParams, Klaracase, UploadParams};

#[tokio::main]
async fn main() -> Result<(), klaracase::Error> {
    let client = Klaracase::from_env()?;
    let bytes = std::fs::read("schriftsatz.pdf").expect("read the file");

    let receipt = client
        .citation_check_documents()
        .upload(UploadParams::new(bytes, "schriftsatz.pdf", "application/pdf"))
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

An upload accepts PDF, DOCX, Markdown and plain text up to 25 MB. It is free. `wait_until_ready` polls every 2 seconds for up to 120 seconds by default. It returns `Error::DocumentFailed` when extraction fails and `Error::DocumentTimeout` when time runs out. The status `ocr_pending` is not final, so the loop keeps going.

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
| `InsufficientCredits { cost, balance, problem }` | 402 |
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

## Retries

- The client retries a 429 and a 503, up to `max_retries` times (default 2). Both statuses guarantee that nothing ran and nothing was charged, so a retry is safe for every operation, including POST.
- It never retries 402, any other 4xx, 500 or 502.
- The wait is `retry_after_seconds` from the body, else the `Retry-After` header, else exponential backoff with full jitter (up to 0.5, 1, 2 seconds, capped at 8).
- A server-stated wait above 30 seconds is not slept. The client returns the error at once and you decide. A `volume_cap` answer is of this kind.
- A transport error (connection, DNS, TLS, timeout) is retried for GET only. A POST whose connection dropped after the request was sent is not retried. Repeating it by hand is free in the same period, because one charge covers each operation, organization and subject per calendar month.

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

## Credits

Calls are billed in credits, to the organization the token is bound to. Search, norm and decision reads cost 1 credit. The citation graph costs 2. A data package costs 5. A citation check costs `max(2, ceil(chars / 5000))`. Coverage, facets, uploads and reading a finished run are free.

One charge covers each operation, organization and subject per calendar month. The same call again in the same month costs nothing more, on REST or on MCP. Every response carries `credits` with `metered`, `cost` and `credits_remaining`.

The API allows 60 requests per minute per token. The three POST operations (`datapackages`, `citation_checks` and `citation_check_documents` upload) are limited to 10 per minute.

## TLS

The default feature `rustls` uses rustls. To use the platform TLS library instead:

```toml
klaracase = { version = "0.1", default-features = false, features = ["native-tls", "tokio"] }
```

| Feature | Default | Effect |
|---|---|---|
| `rustls` | yes | TLS through rustls |
| `native-tls` | no | TLS through the platform library |
| `tokio` | yes | the default `Sleeper` uses `tokio::time`. Without it you must pass your own `Sleeper`. |
| `log` | no | the default deprecation hook uses `log::warn!` |
| `tracing` | no | a `debug!` event per retry and a `warn!` on a deprecation, through the `tracing` crate. The token is never in an event. |

## Stability and privacy

This SDK is 0.x and tracks API release 0.95.0. Minor versions may change the public surface until 1.0. Every change is listed in [CHANGELOG.md](CHANGELOG.md). All Klaracase SDKs bump together when the API contract changes.

The SDK sends no telemetry. The only thing it adds to a request is the User-Agent header `klaracase-rust/<version> rust/<version>`, for example `klaracase-rust/0.1.0 rust/1.85.0`.

Klaracase is not affiliated with any court or with the federal government.

## More

- API reference: <https://klaracase.de/docs/api>
- MCP server for agents: <https://klaracase.de/mcp>, tools guide at <https://klaracase.de/docs/mcp>
- Support: support@klaracase.de

Run `make test` for the tests and `make lint` for formatting, clippy and docs. `make smoke` calls the two free endpoints with a real token. It is not part of CI.

## License

MIT. Copyright (c) 2026 SPARQ VENTURES UG.
