//! The `schemars` feature: schemas of the request types, as tool definitions need them.

#![cfg(feature = "schemars")]

use klaracase::{
    CitationsParams, CoverageParams, CreateCitationCheckParams, CreateDatapackageParams,
    CrossTypeSearchParams, FacetsParams, GetDecisionParams, GetNormParams, SearchDecisionsParams,
};
use schemars::{JsonSchema, schema_for};
use serde_json::{Value, json};

fn schema_of<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).unwrap()
}

#[test]
fn every_params_type_has_an_object_schema() {
    for (name, schema) in [
        (
            "SearchDecisionsParams",
            schema_of::<SearchDecisionsParams>(),
        ),
        ("GetDecisionParams", schema_of::<GetDecisionParams>()),
        ("CitationsParams", schema_of::<CitationsParams>()),
        (
            "CrossTypeSearchParams",
            schema_of::<CrossTypeSearchParams>(),
        ),
        ("GetNormParams", schema_of::<GetNormParams>()),
        (
            "CreateDatapackageParams",
            schema_of::<CreateDatapackageParams>(),
        ),
        ("CoverageParams", schema_of::<CoverageParams>()),
        ("FacetsParams", schema_of::<FacetsParams>()),
        (
            "CreateCitationCheckParams",
            schema_of::<CreateCitationCheckParams>(),
        ),
    ] {
        assert_eq!(schema["type"], "object", "{name}");
        assert_eq!(
            schema["additionalProperties"], false,
            "{name}: unknown keys are refused"
        );
        assert!(schema["properties"].is_object(), "{name}");
    }
}

#[test]
fn the_search_schema_names_the_fields_the_wire_uses() {
    let schema = schema_of::<SearchDecisionsParams>();
    let properties = schema["properties"].as_object().unwrap();
    for field in [
        "query",
        "court",
        "branch",
        "document_type",
        "cited_norm",
        "cited_statutes",
        "date_from",
        "date_to",
        "mode",
        "limit",
        "page",
        "bridge",
    ] {
        assert!(properties.contains_key(field), "missing `{field}`");
    }
    assert_eq!(schema["required"], json!(["query"]));
    assert!(
        properties["query"]["description"]
            .as_str()
            .unwrap()
            .contains("500"),
        "the doc comment becomes the description"
    );
}

#[test]
fn a_closed_list_is_listed_and_comes_from_the_wire_spelling() {
    let schema = schema_of::<SearchDecisionsParams>();
    let branch = &schema["$defs"]["Branch"];
    let text = branch.to_string();
    for value in ["ordentlich", "verwaltung", "arbeit"] {
        assert!(text.contains(value), "`{value}` missing in {text}");
    }
    // `Open<Branch>` shows the same list, not an extra string alternative.
    let reference = schema["properties"]["branch"].to_string();
    assert!(reference.contains("#/$defs/Branch"), "{reference}");
}

#[test]
fn the_facet_type_field_is_called_type() {
    let schema = schema_of::<FacetsParams>();
    assert!(schema["properties"]["type"].is_object(), "{schema}");
    assert!(schema["properties"].get("type_").is_none());
}

#[test]
fn a_citation_check_needs_text_or_a_document_id() {
    let schema = schema_of::<CreateCitationCheckParams>();
    let properties = schema["properties"].as_object().unwrap();
    for field in ["text", "document_id", "reference_date"] {
        assert!(properties.contains_key(field), "missing `{field}`");
    }
}
