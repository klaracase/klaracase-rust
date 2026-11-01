//! The short forms of a call: one-argument reads, request builders, paging, waiting, files.

mod common;

use std::sync::Arc;
use std::time::Duration;

use klaracase::{
    Branch, DocumentStatus, Error, Klaracase, Open, SearchDecisionsParams, UploadParams,
};
use serde_json::{Value, json};

use common::{FakeTime, MockResponse, MockServer};

const SEARCH_BODY: &str = include_str!("fixtures/examples/decisions.search.json");
const CITATIONS_BODY: &str = include_str!("fixtures/examples/decisions.citations.json");
const DECISION_BODY: &str = include_str!("fixtures/examples/decisions.show.json");
const NORM_BODY: &str = include_str!("fixtures/examples/norms.show.json");
const CHECK_BODY: &str = include_str!("fixtures/examples/citation-checks.show.json");
const UPLOADED: &str = include_str!("fixtures/conformance/bodies/documents.uploaded.json");
const EXTRACTING: &str = include_str!("fixtures/conformance/bodies/documents.extracting.json");
const READY: &str = include_str!("fixtures/conformance/bodies/documents.ready.json");
const FAILED: &str =
    include_str!("fixtures/conformance/bodies/documents.failed.encrypted_pdf.json");

fn client_for(server: &MockServer, time: &FakeTime) -> Klaracase {
    Klaracase::builder()
        .token("klc_your_token_here")
        .base_url(server.base_url())
        .sleeper(Arc::new(time.clone()))
        .clock(Arc::new(time.clone()))
        .build()
        .expect("client")
}

fn plain_client(server: &MockServer) -> Klaracase {
    client_for(server, &FakeTime::new())
}

/// The search example with a different `results` list.
fn search_page(results: usize) -> String {
    let mut body: Value = serde_json::from_str(SEARCH_BODY).unwrap();
    let row = body["results"][0].clone();
    body["results"] = Value::Array(vec![row; results]);
    body.to_string()
}

fn pages_asked(server: &MockServer) -> Vec<String> {
    server
        .requests()
        .iter()
        .map(|request| {
            request
                .query_pairs()
                .into_iter()
                .find(|(name, _)| name == "page")
                .map_or_else(|| "-".to_owned(), |(_, value)| value)
        })
        .collect()
}

#[tokio::test]
async fn the_search_builder_sends_what_search_with_sends() {
    let server = MockServer::start(vec![MockResponse::json(200, SEARCH_BODY)], true).await;
    let client = plain_client(&server);

    client
        .decisions()
        .search("Untreue")
        .court("BGH")
        .branch(Branch::Ordentlich)
        .limit(5)
        .send()
        .await
        .expect("builder");
    client
        .decisions()
        .search_with(
            SearchDecisionsParams::new("Untreue")
                .court("BGH")
                .branch(Branch::Ordentlich)
                .limit(5),
        )
        .await
        .expect("params");

    let requests = server.requests();
    assert_eq!(requests[0].query_pairs(), requests[1].query_pairs());
    assert_eq!(requests[0].path, "/api/v1/decisions/search");
}

#[tokio::test]
async fn one_argument_reads_send_no_query_and_get_with_sends_the_slice() {
    let server = MockServer::start(
        vec![
            MockResponse::json(200, DECISION_BODY),
            MockResponse::json(200, DECISION_BODY),
            MockResponse::json(200, NORM_BODY),
        ],
        true,
    )
    .await;
    let client = plain_client(&server);

    client.decisions().get("1 StR 185/01").await.expect("get");
    client
        .decisions()
        .get_with(
            "1 StR 185/01",
            klaracase::GetDecisionParams::default().max_chars(4000),
        )
        .await
        .expect("get_with");
    client.norms().get("§ 535 BGB").await.expect("norm");

    let requests = server.requests();
    assert_eq!(requests[0].path, "/api/v1/decisions/1%20StR%20185%2F01");
    assert_eq!(requests[0].raw_query, None);
    assert_eq!(
        requests[1].query_pairs(),
        [("max_chars".to_owned(), "4000".to_owned())]
    );
    assert_eq!(requests[2].path, "/api/v1/norms/%C2%A7%20535%20BGB");
}

#[tokio::test]
async fn a_pager_stops_at_the_first_empty_page_and_does_not_return_it() {
    let server = MockServer::start(
        vec![
            MockResponse::json(200, &search_page(2)),
            MockResponse::json(200, &search_page(1)),
            MockResponse::json(200, &search_page(0)),
        ],
        false,
    )
    .await;
    let client = plain_client(&server);

    let mut pager = client.decisions().search("Untreue").limit(2).pages(5);
    let mut sizes = Vec::new();
    while let Some(page) = pager.next_page().await.expect("page") {
        sizes.push(page.results.len());
        assert_eq!(page.meta.status, 200);
    }

    assert_eq!(sizes, [2, 1]);
    assert_eq!(pages_asked(&server), ["1", "2", "3"]);
    assert_eq!(pager.pages_fetched(), 2);
    assert!(pager.next_page().await.unwrap().is_none());
    assert_eq!(server.requests().len(), 3, "a finished pager sends nothing");
}

