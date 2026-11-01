//! Runs the shared Klaracase conformance pack (`tests/fixtures/conformance/cases/*.json`) against a
//! local mock server. The rules are in `tests/fixtures/conformance/README.md`.
//!
//! Every case runs in one test function, one after the other, because cases set a process-wide
//! environment variable and reset the process-wide deprecation guard.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use klaracase::{
    CitationsParams, Clock, CoverageParams, CreateCitationCheckParams, CreateDatapackageParams,
    CrossTypeSearchParams, DeprecationNotice, Depth, Direction, Error, Extras, FacetField,
    FacetType, FacetsParams, GetDecisionParams, GetNormParams, Klaracase, Response,
    SearchDecisionsParams, SleepFuture, Sleeper, Statute, UploadParams, WaitParams,
};
use serde::Serialize;
use serde_json::{Map, Value, json};

use common::{MockResponse, MockServer, RecordedRequest};

const EXPECTED_CASES: usize = 102;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
}

// ---------------------------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------------------------

/// Records every wait and returns at once.
#[derive(Default)]
struct RecordingSleeper {
    sleeps: Mutex<Vec<Duration>>,
}

impl Sleeper for RecordingSleeper {
    fn sleep(&self, duration: Duration) -> SleepFuture<'_> {
        self.sleeps.lock().unwrap().push(duration);
        Box::pin(async {})
    }
}

struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }
}

// ---------------------------------------------------------------------------------------------
// What a call produced
// ---------------------------------------------------------------------------------------------

#[allow(clippy::large_enum_variant)]
enum Outcome {
    Success {
        typed: Value,
        raw: Value,
        extra: Map<String, Value>,
        meta: Value,
    },
    Failure(Error),
}

fn outcome<T: Serialize + Extras>(result: Result<Response<T>, Error>) -> Outcome {
    match result {
        Ok(response) => Outcome::Success {
            typed: serde_json::to_value(&response.body).expect("serialize body"),
            raw: response.raw().clone(),
            extra: response.body.extra().clone(),
            meta: json!({
                "status": response.meta.status,
                "api_release": response.meta.api_release,
                "request_duration": response.meta.request_duration.as_secs_f64(),
            }),
        },
        Err(error) => Outcome::Failure(error),
    }
}

// ---------------------------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------------------------

struct Args<'a>(&'a Map<String, Value>);

impl Args<'_> {
    fn string(&self, key: &str) -> Option<String> {
        self.0.get(key).and_then(Value::as_str).map(str::to_owned)
    }

    fn required(&self, key: &str) -> String {
        self.string(key)
            .unwrap_or_else(|| panic!("case needs the argument `{key}`"))
    }

    fn number(&self, key: &str) -> Option<u32> {
        self.0
            .get(key)
            .and_then(Value::as_u64)
            .map(|n| u32::try_from(n).expect("u32"))
    }

    fn boolean(&self, key: &str) -> Option<bool> {
        self.0.get(key).and_then(Value::as_bool)
    }

    fn seconds(&self, key: &str, default: Duration) -> Duration {
        self.0
            .get(key)
            .and_then(Value::as_f64)
            .map_or(default, Duration::from_secs_f64)
    }

    fn parsed<T: std::str::FromStr>(&self, key: &str) -> Option<T>
    where
        T::Err: std::fmt::Debug,
    {
        self.string(key).map(|text| {
            text.parse()
                .unwrap_or_else(|e| panic!("bad `{key}`: {e:?}"))
        })
    }

    fn list<T: std::str::FromStr>(&self, key: &str) -> Vec<T>
    where
        T::Err: std::fmt::Debug,
    {
        self.0
            .get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.as_str().expect("string").parse().expect("known value"))
                    .collect()
            })
            .unwrap_or_default()
    }
}

