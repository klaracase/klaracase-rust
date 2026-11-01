//! What a caller can ask of an error: retry, wait, status, code, headers, typed extras, JSON.

mod common;

use std::sync::Arc;
use std::time::Duration;

use klaracase::{Error, Klaracase};
use serde_json::{Value, json};

use common::{FakeTime, MockResponse, MockServer};

const SUPERSEDED: &str = include_str!("fixtures/conformance/problems/statute_superseded.json");
const AMBIGUOUS: &str = include_str!("fixtures/conformance/problems/ambiguous_aktenzeichen.json");
const TOKEN_LIMIT: &str = include_str!("fixtures/conformance/problems/token_limit_reached.json");
const BUDGET_EMPTY: &str =
    include_str!("fixtures/conformance/problems/insufficient_credits.api_budget_empty.json");
const VOLUME_CAP: &str = include_str!("fixtures/conformance/problems/volume_cap.json");

fn problem(status: u16, code: &str, extra: Value) -> String {
    let mut body = json!({
        "type": format!("https://klaracase.de/docs/api/errors#{code}"),
        "title": "Problem",
        "status": status,
        "detail": format!("detail of {code}"),
        "code": code,
    });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    body.to_string()
}

/// A client that never retries, so the first error answer is the one returned.
async fn failure(response: MockResponse) -> Error {
    let server = MockServer::start(vec![response], true).await;
    let time = FakeTime::new();
    let client = Klaracase::builder()
        .token("klc_your_token_here")
        .base_url(server.base_url())
        .max_retries(0)
        .sleeper(Arc::new(time.clone()))
        .clock(Arc::new(time))
        .build()
        .unwrap();
    client.norms().get("§ 1 BGB").await.unwrap_err()
}

#[tokio::test]
async fn only_transient_errors_are_retryable() {
    let cases = [
        (401, "unauthenticated", false),
        (402, "insufficient_credits", false),
        (402, "token_limit_reached", false),
        (403, "token_ability_missing", false),
        (404, "not_found", false),
        (409, "document_unavailable", false),
        (422, "validation_failed", false),
        (429, "rate_limited", true),
        (503, "service_unavailable", true),
        (500, "server_error", false),
    ];
    for (status, code, retryable) in cases {
        let error = failure(MockResponse::json(
            status,
            &problem(status, code, json!({})),
        ))
        .await;
        assert_eq!(error.is_retryable(), retryable, "{status} {code}");
        assert_eq!(error.status(), Some(status));
        assert_eq!(error.code(), Some(code));
    }

    let dropped = failure(MockResponse::dropped()).await;
    assert!(dropped.is_retryable(), "{dropped}");
    assert_eq!(dropped.status(), None);
    assert!(dropped.headers().is_none());

    let client_side = Klaracase::new("").unwrap_err();
    assert!(!client_side.is_retryable());
    let client_side = Klaracase::builder()
        .token("klc_your_token_here")
        .build()
        .unwrap()
        .decisions()
        .search_with(klaracase::SearchDecisionsParams::new(" "))
        .await
        .unwrap_err();
    assert!(!client_side.is_retryable());
}

#[tokio::test]
async fn retry_after_reads_the_body_first_then_the_header() {
    let in_body = failure(MockResponse::json(
        429,
        &problem(429, "rate_limited", json!({"retry_after_seconds": 7})),
    ))
    .await;
    assert_eq!(in_body.retry_after(), Some(Duration::from_secs(7)));

    let mut with_header = MockResponse::json(503, &problem(503, "service_unavailable", json!({})));
    with_header
        .headers
        .push(("retry-after".to_owned(), "12".to_owned()));
    let in_header = failure(with_header).await;
    assert_eq!(in_header.retry_after(), Some(Duration::from_secs(12)));

    let mut both = MockResponse::json(
        429,
        &problem(429, "rate_limited", json!({"retry_after_seconds": 3})),
    );
    both.headers
        .push(("retry-after".to_owned(), "99".to_owned()));
    assert_eq!(
        failure(both).await.retry_after(),
        Some(Duration::from_secs(3))
    );

    let none = failure(MockResponse::json(
        404,
        &problem(404, "not_found", json!({})),
    ))
    .await;
    assert_eq!(none.retry_after(), None);
}

#[tokio::test]
async fn headers_come_from_the_answer_and_never_hold_a_credential() {
    let mut response = MockResponse::json(404, &problem(404, "not_found", json!({})));
    response.headers.extend([
        ("x-request-id".to_owned(), "req-123".to_owned()),
        ("authorization".to_owned(), "Bearer klc_leaked".to_owned()),
        ("set-cookie".to_owned(), "session=abc".to_owned()),
    ]);
    let error = failure(response).await;

    let headers = error.headers().expect("headers");
    assert_eq!(headers.get("x-request-id").unwrap(), "req-123");
    assert_eq!(headers.get("x-klaracase-api-release").unwrap(), "0.95.0");
    assert!(headers.get("authorization").is_none());
    assert!(headers.get("set-cookie").is_none());
    assert!(!format!("{error:?}").contains("klc_leaked"));
}