#[tokio::test]
async fn max_pages_limits_the_walk_and_page_zero_sends_nothing() {
    let server = MockServer::start(vec![MockResponse::json(200, &search_page(1))], true).await;
    let client = plain_client(&server);

    let mut none = client.decisions().search("Untreue").pages(0);
    assert!(none.next_page().await.unwrap().is_none());
    assert!(server.requests().is_empty());

    let mut pager = client.decisions().search("Untreue").pages(2);
    while pager.next_page().await.unwrap().is_some() {}
    assert_eq!(pages_asked(&server), ["1", "2"]);
}

#[tokio::test]
async fn a_search_walk_is_clamped_to_the_ten_pages_the_server_serves() {
    let server = MockServer::start(vec![MockResponse::json(200, &search_page(1))], true).await;
    let client = plain_client(&server);

    let mut pager = client.decisions().search("Untreue").pages(1_000);
    while pager.next_page().await.unwrap().is_some() {}
    assert_eq!(
        pages_asked(&server),
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]
    );

    // A walk that starts late still stops at the cap.
    let server = MockServer::start(vec![MockResponse::json(200, &search_page(1))], true).await;
    let client = plain_client(&server);
    let mut pager = client.decisions().search("Untreue").page(9).pages(50);
    while pager.next_page().await.unwrap().is_some() {}
    assert_eq!(pages_asked(&server), ["9", "10"]);
}

#[tokio::test]
async fn a_pager_keeps_its_place_after_an_error() {
    let problem = r#"{"type":"about:blank","title":"Unauthorized","status":401,"detail":"no","code":"unauthenticated"}"#;
    let server = MockServer::start(
        vec![
            MockResponse::json(401, problem),
            MockResponse::json(200, &search_page(1)),
            MockResponse::json(200, &search_page(0)),
        ],
        false,
    )
    .await;
    let client = plain_client(&server);

    let mut pager = client.decisions().search("Untreue").pages(3);
    assert!(pager.next_page().await.is_err());
    assert!(
        pager.next_page().await.unwrap().is_some(),
        "same page again"
    );
    assert!(pager.next_page().await.unwrap().is_none());
    assert_eq!(pages_asked(&server), ["1", "1", "2"]);
}

#[tokio::test]
async fn a_bad_query_fails_on_the_first_page_before_any_request() {
    let server = MockServer::start(vec![], true).await;
    let client = plain_client(&server);

    let mut pager = client.decisions().search("  ").pages(3);
    let error = pager.next_page().await.unwrap_err();
    assert!(matches!(error, Error::ClientValidation { .. }));
    assert!(server.requests().is_empty());
}

fn citations_page(has_more: bool) -> String {
    let mut body: Value = serde_json::from_str(CITATIONS_BODY).unwrap();
    body["pagination"]["has_more_references"] = json!(has_more);
    body["pagination"]["has_more_referenced_by"] = json!(false);
    body.to_string()
}

#[tokio::test]
async fn a_citation_walk_stops_when_the_page_says_nothing_follows() {
    let server = MockServer::start(
        vec![
            MockResponse::json(200, &citations_page(true)),
            MockResponse::json(200, &citations_page(false)),
        ],
        false,
    )
    .await;
    let client = plain_client(&server);

    let mut pager = client.decisions().citations("1 StR 185/01").pages(10);
    let mut count = 0;
    while pager.next_page().await.unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 2);
    assert_eq!(pages_asked(&server), ["1", "2"]);
    assert_eq!(
        server.requests()[0].path,
        "/api/v1/decisions/1%20StR%20185%2F01/citations"
    );
}

#[tokio::test]
async fn a_depth_two_citation_walk_is_one_page_and_the_cap_is_twenty() {
    let server =
        MockServer::start(vec![MockResponse::json(200, &citations_page(true))], true).await;
    let client = plain_client(&server);

    let mut pager = client
        .decisions()
        .citations("1 StR 185/01")
        .depth(klaracase::Depth::Two)
        .pages(5);
    while pager.next_page().await.unwrap().is_some() {}
    assert_eq!(server.requests().len(), 1, "page 2 is undefined at depth 2");

    let server =
        MockServer::start(vec![MockResponse::json(200, &citations_page(true))], true).await;
    let client = plain_client(&server);
    let mut pager = client.decisions().citations("1 StR 185/01").pages(500);
    while pager.next_page().await.unwrap().is_some() {}
    assert_eq!(server.requests().len(), 20);
}