async fn dispatch(
    client: &Klaracase,
    group: Option<&str>,
    method: &str,
    args: &Map<String, Value>,
) -> Outcome {
    let a = Args(args);
    match (group, method) {
        (Some("decisions"), "search") => outcome(
            client
                .decisions()
                .search(SearchDecisionsParams {
                    query: a.required("query"),
                    court: a.string("court"),
                    branch: a.parsed("branch"),
                    document_type: a.parsed("document_type"),
                    cited_norm: a.string("cited_norm"),
                    cited_statutes: a.list::<Statute>("cited_statutes"),
                    date_from: a.string("date_from"),
                    date_to: a.string("date_to"),
                    mode: a.parsed("mode"),
                    limit: a.number("limit"),
                    page: a.number("page"),
                    bridge: a.boolean("bridge"),
                })
                .await,
        ),
        (None, "search") => outcome(
            client
                .search(CrossTypeSearchParams {
                    query: a.required("query"),
                    jurisdiction: a.string("jurisdiction"),
                    court: a.string("court"),
                    branch: a.parsed("branch"),
                    date_from: a.string("date_from"),
                    date_to: a.string("date_to"),
                    mode: a.parsed("mode"),
                    document_type: a.parsed("document_type"),
                    cited_norm: a.string("cited_norm"),
                    cited_statutes: a.list::<Statute>("cited_statutes"),
                    limit: a.number("limit"),
                    bridge: a.boolean("bridge"),
                })
                .await,
        ),
        (Some("decisions"), "get") => outcome(
            client
                .decisions()
                .get(
                    &a.required("id"),
                    GetDecisionParams {
                        max_chars: a.number("max_chars"),
                        rn_from: a.number("rn_from"),
                        rn_to: a.number("rn_to"),
                        section: a.string("section"),
                        court: a.string("court"),
                        date: a.string("date"),
                    },
                )
                .await,
        ),
        (Some("decisions"), "citations") => outcome(
            client
                .decisions()
                .citations(
                    &a.required("id"),
                    CitationsParams {
                        direction: a.parsed::<Direction>("direction"),
                        depth: a
                            .number("depth")
                            .map(|n| if n == 1 { Depth::One } else { Depth::Two }),
                        court: a.string("court"),
                        date: a.string("date"),
                        page: a.number("page"),
                    },
                )
                .await,
        ),
        (Some("norms"), "get") => outcome(
            client
                .norms()
                .get(
                    &a.required("reference"),
                    GetNormParams {
                        jurisdiction: a.string("jurisdiction"),
                    },
                )
                .await,
        ),
        (Some("datapackages"), "create") => outcome(
            client
                .datapackages()
                .create(CreateDatapackageParams {
                    ids: args
                        .get("ids")
                        .and_then(Value::as_array)
                        .expect("ids")
                        .iter()
                        .map(|id| id.as_str().expect("string").to_owned())
                        .collect(),
                    include_citations: a.boolean("include_citations"),
                    full_text: a.boolean("full_text"),
                    max_chars: a.number("max_chars"),
                })
                .await,
        ),
        (Some("citation_checks"), "create") => outcome(
            client
                .citation_checks()
                .create(CreateCitationCheckParams {
                    text: a.string("text"),
                    document_id: a.string("document_id"),
                    reference_date: a.string("reference_date"),
                })
                .await,
        ),
        (Some("citation_checks"), "get") => {
            outcome(client.citation_checks().get(&a.required("run_id")).await)
        }
        (Some("citation_check_documents"), "upload") => {
            let file = args
                .get("file")
                .and_then(|f| f.get("$file"))
                .and_then(Value::as_str)
                .expect("$file");
            let bytes = std::fs::read(fixtures().join(file)).expect("read upload fixture");
            outcome(
                client
                    .citation_check_documents()
                    .upload(UploadParams {
                        bytes,
                        filename: a.required("filename"),
                        content_type: a.required("content_type"),
                    })
                    .await,
            )
        }
        (Some("citation_check_documents"), "get") => outcome(
            client
                .citation_check_documents()
                .get(&a.required("document_id"))
                .await,
        ),
        (Some("citation_check_documents"), "wait_until_ready") => {
            let defaults = WaitParams::default();
            outcome(
                client
                    .citation_check_documents()
                    .wait_until_ready(
                        &a.required("document_id"),
                        WaitParams {
                            poll_interval: a.seconds("poll_interval", defaults.poll_interval),
                            timeout: a.seconds("timeout", defaults.timeout),
                        },
                    )
                    .await,
            )
        }
        (Some("corpus"), "coverage") => outcome(
            client
                .corpus()
                .coverage(CoverageParams {
                    branch: a.parsed("branch"),
                    top: a.number("top"),
                })
                .await,
        ),
        (Some("corpus"), "facets") => outcome(
            client
                .corpus()
                .facets(
                    a.parsed::<FacetField>("field").expect("field"),
                    FacetsParams {
                        type_: a.parsed::<FacetType>("type"),
                        norm: a.string("norm"),
                        prefix: a.string("prefix"),
                    },
                )
                .await,
        ),
        other => panic!("the runner has no dispatch for {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Path lookups and matchers
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
enum Step {
    Key(String),
    Index(usize),
}

/// Parses `a.b[0]["§ 1.2"].c` into steps.
fn parse_path(path: &str) -> Vec<Step> {
    let chars: Vec<char> = path.chars().collect();
    let mut steps = Vec::new();
    let mut i = 0;
    let mut name = String::new();
    let flush = |name: &mut String, steps: &mut Vec<Step>| {
        if !name.is_empty() {
            steps.push(Step::Key(std::mem::take(name)));
        }
    };
    while i < chars.len() {
        match chars[i] {
            '.' => {
                flush(&mut name, &mut steps);
                i += 1;
            }
            '[' if chars.get(i + 1) == Some(&'"') => {
                flush(&mut name, &mut steps);
                let end = (i + 2..chars.len())
                    .find(|&j| chars[j] == '"' && chars.get(j + 1) == Some(&']'))
                    .expect("closing quote");
                steps.push(Step::Key(chars[i + 2..end].iter().collect()));
                i = end + 2;
            }
            '[' => {
                flush(&mut name, &mut steps);
                let end = (i..chars.len())
                    .find(|&j| chars[j] == ']')
                    .expect("closing bracket");
                let index: String = chars[i + 1..end].iter().collect();
                steps.push(Step::Index(index.parse().expect("index")));
                i = end + 1;
            }
            c => {
                name.push(c);
                i += 1;
            }
        }
    }
    flush(&mut name, &mut steps);
    steps
}

fn lookup<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    parse_path(path)
        .iter()
        .try_fold(root, |value, step| match step {
            Step::Key(key) => value.get(key.as_str()),
            Step::Index(index) => value.get(*index),
        })
}

fn matches(actual: &Value, expected: &Value) -> Result<(), String> {
    let Some(matchers) = expected
        .as_object()
        .filter(|o| !o.is_empty() && o.keys().all(|k| k.starts_with('$')))
    else {
        return if actual == expected {
            Ok(())
        } else {
            Err(format!("expected {expected}, got {actual}"))
        };
    };
    for (matcher, wanted) in matchers {
        let ok = match matcher.as_str() {
            "$type" => {
                let kind = match actual {
                    Value::Null => "null",
                    Value::Bool(_) => "boolean",
                    Value::Number(_) => "number",
                    Value::String(_) => "string",
                    Value::Array(_) => "array",
                    Value::Object(_) => "object",
                };
                wanted.as_str() == Some(kind)
            }
            "$min" => actual
                .as_f64()
                .zip(wanted.as_f64())
                .is_some_and(|(a, w)| a >= w),
            "$length" => {
                let length = match actual {
                    Value::String(s) => Some(s.chars().count()),
                    Value::Array(a) => Some(a.len()),
                    _ => None,
                };
                length
                    .zip(wanted.as_u64())
                    .is_some_and(|(l, w)| l as u64 == w)
            }
            "$contains" => actual
                .as_str()
                .zip(wanted.as_str())
                .is_some_and(|(a, w)| a.contains(w)),
            "$starts_with" => actual
                .as_str()
                .zip(wanted.as_str())
                .is_some_and(|(a, w)| a.starts_with(w)),
            other => return Err(format!("unknown matcher {other}")),
        };
        if !ok {
            return Err(format!("matcher {matcher}={wanted} failed on {actual}"));
        }
    }
    Ok(())
}

fn strip_nulls(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), strip_nulls(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip_nulls).collect()),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------------------------
// Request matching
// ---------------------------------------------------------------------------------------------

fn check_request(seen: &RecordedRequest, expected: &Value, case: &Value) -> Result<(), String> {
    let method = expected["method"].as_str().unwrap();
    if seen.method != method {
        return Err(format!("method {} != {method}", seen.method));
    }
    let wanted_path = format!("/api/v1{}", expected["path"].as_str().unwrap());
    if seen.path != wanted_path {
        return Err(format!("raw path {} != {wanted_path}", seen.path));
    }

    // Query: names equal as a set; list order matters; values compare decoded.
    let mut actual: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in seen.query_pairs() {
        actual.entry(name).or_default().push(value);
    }
    let mut wanted: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in expected["query"].as_object().unwrap() {
        let values = match value {
            Value::Array(items) => items
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect(),
            other => vec![other.as_str().unwrap().to_owned()],
        };
        wanted.insert(name.clone(), values);
    }
    if actual != wanted {
        return Err(format!("query {actual:?} != {wanted:?}"));
    }

    let suffix = expected.get("user_agent_suffix").and_then(Value::as_str);
    for (name, value) in expected["headers"].as_object().unwrap() {
        let value = value.as_str().unwrap();
        let got = seen
            .header(name)
            .ok_or_else(|| format!("header {name} missing"))?;
        match name.as_str() {
            "user-agent" => {
                if !got.starts_with(value) {
                    return Err(format!("user-agent {got} lacks prefix {value}"));
                }
                if suffix.is_some_and(|suffix| !got.ends_with(&format!(" {suffix}"))) {
                    return Err(format!("user-agent {got} lacks suffix {suffix:?}"));
                }
            }
            "content-type" => {
                let media = got.split(';').next().unwrap().trim().to_ascii_lowercase();
                if media != value {
                    return Err(format!("content-type {media} != {value}"));
                }
            }
            _ => {
                if got != value {
                    return Err(format!("header {name}: {got} != {value}"));
                }
            }
        }
    }
    if expected["headers"].get("content-type").is_none()
        && method == "GET"
        && seen.header("content-type").is_some()
    {
        return Err("a GET must not send content-type".to_owned());
    }

    if let Some(json) = expected.get("json") {
        let body: Value =
            serde_json::from_slice(&seen.body).map_err(|e| format!("body is not JSON: {e}"))?;
        if &body != json {
            return Err(format!("json body {body} != {json}"));
        }
    }
    if let Some(parts) = expected.get("multipart") {
        let got = parse_multipart(seen)?;
        let wanted: Vec<(String, String, String)> = parts
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p["name"].as_str().unwrap().to_owned(),
                    p["filename"].as_str().unwrap().to_owned(),
                    p["content_type"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        if got != wanted {
            return Err(format!("multipart {got:?} != {wanted:?}"));
        }
    }
    let _ = case;
    Ok(())
}

fn parse_multipart(seen: &RecordedRequest) -> Result<Vec<(String, String, String)>, String> {
    let content_type = seen.header("content-type").ok_or("no content-type")?;
    let boundary = content_type
        .split(';')
        .filter_map(|part| part.trim().strip_prefix("boundary="))
        .next()
        .ok_or("no boundary")?
        .trim_matches('"')
        .to_owned();
    let body = String::from_utf8_lossy(&seen.body).into_owned();
    let delimiter = format!("--{boundary}");
    let mut parts = Vec::new();
    for section in body.split(&delimiter).skip(1) {
        if section.starts_with("--") {
            break;
        }
        let head = section.split("\r\n\r\n").next().unwrap_or("");
        let mut name = String::new();
        let mut filename = String::new();
        let mut part_type = String::new();
        for line in head.split("\r\n").filter(|l| !l.is_empty()) {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("content-disposition:") {
                for piece in line.split(';').map(str::trim) {
                    if let Some(v) = piece.strip_prefix("name=") {
                        name = v.trim_matches('"').to_owned();
                    }
                    if let Some(v) = piece.strip_prefix("filename=") {
                        filename = v.trim_matches('"').to_owned();
                    }
                }
            } else if lower.starts_with("content-type:") {
                part_type = line["content-type:".len()..].trim().to_owned();
            }
        }
        parts.push((name, filename, part_type));
    }
    Ok(parts)
}

// ---------------------------------------------------------------------------------------------
// Error view
// ---------------------------------------------------------------------------------------------

fn error_class(error: &Error) -> &'static str {
    match error {
        Error::Authentication(_) => "AuthenticationError",
        Error::Permission(_) => "PermissionError",
        Error::NotFound(_) => "NotFoundError",
        Error::Conflict(_) => "ConflictError",
        Error::Validation(_) => "ValidationError",
        Error::InsufficientCredits { .. } => "InsufficientCreditsError",
        Error::RateLimit { .. } => "RateLimitError",
        Error::ServiceUnavailable(_) => "ServiceUnavailableError",
        Error::Api(_) => "ApiError",
        Error::Transport(_) | Error::Timeout(_) => "TransportError",
        Error::DocumentFailed { .. } => "DocumentFailedError",
        Error::DocumentTimeout { .. } => "DocumentTimeoutError",
        // The pack names the client-side input error ValidationError (code client_validation).
        Error::Configuration {
            code: "client_validation",
            ..
        } => "ValidationError",
        Error::Configuration { .. } => "ConfigurationError",
        _ => "UnknownError",
    }
}

/// The error as a JSON object for `fields` lookups. Typed members win over `extra` members of the
/// same name; a bare name the SDK does not type falls through to `extra`.
fn error_view(error: &Error) -> Value {
    let mut view = Map::new();
    if let Some(problem) = error.problem() {
        view.extend(problem.extra.clone());
        view.insert("type".into(), json!(problem.type_));
        view.insert("title".into(), json!(problem.title));
        view.insert("status".into(), json!(problem.status));
        view.insert("detail".into(), json!(problem.detail));
        view.insert("code".into(), json!(problem.code));
        view.insert("api_release".into(), json!(problem.api_release));
        view.insert("extra".into(), Value::Object(problem.extra.clone()));
        if let Some(ability) = problem.required_ability() {
            view.insert("required_ability".into(), json!(ability));
        }
        if let Some(errors) = problem.errors() {
            view.insert("errors".into(), Value::Object(errors.clone()));
        }
    }
    match error {
        Error::InsufficientCredits { cost, balance, .. } => {
            view.insert("cost".into(), json!(cost));
            view.insert("balance".into(), json!(balance));
        }
        Error::RateLimit {
            retry_after_seconds,
            ..
        } => {
            view.insert("retry_after_seconds".into(), json!(retry_after_seconds));
        }
        Error::DocumentFailed { receipt } | Error::DocumentTimeout { receipt } => {
            view.insert("error_code".into(), json!(receipt.error_code));
            view.insert("document".into(), serde_json::to_value(receipt).unwrap());
        }
        Error::Configuration { message, .. } => {
            view.insert("message".into(), json!(message));
        }
        _ => {}
    }
    Value::Object(view)
}

// ---------------------------------------------------------------------------------------------
// One case
// ---------------------------------------------------------------------------------------------

fn build_mock_response(entry: &Value, case_dir: &Path) -> MockResponse {
    if entry.get("drop_connection").and_then(Value::as_bool) == Some(true) {
        return MockResponse::dropped();
    }
    let mut headers: Vec<(String, String)> = entry
        .get("headers")
        .and_then(Value::as_object)
        .map(|h| {
            h.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let mut body = Vec::new();
    let mut is_json = false;
    if let Some(inline) = entry.get("body") {
        body = serde_json::to_vec(inline).unwrap();
        is_json = true;
    } else if let Some(file) = entry.get("body_file").and_then(Value::as_str) {
        let path = case_dir.join(file);
        body = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        is_json = path.extension().is_some_and(|e| e == "json");
    } else if let Some(text) = entry.get("body_text").and_then(Value::as_str) {
        body = text.as_bytes().to_vec();
    }
    if is_json && !headers.iter().any(|(k, _)| k == "content-type") {
        headers.push(("content-type".to_owned(), "application/json".to_owned()));
    }
    MockResponse {
        status: entry["status"].as_u64().unwrap_or(599) as u16,
        headers,
        body,
        delay: Duration::from_secs_f64(
            entry
                .get("delay_seconds")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        ),
        drop_connection: false,
    }
}

async fn run_case(path: &Path) -> Result<(), String> {
    let case: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let case_dir = path.parent().unwrap();
    let runner = case.get("runner").cloned().unwrap_or_else(|| json!({}));
    let real_clock = runner
        .get("real_clock")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let repeat_last = runner
        .get("repeat_last_response")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Step 1: reset state.
    // SAFETY: this test binary runs this function from a single test, one case at a time.
    unsafe {
        std::env::remove_var("KLARACASE_API_TOKEN");
        for (name, value) in case["sdk"]
            .get("env")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            std::env::set_var(name, value.as_str().unwrap());
        }
    }
    klaracase::reset_deprecation_guard();

    // Step 2: mock.
    let responses: Vec<MockResponse> = case["responses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| build_mock_response(entry, case_dir))
        .collect();
    let server = MockServer::start(responses, repeat_last).await;

    // Step 3: client.
    let options = case["sdk"]
        .get("options")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let sleeper = Arc::new(RecordingSleeper::default());
    let notices: Arc<Mutex<Vec<DeprecationNotice>>> = Arc::default();
    let mut base_url = server.base_url();
    if options
        .get("base_url_trailing_slash")
        .and_then(Value::as_bool)
        == Some(true)
    {
        base_url.push('/');
    }
    let mut builder = Klaracase::builder().base_url(base_url).on_deprecation({
        let notices = Arc::clone(&notices);
        Arc::new(move |notice| notices.lock().unwrap().push(notice))
    });
    match options.get("token") {
        None => builder = builder.token("test_token_123"),
        Some(Value::Null) => {}
        Some(token) => builder = builder.token(token.as_str().unwrap()),
    }
    if let Some(n) = options.get("max_retries").and_then(Value::as_u64) {
        builder = builder.max_retries(n as u32);
    }
    if let Some(seconds) = options.get("timeout").and_then(Value::as_f64) {
        builder = builder.timeout(Duration::from_secs_f64(seconds));
    }
    if let Some(suffix) = options.get("user_agent_suffix").and_then(Value::as_str) {
        builder = builder.user_agent_suffix(suffix);
    }
    if !real_clock {
        builder = builder.sleeper(sleeper.clone()).clock(Arc::new(RealClock));
    }
    let client = match builder.build() {
        Ok(client) => client,
        Err(error) => {
            let outcomes = vec![Outcome::Failure(error)];
            return check_outcomes(&case, &server, &sleeper, &notices, outcomes, real_clock);
        }
    };

    // Steps 4 and 5: call.
    let sdk = &case["sdk"];
    let args = sdk
        .get("args")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let repeat = sdk.get("repeat").and_then(Value::as_u64).unwrap_or(1);
    let mut outcomes = Vec::new();
    for _ in 0..repeat {
        outcomes.push(
            dispatch(
                &client,
                sdk["group"].as_str(),
                sdk["method"].as_str().unwrap(),
                &args,
            )
            .await,
        );
    }
    check_outcomes(&case, &server, &sleeper, &notices, outcomes, real_clock)
}

fn check_outcomes(
    case: &Value,
    server: &MockServer,
    sleeper: &RecordingSleeper,
    notices: &Mutex<Vec<DeprecationNotice>>,
    outcomes: Vec<Outcome>,
    real_clock: bool,
) -> Result<(), String> {
    let expect = &case["expect"];

    // Requests.
    let seen = server.requests();
    match &expect["request"] {
        Value::Null => {
            if !seen.is_empty() {
                return Err(format!("expected no request, saw {}", seen.len()));
            }
        }
        wanted => {
            for (n, request) in seen.iter().enumerate() {
                check_request(request, wanted, case)
                    .map_err(|e| format!("request {}: {e}", n + 1))?;
            }
        }
    }
    let count = seen.len();
    match &expect["request_count"] {
        Value::Number(n) => {
            if count as u64 != n.as_u64().unwrap() {
                return Err(format!("request_count {count} != {n}"));
            }
        }
        range => {
            let (min, max) = (
                range["min"].as_u64().unwrap(),
                range["max"].as_u64().unwrap(),
            );
            if (count as u64) < min || (count as u64) > max {
                return Err(format!("request_count {count} outside {min}..={max}"));
            }
        }
    }
    if server.unmatched() > 0 {
        return Err(format!(
            "{} request(s) found no answer left",
            server.unmatched()
        ));
    }

    // Sleeps.
    if let (Some(wanted), false) = (expect.get("sleeps").and_then(Value::as_array), real_clock) {
        let slept = sleeper.sleeps.lock().unwrap().clone();
        if slept.len() != wanted.len() {
            return Err(format!("sleeps {slept:?} != {wanted:?}"));
        }
        for (got, want) in slept.iter().zip(wanted) {
            let got = got.as_secs_f64();
            let ok = match want.get("exact").and_then(Value::as_f64) {
                Some(exact) => (got - exact).abs() < 1e-9,
                None => {
                    got >= want["min"].as_f64().unwrap() - 1e-9
                        && got <= want["max"].as_f64().unwrap() + 1e-9
                }
            };
            if !ok {
                return Err(format!("sleep {got} does not fit {want}"));
            }
        }
    }

    // Result.
    let expected = &case["expect_result"];
    for outcome in &outcomes {
        match (expected["kind"].as_str().unwrap(), outcome) {
            (
                "success",
                Outcome::Success {
                    typed,
                    raw,
                    extra,
                    meta,
                },
            ) => {
                check_success(expected, typed, raw, extra, meta)?;
            }
            ("error", Outcome::Failure(error)) => check_error(case, expected, error, server)?,
            ("success", Outcome::Failure(error)) => {
                return Err(format!("expected success, got {error}"));
            }
            (_, Outcome::Success { .. }) => return Err("expected an error, got success".to_owned()),
            (other, _) => return Err(format!("unknown result kind {other}")),
        }
    }

    // Deprecation hook.
    let delivered = notices.lock().unwrap().clone();
    let expect_warning = expected.get("deprecation_warning").and_then(Value::as_bool);
    if expect_warning.is_some_and(|wanted| wanted == delivered.is_empty()) {
        return Err(format!(
            "deprecation_warning expected {expect_warning:?}, hook ran {} times",
            delivered.len()
        ));
    }
    let expect_count = expected
        .get("deprecation_warning_count")
        .and_then(Value::as_u64);
    if expect_count.is_some_and(|total| delivered.len() as u64 != total) {
        return Err(format!(
            "deprecation hook ran {} times, expected {expect_count:?}",
            delivered.len()
        ));
    }
    if let Some(raw) = expected.get("deprecation").and_then(Value::as_object) {
        let first = delivered.first().ok_or("hook did not run")?;
        for (key, want) in raw {
            let got = match key.as_str() {
                "deprecation" => &first.deprecation,
                "sunset" => &first.sunset,
                "link" => &first.link,
                other => return Err(format!("unknown deprecation key {other}")),
            };
            if got.as_deref() != want.as_str() {
                return Err(format!("deprecation {key}: {got:?} != {want}"));
            }
        }
    }
    Ok(())
}

fn check_success(
    expected: &Value,
    typed: &Value,
    raw: &Value,
    extra: &Map<String, Value>,
    meta: &Value,
) -> Result<(), String> {
    let extra_root = Value::Object(extra.clone());
    for (path, want) in expected["assert"].as_object().unwrap() {
        let found = if let Some(rest) = path.strip_prefix("meta.") {
            meta.get(rest)
        } else if let Some(rest) = path.strip_prefix("extra.") {
            lookup(&extra_root, rest)
        } else {
            lookup(typed, path)
        };
        let found = found.ok_or_else(|| format!("path `{path}` not found"))?;
        matches(found, want).map_err(|e| format!("`{path}`: {e}"))?;
    }
    if expected.get("body_roundtrip").and_then(Value::as_bool) == Some(true) {
        // The lossless view is the raw JSON. The typed view must carry the same data: nothing
        // renamed, dropped or cleaned, unknown keys included. A `null` and an absent key are one
        // thing here because the typed view writes `None` as `null`.
        if strip_nulls(typed) != strip_nulls(raw) {
            return Err(format!(
                "typed view differs from the body\n typed: {}\n raw:   {}",
                strip_nulls(typed),
                strip_nulls(raw)
            ));
        }
    }
    Ok(())
}

fn check_error(
    case: &Value,
    expected: &Value,
    error: &Error,
    server: &MockServer,
) -> Result<(), String> {
    let class = expected["class"].as_str().unwrap();
    if error_class(error) != class {
        return Err(format!("class {} != {class} ({error})", error_class(error)));
    }
    if class != "ConfigurationError" {
        let want_code = expected.get("code").and_then(Value::as_str);
        if error.code() != want_code {
            return Err(format!("code {:?} != {want_code:?}", error.code()));
        }
        let want_status = expected
            .get("status")
            .and_then(Value::as_u64)
            .map(|s| s as u16);
        if error.status() != want_status {
            return Err(format!("status {:?} != {want_status:?}", error.status()));
        }
    }

    // Checks the runner makes without the case saying so: the problem equals the body.
    if let Some(problem) = error.problem() {
        let last = server.served().last().copied().ok_or("no answer served")?;
        let entry = &case["responses"][last];
        let body: Option<Value> = if let Some(inline) = entry.get("body") {
            Some(inline.clone())
        } else if let Some(file) = entry.get("body_file").and_then(Value::as_str) {
            let path = fixtures().join("cases").join(file);
            serde_json::from_slice(&std::fs::read(path).unwrap()).ok()
        } else {
            None
        };
        if let Some(Value::Object(members)) =
            body.filter(|b| b.get("code").is_some_and(Value::is_string))
        {
            for (key, got) in [
                ("type", json!(problem.type_)),
                ("title", json!(problem.title)),
                ("detail", json!(problem.detail)),
                ("status", json!(problem.status)),
                ("code", json!(problem.code)),
            ] {
                if members.get(key) != Some(&got) {
                    return Err(format!(
                        "problem.{key} {got} != body {:?}",
                        members.get(key)
                    ));
                }
            }
            let rest: Map<String, Value> = members
                .iter()
                .filter(|(k, _)| {
                    !["type", "title", "status", "detail", "code"].contains(&k.as_str())
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if rest != problem.extra {
                return Err(format!("problem.extra {:?} != {rest:?}", problem.extra));
            }
        }
        let want_release = entry["headers"]
            .get("x-klaracase-api-release")
            .and_then(Value::as_str);
        if problem.api_release.as_deref() != want_release {
            return Err(format!(
                "api_release {:?} != {want_release:?}",
                problem.api_release
            ));
        }
    }

    let view = error_view(error);
    if let Some(fields) = expected.get("fields").and_then(Value::as_object) {
        for (path, want) in fields {
            let found =
                lookup(&view, path).ok_or_else(|| format!("field `{path}` not found in {view}"))?;
            matches(found, want).map_err(|e| format!("field `{path}`: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The test
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_conformance_pack_passes() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures().join("cases"))
        .expect("cases directory")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    assert_eq!(
        paths.len(),
        EXPECTED_CASES,
        "the pack should hold {EXPECTED_CASES} cases"
    );

    let mut failures = Vec::new();
    for path in &paths {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        if let Err(message) = run_case(path).await {
            failures.push(format!("{name}: {message}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n{}",
        failures.len(),
        paths.len(),
        failures.join("\n")
    );
}
