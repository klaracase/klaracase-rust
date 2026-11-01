//! Tests for behaviour the conformance pack does not fix: injected clocks, the HTTP client seam, zero
//! values on the wire, and the error accessors.

mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use klaracase::{
    Clock, CreateDatapackageParams, Error, Klaracase, SearchDecisionsParams, SleepFuture, Sleeper,
    WaitParams,
};
use serde_json::Value;

use common::{MockResponse, MockServer};

const SEARCH_BODY: &str = include_str!("fixtures/examples/decisions.search.json");
const EXTRACTING: &str = include_str!("fixtures/conformance/bodies/documents.extracting.json");
const DOCUMENT_ID: &str = "0193b0c6-3c4d-7e5f-8a60-7c8d9e0f1a2b";

/// A clock that only moves when the sleeper sleeps.
#[derive(Clone)]
struct FakeTime {
    base: Instant,
    elapsed_millis: Arc<AtomicU64>,
    sleeps: Arc<Mutex<Vec<Duration>>>,
}

impl FakeTime {
    fn new() -> Self {
        Self {
            base: Instant::now(),
            elapsed_millis: Arc::default(),
            sleeps: Arc::default(),
        }
    }
}

impl Clock for FakeTime {
    fn now(&self) -> Instant {
        self.base + Duration::from_millis(self.elapsed_millis.load(Ordering::SeqCst))
    }
}

impl Sleeper for FakeTime {
    fn sleep(&self, duration: Duration) -> SleepFuture<'_> {
        self.elapsed_millis
            .fetch_add(duration.as_millis() as u64, Ordering::SeqCst);
        self.sleeps.lock().unwrap().push(duration);
        Box::pin(async {})
    }
}

fn client_for(server: &MockServer, time: &FakeTime) -> Klaracase {
    Klaracase::builder()
        .token("klc_your_token_here")
        .base_url(server.base_url())
        .sleeper(Arc::new(time.clone()))
        .clock(Arc::new(time.clone()))
        .build()
        .expect("client")
}

#[tokio::test]
async fn wait_until_ready_times_out_on_the_injected_clock() {
    let server = MockServer::start(vec![MockResponse::json(200, EXTRACTING)], true).await;
    let time = FakeTime::new();
    let client = client_for(&server, &time);

    let error = client
        .citation_check_documents()
        .wait_until_ready(
            DOCUMENT_ID,
            WaitParams {
                poll_interval: Duration::from_secs(2),
                timeout: Duration::from_secs(10),
            },
        )
        .await
        .unwrap_err();

    let Error::DocumentTimeout { receipt } = &error else {
        panic!("wrong error: {error}")
    };
    assert_eq!(receipt.status, "extracting");
    assert_eq!(error.code(), Some("wait_timeout"));
    // Polls at 0, 2, 4, 6, 8 and 10 seconds of fake time; the sixth poll sees the deadline.
    assert_eq!(server.requests().len(), 6);
    assert_eq!(time.sleeps.lock().unwrap().len(), 5);
}

#[tokio::test]
async fn zero_and_false_are_sent_and_none_is_not() {
    let server = MockServer::start(vec![MockResponse::json(200, SEARCH_BODY)], true).await;
    let client = client_for(&server, &FakeTime::new());

    client
        .decisions()
        .search(SearchDecisionsParams {
            query: "Untreue".into(),
            limit: Some(0),
            page: Some(0),
            bridge: Some(false),
            ..Default::default()
        })
        .await
        .expect("search");

    let mut pairs = server.requests()[0].query_pairs();
    pairs.sort();
    let expected: Vec<(String, String)> = [
        ("bridge", "false"),
        ("limit", "0"),
        ("page", "0"),
        ("query", "Untreue"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    assert_eq!(pairs, expected);
}

#[tokio::test]
async fn datapackage_body_holds_only_what_was_set() {
    let body = include_str!("fixtures/examples/datapackages.store.json");
    let server = MockServer::start(vec![MockResponse::json(200, body)], true).await;
    let client = client_for(&server, &FakeTime::new());

    client
        .datapackages()
        .create(CreateDatapackageParams {
            full_text: Some(false),
            ..CreateDatapackageParams::new(["1 StR 185/01"])
        })
        .await
        .expect("create");

    let sent: Value = serde_json::from_slice(&server.requests()[0].body).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({"ids": ["1 StR 185/01"], "full_text": false})
    );
}

#[tokio::test]
async fn an_injected_http_client_is_used() {
    let server = MockServer::start(vec![MockResponse::json(200, SEARCH_BODY)], true).await;
    let http = reqwest::Client::builder()
        .default_headers(
            [(
                reqwest::header::HeaderName::from_static("x-from-injected-client"),
                "yes".parse().unwrap(),
            )]
            .into_iter()
            .collect(),
        )
        .build()
        .unwrap();
    let client = Klaracase::builder()
        .token("klc_your_token_here")
        .base_url(server.base_url())
        .http_client(http)
        .build()
        .unwrap();

    client
        .decisions()
        .search(SearchDecisionsParams::new("Untreue"))
        .await
        .expect("search");

    let request = &server.requests()[0];
    assert_eq!(request.header("x-from-injected-client"), Some("yes"));
    assert_eq!(
        request.header("authorization"),
        Some("Bearer klc_your_token_here")
    );
}

#[tokio::test]
async fn a_2xx_answer_that_is_not_json_is_a_decode_error() {
    let server =
        MockServer::start(vec![MockResponse::json(200, "<html>not json</html>")], true).await;
    let client = client_for(&server, &FakeTime::new());

    let error = client
        .decisions()
        .search(SearchDecisionsParams::new("Untreue"))
        .await
        .unwrap_err();

    assert!(
        matches!(error, Error::Decode { status: 200, .. }),
        "{error}"
    );
}

#[tokio::test]
async fn path_parameters_that_a_url_parser_would_collapse_are_refused_before_a_request() {
    let server = MockServer::start(vec![], true).await;
    let client = client_for(&server, &FakeTime::new());

    for bad in ["", ".", ".."] {
        let error = client
            .decisions()
            .get(bad, klaracase::GetDecisionParams::default())
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some("client_validation"), "{bad:?}");
    }
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn error_accessors_expose_the_problem() {
    let body = include_str!("fixtures/conformance/problems/token_ability_missing.json");
    let server = MockServer::start(vec![MockResponse::json(403, body)], true).await;
    let client = client_for(&server, &FakeTime::new());

    let error = client
        .decisions()
        .search(SearchDecisionsParams::new("Untreue"))
        .await
        .unwrap_err();

    assert!(matches!(error, Error::Permission(_)));
    assert_eq!(error.code(), Some("token_ability_missing"));
    assert_eq!(error.status(), Some(403));
    assert_eq!(
        error.problem().and_then(|p| p.required_ability()),
        Some("citation-check")
    );
    assert!(!error.is_transport());
}

#[tokio::test]
async fn the_token_never_appears_in_an_error_message() {
    let server = MockServer::start(
        vec![MockResponse::json(401, r#"{"code":"unauthenticated"}"#)],
        true,
    )
    .await;
    let client = client_for(&server, &FakeTime::new());

    let error = client
        .decisions()
        .search(SearchDecisionsParams::new("Untreue"))
        .await
        .unwrap_err();

    assert!(!format!("{error} {error:?}").contains("klc_your_token_here"));
}
