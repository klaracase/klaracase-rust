//! Client-side validation: input of the wrong shape is refused before any request, and nothing the
//! server owns is enforced.

mod common;

use klaracase::{
    Branch, CitationsParams, CreateCitationCheckParams, CreateDatapackageParams,
    CrossTypeSearchParams, Error, GetDecisionParams, Klaracase, SearchDecisionsParams, Statute,
};
use serde_json::json;

use common::{MockResponse, MockServer};

const SEARCH_BODY: &str = include_str!("fixtures/examples/decisions.search.json");

async fn client_and_server(validation: bool) -> (Klaracase, MockServer) {
    let server = MockServer::start(vec![MockResponse::json(200, SEARCH_BODY)], true).await;
    let client = Klaracase::builder()
        .token("klc_your_token_here")
        .base_url(server.base_url())
        .client_validation(validation)
        .build()
        .expect("client");
    (client, server)
}

fn assert_refused(error: &Error, needle: &str) {
    assert!(
        matches!(error, Error::ClientValidation { .. }),
        "wrong variant: {error:?}"
    );
    assert_eq!(error.code(), Some("client_validation"));
    assert_eq!(error.status(), None);
    assert!(
        error.to_string().contains(needle),
        "`{error}` should name `{needle}`"
    );
}

#[tokio::test]
async fn bad_input_sends_no_request() {
    let (client, server) = client_and_server(true).await;

    let error = client
        .decisions()
        .search(SearchDecisionsParams::new("   "))
        .await
        .unwrap_err();
    assert_refused(&error, "query");

    let error = client
        .search(CrossTypeSearchParams::new(""))
        .await
        .unwrap_err();
    assert_refused(&error, "query");

    for bad in [
        "2020-1-1",
        "01.01.2020",
        "2020-02-30",
        "2020-13-01",
        "tomorrow",
    ] {
        let error = client
            .decisions()
            .search(SearchDecisionsParams::new("Untreue").date_from(bad))
            .await
            .unwrap_err();
        assert_refused(&error, "date_from");
        let error = client
            .decisions()
            .search(SearchDecisionsParams::new("Untreue").date_to(bad))
            .await
            .unwrap_err();
        assert_refused(&error, "date_to");
    }

    let error = client
        .decisions()
        .search(
            SearchDecisionsParams::new("Untreue")
                .date_from("2020-01-02")
                .date_to("2020-01-01"),
        )
        .await
        .unwrap_err();
    assert_refused(&error, "date_from");

    let error = client
        .decisions()
        .get(
            "1 StR 185/01",
            GetDecisionParams::default().rn_from(9).rn_to(3),
        )
        .await
        .unwrap_err();
    assert_refused(&error, "rn_from");

    let error = client
        .decisions()
        .get(
            "1 StR 185/01",
            GetDecisionParams::default().date("1.1.2020"),
        )
        .await
        .unwrap_err();
    assert_refused(&error, "date");

    let error = client
        .decisions()
        .citations("1 StR 185/01", CitationsParams::default().date("soon"))
        .await
        .unwrap_err();
    assert_refused(&error, "date");

    assert!(
        server.requests().is_empty(),
        "no request may leave the client"
    );
}

