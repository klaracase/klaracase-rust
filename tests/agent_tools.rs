//! The `agent_tools` example: definitions from the schemas, dispatch by name, slim answers and
//! errors as JSON. The example file is compiled into this test, so what the README shows is tested.

#![cfg(feature = "schemars")]

mod common;

#[allow(dead_code, clippy::duplicate_mod)]
#[path = "../examples/agent_tools.rs"]
mod agent_tools;

use klaracase::Klaracase;
use serde_json::{Value, json};

use common::{MockResponse, MockServer};

const DECISION_BODY: &str = include_str!("fixtures/examples/decisions.show.json");
const CHECK_BODY: &str = include_str!("fixtures/examples/citation-checks.show.json");
const SEARCH_BODY: &str = include_str!("fixtures/examples/decisions.search.json");
const BUDGET_BODY: &str =
    include_str!("fixtures/conformance/bodies/decisions.search.api_budget.json");
const TOKEN_LIMIT: &str = include_str!("fixtures/conformance/problems/token_limit_reached.json");
const BUDGET_EMPTY: &str =
    include_str!("fixtures/conformance/problems/insufficient_credits.api_budget_empty.json");
const BUDGET_NOT_AVAILABLE: &str = include_str!(
    "fixtures/conformance/problems/insufficient_credits.api_budget_not_available.json"
);

fn client_for(server: &MockServer) -> Klaracase {
    Klaracase::builder()
        .token("klc_your_token_here")
        .base_url(server.base_url())
        .build()
        .expect("client")
}

#[test]
fn definitions_are_flat_objects_with_names_and_descriptions() {
    let tools = agent_tools::tool_definitions();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "search_decisions",
            "get_decision",
            "get_norm",
            "check_citations",
            "cross_type_search"
        ]
    );
    for tool in &tools {
        let schema = tool["input_schema"].to_string();
        assert!(
            !schema.contains("$ref") && !schema.contains("$defs"),
            "{schema}"
        );
        assert!(!schema.contains("$schema"));
        assert_eq!(tool["input_schema"]["type"], "object");
        assert!(tool["description"].as_str().unwrap().len() > 20);
    }
    let get = &tools[1]["input_schema"];
    assert_eq!(get["required"], json!(["id"]));
    assert!(get["properties"]["max_chars"].is_object());
    // The closed list of a request enum is in the schema, inline.
    assert!(
        tools[0]["input_schema"]["properties"]["branch"]
            .to_string()
            .contains("ordentlich")
    );
}

#[tokio::test]
async fn get_decision_asks_for_4000_characters_unless_told_otherwise() {
    let server = MockServer::start(vec![MockResponse::json(200, DECISION_BODY)], true).await;
    let client = client_for(&server);

    let out = agent_tools::call_tool(&client, "get_decision", json!({"id": "1 StR 185/01"})).await;
    assert!(out.get("error").is_none(), "{out}");
    assert!(out["text"].is_string() || out["text"].is_null());
    assert!(
        out.get("metadata").is_none(),
        "the metadata block is dropped"
    );

    agent_tools::call_tool(
        &client,
        "get_decision",
        json!({"id": "1 StR 185/01", "max_chars": 100}),
    )
    .await;

    let requests = server.requests();
    assert_eq!(
        requests[0].query_pairs(),
        [("max_chars".to_owned(), "4000".to_owned())]
    );
    assert_eq!(
        requests[1].query_pairs(),
        [("max_chars".to_owned(), "100".to_owned())]
    );
}

#[tokio::test]
async fn check_citations_is_cut_to_counts_and_short_findings() {
    let server = MockServer::start(vec![MockResponse::json(200, CHECK_BODY)], true).await;
    let client = client_for(&server);

    let out = agent_tools::call_tool(
        &client,
        "check_citations",
        json!({"text": "BGH 1 StR 185/01"}),
    )
    .await;

    let keys: Vec<&str> = out
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "id",
            "status",
            "verdict_counts",
            "unique_citations",
            "findings"
        ]
    );
    let findings = out["findings"].as_array().unwrap();
    assert!(!findings.is_empty());
    let finding_keys: Vec<&str> = findings[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        finding_keys,
        [
            "id",
            "canonical",
            "verdict",
            "raw_text",
            "start_offset",
            "end_offset"
        ]
    );
    assert!(
        out.to_string().len() < CHECK_BODY.len(),
        "the slim form is smaller"
    );
}