#[tokio::test]
async fn wait_until_ready_takes_a_timeout_and_polls_every_two_seconds() {
    let server = MockServer::start(
        vec![
            MockResponse::json(200, UPLOADED),
            MockResponse::json(200, EXTRACTING),
            MockResponse::json(200, READY),
        ],
        false,
    )
    .await;
    let time = FakeTime::new();
    let client = client_for(&server, &time);

    let ready = client
        .citation_check_documents()
        .wait_until_ready(
            "0193b0c6-3c4d-7e5f-8a60-7c8d9e0f1a2b",
            Duration::from_secs(60),
        )
        .await
        .expect("ready");

    assert!(ready.is_ready());
    assert_eq!(
        *time.sleeps.lock().unwrap(),
        [Duration::from_secs(2), Duration::from_secs(2)]
    );
}

fn temp_file(name: &str, content: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("klaracase-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn from_path_guesses_the_name_and_the_media_type() {
    for (name, media) in [
        ("Schriftsatz.PDF", "application/pdf"),
        (
            "brief.docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        ("notes.md", "text/markdown"),
        ("text.txt", "text/plain"),
    ] {
        let path = temp_file(name, b"content");
        let params = UploadParams::from_path(&path).expect("params");
        assert_eq!(params.filename, name);
        assert_eq!(params.content_type, media);
        assert_eq!(params.bytes(), b"content");
    }
}

#[test]
fn from_path_refuses_an_unknown_extension_and_a_missing_file() {
    let path = temp_file("data.bin", b"x");
    let error = UploadParams::from_path(&path).unwrap_err();
    assert!(matches!(error, Error::ClientValidation { .. }), "{error}");

    let error = UploadParams::from_path(path.with_file_name("missing.pdf")).unwrap_err();
    assert!(matches!(error, Error::ClientValidation { .. }), "{error}");
    assert!(error.to_string().contains("cannot read"));
}

#[tokio::test]
async fn check_file_uploads_waits_and_checks_in_that_order() {
    let path = temp_file("schriftsatz.pdf", b"%PDF-1.7 fake body");
    let server = MockServer::start(
        vec![
            MockResponse::json(202, UPLOADED),
            MockResponse::json(200, EXTRACTING),
            MockResponse::json(200, READY),
            MockResponse::json(200, CHECK_BODY),
        ],
        false,
    )
    .await;
    let time = FakeTime::new();
    let client = client_for(&server, &time);

    let run = client
        .citation_checks()
        .check_file(&path)
        .timeout(Duration::from_secs(30))
        .reference_date("2024-05-01")
        .send()
        .await
        .expect("run");

    assert!(!run.findings.is_empty());
    let requests = server.requests();
    let steps: Vec<(&str, &str)> = requests
        .iter()
        .map(|r| (r.method.as_str(), r.path.as_str()))
        .collect();
    let id = "0193b0c6-3c4d-7e5f-8a60-7c8d9e0f1a2b";
    assert_eq!(
        steps,
        [
            ("POST", "/api/v1/citation-check-documents"),
            (
                "GET",
                &format!("/api/v1/citation-check-documents/{id}") as &str
            ),
            (
                "GET",
                &format!("/api/v1/citation-check-documents/{id}") as &str
            ),
            ("POST", "/api/v1/citation-checks"),
        ]
    );
    let body: Value = serde_json::from_slice(&requests[3].body).unwrap();
    assert_eq!(
        body,
        json!({"document_id": id, "reference_date": "2024-05-01"})
    );
    assert!(
        String::from_utf8_lossy(&requests[0].body).contains("%PDF-1.7 fake body"),
        "the file bytes are in the upload"
    );
}

#[tokio::test]
async fn check_file_does_not_check_when_the_document_failed() {
    let path = temp_file("scan.pdf", b"x");
    let server = MockServer::start(
        vec![
            MockResponse::json(202, UPLOADED),
            MockResponse::json(200, FAILED),
        ],
        false,
    )
    .await;
    let client = plain_client(&server);

    let error = client
        .citation_checks()
        .check_file(&path)
        .send()
        .await
        .unwrap_err();

    let Error::DocumentFailed { receipt } = &error else {
        panic!("wrong error: {error}")
    };
    assert_eq!(receipt.status, Some(Open::Known(DocumentStatus::Failed)));
    assert_eq!(error.code(), Some("encrypted_pdf"));
    let paths: Vec<String> = server.requests().iter().map(|r| r.path.clone()).collect();
    assert!(
        !paths.iter().any(|p| p.ends_with("/citation-checks")),
        "{paths:?}"
    );
}

#[tokio::test]
async fn check_file_refuses_a_bad_date_or_file_before_any_request() {
    let path = temp_file("brief.pdf", b"x");
    let server = MockServer::start(vec![], true).await;
    let client = plain_client(&server);

    let error = client
        .citation_checks()
        .check_file(&path)
        .reference_date("May 2024")
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::ClientValidation { .. }), "{error}");

    let error = client
        .citation_checks()
        .check_file(path.with_file_name("gone.pdf"))
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::ClientValidation { .. }), "{error}");
    assert!(server.requests().is_empty());
}