#[tokio::test]
async fn datapackage_ids_must_be_one_to_ten_and_not_blank() {
    let (client, server) = client_and_server(true).await;

    let none: [&str; 0] = [];
    let error = client
        .datapackages()
        .create(CreateDatapackageParams::new(none))
        .await
        .unwrap_err();
    assert_refused(&error, "ids");

    let eleven: Vec<String> = (1..=11).map(|n| format!("{n} StR 1/01")).collect();
    let error = client
        .datapackages()
        .create(CreateDatapackageParams::new(eleven))
        .await
        .unwrap_err();
    assert_refused(&error, "ids");

    let error = client
        .datapackages()
        .create(CreateDatapackageParams::new(["1 StR 185/01", " "]))
        .await
        .unwrap_err();
    assert_refused(&error, "ids");

    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn a_citation_check_takes_exactly_one_source_and_a_well_formed_date() {
    let (client, server) = client_and_server(true).await;
    let checks = client.citation_checks();

    let both = CreateCitationCheckParams::from_text("a").document_id("b");
    assert_refused(&checks.create(both).await.unwrap_err(), "not both");

    let neither: CreateCitationCheckParams = serde_json::from_value(json!({})).unwrap();
    assert_refused(&checks.create(neither).await.unwrap_err(), "text");

    let blank = CreateCitationCheckParams::from_text("  ");
    assert_refused(&checks.create(blank).await.unwrap_err(), "text");

    let bad_date = CreateCitationCheckParams::from_text("Text").reference_date("1 May 2024");
    assert_refused(
        &checks.create(bad_date).await.unwrap_err(),
        "reference_date",
    );

    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn numbers_and_lengths_the_server_owns_are_sent_as_given() {
    let (client, server) = client_and_server(true).await;

    // A one-character query, a limit and a page far above the caps, and a long query.
    client
        .decisions()
        .search(SearchDecisionsParams::new("x").limit(5_000).page(99))
        .await
        .expect("sent");
    client
        .decisions()
        .search(SearchDecisionsParams::new("y".repeat(600)))
        .await
        .expect("sent");

    let requests = server.requests();
    assert_eq!(
        requests.len(),
        2,
        "the server, not the client, answers a cap"
    );
    let pairs = requests[0].query_pairs();
    assert!(pairs.contains(&("limit".to_owned(), "5000".to_owned())));
    assert!(pairs.contains(&("page".to_owned(), "99".to_owned())));
}

#[tokio::test]
async fn switching_client_validation_off_lets_the_server_judge() {
    let (client, server) = client_and_server(false).await;

    client
        .decisions()
        .search(
            SearchDecisionsParams::new("Untreue")
                .date_from("2020-01-02")
                .date_to("2020-01-01"),
        )
        .await
        .expect("sent");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn closed_lists_accept_typed_values_and_the_servers_own_spelling() {
    let (client, server) = client_and_server(true).await;

    client
        .decisions()
        .search(
            SearchDecisionsParams::new("Untreue")
                .branch(Branch::Arbeit)
                .mode("keyword")
                .cited_statute(Statute::StGB)
                .cited_statute("NewStatute"),
        )
        .await
        .expect("sent");

    // Query order is the order of the setters; the list keeps its own order.
    let pairs = server.requests()[0].query_pairs();
    let pairs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("query", "Untreue"),
            ("branch", "arbeit"),
            ("cited_statutes[]", "StGB"),
            ("cited_statutes[]", "NewStatute"),
            ("mode", "keyword"),
        ]
    );
}

#[test]
fn params_round_trip_through_serde_and_reject_unknown_keys() {
    let params = SearchDecisionsParams::new("Untreue")
        .court("BGH")
        .branch(Branch::Ordentlich)
        .cited_statutes([Statute::StGB, Statute::StPO])
        .limit(0)
        .bridge(false);
    let value = serde_json::to_value(&params).unwrap();
    assert_eq!(
        value,
        json!({
            "query": "Untreue", "court": "BGH", "branch": "ordentlich",
            "cited_statutes": ["StGB", "StPO"], "limit": 0, "bridge": false
        })
    );
    let back: SearchDecisionsParams = serde_json::from_value(value).unwrap();
    assert_eq!(back, params);

    let typo =
        serde_json::from_value::<SearchDecisionsParams>(json!({"query": "x", "curt": "BGH"}));
    assert!(
        typo.is_err(),
        "a misspelled key must not be dropped silently"
    );
    let no_query = serde_json::from_value::<SearchDecisionsParams>(json!({"court": "BGH"}));
    assert!(no_query.is_err());
}