#[tokio::test]
async fn typed_problem_members_are_read_and_stay_in_extra() {
    let error = failure(MockResponse::json(404, SUPERSEDED)).await;
    let problem = error.problem().unwrap();
    assert_eq!(problem.successor.as_deref(), Some("BauGB"));
    assert_eq!(problem.extra["successor"], "BauGB");
    assert!(problem.candidates.is_none() && problem.cap.is_none());

    let error = failure(MockResponse::json(422, AMBIGUOUS)).await;
    let candidates = error.problem().unwrap().candidates.as_ref().unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].court.as_deref(), Some("OLG Hamm"));
    assert_eq!(
        candidates[1].id.as_deref(),
        Some("0193b0c7-1a2b-7c3d-8e4f-5a6b7c8d9e02")
    );
    assert_eq!(candidates[1].aktenzeichen.as_deref(), Some("5 U 123/19"));

    let error = failure(MockResponse::json(429, VOLUME_CAP)).await;
    let problem = error.problem().unwrap();
    assert_eq!(problem.unit.as_deref(), Some("rows"));
    assert_eq!(problem.cap, Some(1000));
    assert_eq!(
        problem.resets_at.as_deref(),
        Some("2026-11-02T00:00:00+01:00")
    );
    assert_eq!(error.retry_after(), Some(Duration::from_secs(41230)));
}

#[tokio::test]
async fn a_typed_member_of_the_wrong_type_is_none_not_a_failure() {
    let body = problem(
        429,
        "volume_cap",
        json!({"cap": "many", "unit": 3, "candidates": "none", "successor": ["x"]}),
    );
    let error = failure(MockResponse::json(429, &body)).await;
    let problem = error.problem().unwrap();
    assert!(problem.cap.is_none() && problem.unit.is_none());
    assert!(problem.candidates.is_none() && problem.successor.is_none());
    assert_eq!(problem.extra["cap"], "many");
}

#[tokio::test]
async fn an_error_serializes_to_four_members() {
    let rate = failure(MockResponse::json(429, VOLUME_CAP)).await;
    assert_eq!(
        serde_json::to_value(&rate).unwrap(),
        json!({
            "code": "volume_cap",
            "status": 429,
            "detail": rate.problem().unwrap().detail,
            "retry_after_seconds": 41230,
        })
    );

    let plain = failure(MockResponse::json(
        404,
        &problem(404, "not_found", json!({})),
    ))
    .await;
    assert_eq!(
        serde_json::to_value(&plain).unwrap(),
        json!({"code": "not_found", "status": 404, "detail": "detail of not_found", "retry_after_seconds": null})
    );

    let dropped = serde_json::to_value(failure(MockResponse::dropped()).await).unwrap();
    assert_eq!(dropped["code"], "transport_error");
    assert_eq!(dropped["status"], Value::Null);
    assert!(dropped["detail"].as_str().unwrap().contains("transport"));

    let setup = serde_json::to_value(Klaracase::new("").unwrap_err()).unwrap();
    assert_eq!(setup["code"], "missing_token");
    assert_eq!(setup.as_object().unwrap().len(), 4);
}

#[tokio::test]
async fn a_402_refusal_is_never_retried_even_with_retries_on_and_a_retry_after_header() {
    for body in [TOKEN_LIMIT, BUDGET_EMPTY] {
        let mut refusal = MockResponse::json(402, body);
        refusal
            .headers
            .push(("retry-after".to_owned(), "1".to_owned()));
        let server = MockServer::start(vec![refusal], true).await;
        let time = FakeTime::new();
        let client = Klaracase::builder()
            .token("klc_your_token_here")
            .base_url(server.base_url())
            .max_retries(5)
            .sleeper(Arc::new(time.clone()))
            .clock(Arc::new(time))
            .build()
            .unwrap();

        let error = client.norms().get("§ 1 BGB").await.unwrap_err();

        assert_eq!(server.requests().len(), 1, "{error}");
        assert!(!error.is_retryable());
        assert_eq!(error.status(), Some(402));
    }
}

#[tokio::test]
async fn the_two_402_refusals_read_with_their_own_typed_members() {
    let limit = failure(MockResponse::json(402, TOKEN_LIMIT)).await;
    let Error::TokenLimitReached {
        cap,
        spent,
        settings_url,
        ..
    } = &limit
    else {
        panic!("{limit:?}");
    };
    assert_eq!((*cap, *spent), (Some(500), Some(500)));
    assert!(settings_url.is_some());
    assert_eq!(limit.problem().unwrap().cap, Some(500));

    let empty = failure(MockResponse::json(402, BUDGET_EMPTY)).await;
    let Error::InsufficientCredits {
        reason, top_up_url, ..
    } = &empty
    else {
        panic!("{empty:?}");
    };
    assert_eq!(
        *reason,
        Some(klaracase::InsufficientCreditsReason::ApiBudgetEmpty.into())
    );
    assert!(top_up_url.is_some());
}