#[tokio::test]
async fn search_passes_the_answer_through() {
    let server = MockServer::start(vec![MockResponse::json(200, SEARCH_BODY)], true).await;
    let out = agent_tools::call_tool(
        &client_for(&server),
        "search_decisions",
        json!({"query": "Untreue", "branch": "ordentlich", "limit": 3}),
    )
    .await;
    assert_eq!(out, serde_json::from_str::<Value>(SEARCH_BODY).unwrap());
}

#[tokio::test]
async fn failures_become_error_json_with_four_members() {
    let problem = r#"{"type":"about:blank","title":"Too Many Requests","status":429,"detail":"slow down","code":"rate_limited","retry_after_seconds":45}"#;
    let server = MockServer::start(vec![MockResponse::json(429, problem)], true).await;
    let client = client_for(&server);

    let limited =
        agent_tools::call_tool(&client, "get_norm", json!({"reference": "§ 1 BGB"})).await;
    assert_eq!(
        limited,
        json!({"error": {"code": "rate_limited", "status": 429, "detail": "slow down", "retry_after_seconds": 45}})
    );

    for (name, args, needle) in [
        ("nope", json!({}), "unknown tool"),
        (
            "search_decisions",
            json!({"query": "x", "curt": "BGH"}),
            "curt",
        ),
        ("search_decisions", json!({"query": "  "}), "query"),
        ("get_decision", json!({}), "`id`"),
        (
            "check_citations",
            json!({"text": "a", "document_id": "b"}),
            "not both",
        ),
    ] {
        let out = agent_tools::call_tool(&client, name, args).await;
        assert_eq!(out["error"]["code"], "client_validation", "{name}: {out}");
        assert_eq!(out["error"]["status"], Value::Null);
        assert!(
            out["error"]["detail"].as_str().unwrap().contains(needle),
            "{name}: {out}"
        );
    }
    assert_eq!(server.requests().len(), 1, "bad arguments send no request");
}

#[tokio::test]
async fn a_402_refusal_reads_as_error_json_and_is_not_retried() {
    for (body, code, reason) in [
        (TOKEN_LIMIT, "token_limit_reached", None),
        (
            BUDGET_EMPTY,
            "insufficient_credits",
            Some("api_budget_empty"),
        ),
        (
            BUDGET_NOT_AVAILABLE,
            "insufficient_credits",
            Some("api_budget_not_available"),
        ),
    ] {
        let server = MockServer::start(vec![MockResponse::json(402, body)], true).await;
        let out = agent_tools::call_tool(
            &client_for(&server),
            "search_decisions",
            json!({"query": "Untreue"}),
        )
        .await;

        let error = out["error"].as_object().unwrap();
        assert_eq!(error["code"], code, "{out}");
        assert_eq!(error["status"], 402);
        assert_eq!(error["retry_after_seconds"], Value::Null);
        assert_eq!(error.get("reason").and_then(Value::as_str), reason, "{out}");
        for link in ["top_up_url", "settings_url"] {
            assert!(
                !error.contains_key(link),
                "{link} is for a human admin: {out}"
            );
        }
        assert_eq!(server.requests().len(), 1, "a 402 is not retried");
    }
}

#[tokio::test]
async fn a_pass_through_answer_drops_the_api_budget_and_keeps_the_rest_of_credits() {
    let server = MockServer::start(vec![MockResponse::json(200, BUDGET_BODY)], true).await;
    let out = agent_tools::call_tool(
        &client_for(&server),
        "search_decisions",
        json!({"query": "Untreue"}),
    )
    .await;

    let credits = out["credits"].as_object().unwrap();
    assert!(!credits.contains_key("api_budget"), "{out}");
    assert_eq!(credits["cost"], 1);
    assert_eq!(credits["credits_remaining"], 0);
    // The answer is otherwise unchanged.
    let mut wanted: Value = serde_json::from_str(BUDGET_BODY).unwrap();
    wanted["credits"]
        .as_object_mut()
        .unwrap()
        .remove("api_budget");
    assert_eq!(out, wanted);
}
