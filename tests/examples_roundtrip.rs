//! Drift guards for the typed response model.
//!
//! The twelve example bodies and the response blocks of `CONTRACT.md` are the only real samples of
//! the API. These tests make a change to the model, or a new key in the API, fail loudly.

use klaracase::{
    CitationCheckRun, CitationGraph, CorpusCoverage, CrossTypeSearchResults, Datapackage, Decision,
    DecisionSearchResults, DocumentReceipt, Extras, Facets, Norm, Open, RunStatus, Verdict,
    WireValue,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const EXAMPLES: [&str; 12] = [
    "citation-check-documents.show",
    "citation-check-documents.store",
    "citation-checks.show",
    "citation-checks.store",
    "corpus.coverage",
    "corpus.facets",
    "datapackages.store",
    "decisions.citations",
    "decisions.search",
    "decisions.show",
    "norms.show",
    "search",
];

/// `## METHOD path` headings of `CONTRACT.md` and the example each response block follows.
const CONTRACT_SECTIONS: [(&str, &str); 12] = [
    ("POST /citation-checks", "citation-checks.store"),
    ("GET /citation-checks/{run}", "citation-checks.show"),
    (
        "POST /citation-check-documents",
        "citation-check-documents.store",
    ),
    (
        "GET /citation-check-documents/{document}",
        "citation-check-documents.show",
    ),
    ("GET /decisions/{id}/citations", "decisions.citations"),
    ("GET /corpus/coverage", "corpus.coverage"),
    ("GET /corpus/facets/{field}", "corpus.facets"),
    ("POST /datapackages", "datapackages.store"),
    ("GET /decisions/{id}", "decisions.show"),
    ("GET /norms/{reference}", "norms.show"),
    ("GET /decisions/search", "decisions.search"),
    ("GET /search", "search"),
];

fn example(name: &str) -> Value {
    let path = format!(
        "{}/tests/fixtures/examples/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}")))
        .expect("example is JSON")
}

/// The first JSON block after `### Responses` of every operation in `CONTRACT.md`.
fn contract_blocks() -> Vec<(String, Value)> {
    let path = format!(
        "{}/tests/fixtures/contract/CONTRACT.md",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).expect("CONTRACT.md fixture");
    let mut blocks = Vec::new();
    for section in text.split("\n## ") {
        let heading = section.lines().next().unwrap_or_default().trim();
        let Some((_, example_name)) = CONTRACT_SECTIONS.iter().find(|(h, _)| *h == heading) else {
            continue;
        };
        let responses = section
            .split("### Responses")
            .nth(1)
            .expect("a Responses part");
        let block = responses
            .split("```json\n")
            .nth(1)
            .and_then(|rest| rest.split("\n```").next())
            .expect("a JSON block");
        blocks.push((
            (*example_name).to_owned(),
            serde_json::from_str(block).expect("block is JSON"),
        ));
    }
    blocks
}

/// Removes `null` members at every depth. A `None` field is not written, so `null` and absent are
/// one thing in a comparison.
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

/// The null members a statute row carries for decision-only keys. They are an allow-listed
/// exception: the API writes them, the model does not declare them.
fn without_law_row_nulls(mut body: Value) -> Value {
    if let Some(Value::Array(rows)) = body.pointer_mut("/sections/laws") {
        for row in rows {
            if let Value::Object(members) = row {
                members.retain(|_, value| !value.is_null());
            }
        }
    }
    body
}

/// Calls `$body!(Type)` with the response type of an example.
macro_rules! for_each_example {
    ($name:expr, $callback:ident) => {
        match $name {
            "citation-check-documents.show" | "citation-check-documents.store" => {
                $callback!(DocumentReceipt)
            }
            "citation-checks.show" | "citation-checks.store" => $callback!(CitationCheckRun),
            "corpus.coverage" => $callback!(CorpusCoverage),
            "corpus.facets" => $callback!(Facets),
            "datapackages.store" => $callback!(Datapackage),
            "decisions.citations" => $callback!(CitationGraph),
            "decisions.search" => $callback!(DecisionSearchResults),
            "decisions.show" => $callback!(Decision),
            "norms.show" => $callback!(Norm),
            "search" => $callback!(CrossTypeSearchResults),
            other => panic!("no type for example {other}"),
        }
    };
}

fn round_trip<T: DeserializeOwned + Serialize>(raw: &Value) -> Result<Value, String> {
    let typed: T = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    serde_json::to_value(&typed).map_err(|e| e.to_string())
}

/// True when some struct holds a key it does not declare (the key landed in an `extra` map).
fn has_undeclared_key<T: DeserializeOwned + std::fmt::Debug>(raw: &Value) -> bool {
    let typed: T = serde_json::from_value(raw.clone()).expect("the body decodes");
    // Pretty Debug writes an empty map as `extra: {},` and a map with entries over several lines.
    format!("{typed:#?}").contains("extra: {\n")
}

fn undeclared(name: &str, raw: &Value) -> bool {
    macro_rules! check {
        ($t:ty) => {
            has_undeclared_key::<$t>(raw)
        };
    }
    for_each_example!(name, check)
}

#[test]
fn every_example_round_trips_with_nulls_stripped() {
    for name in EXAMPLES {
        let raw = example(name);
        macro_rules! check {
            ($t:ty) => {
                round_trip::<$t>(&raw)
            };
        }
        let written = for_each_example!(name, check).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(strip_nulls(&written), strip_nulls(&raw), "{name}");
    }
}

#[test]
fn every_key_of_every_example_is_declared() {
    for name in EXAMPLES {
        let raw = without_law_row_nulls(example(name));
        assert!(!undeclared(name, &raw), "{name} holds an undeclared key");
    }
}

#[test]
fn every_key_of_the_contract_response_blocks_is_declared() {
    let blocks = contract_blocks();
    assert_eq!(blocks.len(), 12);
    for (name, raw) in blocks {
        let raw = without_law_row_nulls(raw);
        assert!(
            !undeclared(&name, &raw),
            "the CONTRACT.md block for {name} holds an undeclared key"
        );
        // The block must also read and write back like the example does.
        macro_rules! check {
            ($t:ty) => {
                round_trip::<$t>(&raw)
            };
        }
        let written = for_each_example!(name.as_str(), check).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(strip_nulls(&written), strip_nulls(&raw), "{name}");
    }
}

#[test]
fn the_guard_fails_on_a_nested_unknown_key() {
    // A guard counts only once it has been seen to fail.
    let mut raw = example("citation-checks.show");
    raw["findings"][0]["evidence"]["surprise"] = json!("a key from a later release");
    assert!(undeclared("citation-checks.show", &raw));

    let mut raw = example("decisions.search");
    raw["results"][0]["snippets"][0]["surprise"] = json!(1);
    assert!(undeclared("decisions.search", &raw));

    let mut raw = example("corpus.coverage");
    raw["branches"][0]["instance_mix"]["federal"]["surprise"] = json!(true);
    assert!(undeclared("corpus.coverage", &raw));

    // And a top-level unknown key stays readable: nothing is lost.
    let mut raw = example("norms.show");
    raw["future_key"] = json!({"nested": [1, 2]});
    let norm: Norm = serde_json::from_value(raw).unwrap();
    assert_eq!(norm.extra()["future_key"]["nested"][1], json!(2));
}

/// Paths of every value in a JSON tree, as pointers.
fn pointers(value: &Value, here: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next = format!("{here}/{}", key.replace('~', "~0").replace('/', "~1"));
                out.push(next.clone());
                pointers(child, &next, out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                let next = format!("{here}/{index}");
                out.push(next.clone());
                pointers(child, &next, out);
            }
        }
        _ => {}
    }
}

/// Keys that identify an object. The model requires them, so a `null` there is a real error.
fn is_identity(pointer: &str) -> bool {
    let key = pointer.rsplit('/').next().unwrap_or_default();
    // An entry of a keyed count map (`verdict_counts`, facet `values`): the entry is the count.
    let depth = pointer.matches('/').count();
    if depth == 2 && (pointer.starts_with("/verdict_counts/") || pointer.starts_with("/values/")) {
        return true;
    }
    let numeric = key.chars().all(|c| c.is_ascii_digit());
    if numeric {
        // An array element: its parent decides. A null element cannot read as a struct.
        return true;
    }
    matches!(
        key,
        "id" | "reference"
            | "field"
            | "value"
            | "sections"
            | "source"
            | "artifact_id"
            | "reference_canonical"
    ) || (key == "court" && (pointer.contains("/courts/") || pointer.contains("/top_courts/")))
        || (key == "branch" && pointer.contains("/branches/"))
}

#[test]
fn a_null_in_place_of_any_value_still_decodes() {
    for name in EXAMPLES {
        let raw = example(name);
        let mut all = Vec::new();
        pointers(&raw, "", &mut all);
        for pointer in all {
            if is_identity(&pointer) {
                continue;
            }
            let mut changed = raw.clone();
            *changed.pointer_mut(&pointer).expect("pointer") = Value::Null;
            macro_rules! check {
                ($t:ty) => {
                    round_trip::<$t>(&changed)
                };
            }
            for_each_example!(name, check)
                .unwrap_or_else(|e| panic!("{name}: null at {pointer} fails: {e}"));
        }
    }
}

#[test]
fn a_missing_list_reads_as_empty_and_a_missing_flag_stays_none() {
    let decision: Decision = serde_json::from_value(json!({"id": "x"})).unwrap();
    assert!(decision.available_sections.is_empty());
    assert_eq!(decision.truncated, None, "no fabricated default");
    assert_eq!(decision.full_text, None);

    let page: DecisionSearchResults = serde_json::from_value(json!({"results": null})).unwrap();
    assert!(page.results.is_empty());
    assert_eq!(page.truncated, None);
}

#[test]
fn count_is_exact_only_when_the_server_says_so() {
    let facets: Facets = serde_json::from_value(example("corpus.facets")).unwrap();
    assert!(facets.values.get("BGH").unwrap().is_exact());
    let floor: klaracase::Count = serde_json::from_value(json!({"value": 5})).unwrap();
    assert!(!floor.is_exact());
    let floor: klaracase::Count =
        serde_json::from_value(json!({"value": 5, "is_floor": true})).unwrap();
    assert!(!floor.is_exact());
}

#[test]
fn facet_values_keep_the_server_order() {
    let facets: Facets = serde_json::from_value(example("corpus.facets")).unwrap();
    let order: Vec<&str> = facets.values.iter().map(|(name, _)| name).collect();
    assert_eq!(order, ["BGH", "BVerfG", "BVerwG"]);
    assert_eq!(facets.values.get("BVerwG").unwrap().value, 15000);
    assert!(facets.values.get("nope").is_none());
}

#[test]
fn an_unknown_enum_value_survives_a_round_trip_byte_for_byte() {
    let mut raw = example("citation-checks.show");
    raw["status"] = json!("quarantined");
    raw["findings"][0]["verdict"] = json!("quote_mismatch");
    raw["unique_citations"][0]["verdict"] = json!("Not_Found");
    raw["verdict_counts"]["quote_mismatch"] = json!(2);

    let run: CitationCheckRun = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(run.status, Some(Open::Other("quarantined".to_owned())));
    assert_eq!(
        run.findings[0].verdict,
        Some(Open::Other("quote_mismatch".to_owned()))
    );
    assert_eq!(
        run.unique_citations[0].verdict.as_ref().map(Open::as_str),
        Some("Not_Found"),
        "the match is exact: a different case is another spelling"
    );
    let written = serde_json::to_value(&run).unwrap();
    assert_eq!(written["status"], raw["status"]);
    assert_eq!(
        written["findings"][0]["verdict"],
        raw["findings"][0]["verdict"]
    );
    assert_eq!(
        written["unique_citations"][0]["verdict"],
        raw["unique_citations"][0]["verdict"]
    );
    assert_eq!(run.verdict_counts["quote_mismatch"], 2);
}

#[test]
fn known_enum_values_read_as_known_and_write_back_unchanged() {
    let run: CitationCheckRun = serde_json::from_value(example("citation-checks.show")).unwrap();
    assert_eq!(run.status, Some(Open::Known(RunStatus::Complete)));
    assert!(run.findings.iter().all(|f| f.verdict.is_some()));
    for finding in &run.findings {
        let verdict = finding.verdict.as_ref().unwrap();
        assert!(verdict.known().is_some(), "{verdict} should be known");
        assert_eq!(
            serde_json::to_value(verdict).unwrap(),
            json!(verdict.as_str())
        );
    }
}

#[test]
fn every_wire_value_of_the_main_vocabularies_round_trips() {
    for verdict in [
        Verdict::VerifiedCorpus,
        Verdict::VerifiedExternal,
        Verdict::NotFound,
        Verdict::OutOfCoverage,
        Verdict::Ambiguous,
        Verdict::Unverifiable,
        Verdict::Discrepancy,
    ] {
        assert_eq!(Verdict::from_wire(verdict.as_str()), Some(verdict));
        assert_eq!(verdict.as_str().parse::<Verdict>(), Ok(verdict));
    }
    for status in [
        RunStatus::Pending,
        RunStatus::Gating,
        RunStatus::Extracting,
        RunStatus::Resolving,
        RunStatus::CheckingExternal,
        RunStatus::Complete,
        RunStatus::Rejected,
        RunStatus::Failed,
    ] {
        assert_eq!(RunStatus::from_wire(status.as_str()), Some(status));
    }
}

#[test]
fn findings_group_by_verdict_and_unknown_verdicts_get_their_own_group() {
    let mut raw = example("citation-checks.show");
    let first_verdict = raw["findings"][0]["verdict"].clone();
    raw["findings"][0]["verdict"] = json!("quote_mismatch");
    let run: CitationCheckRun = serde_json::from_value(raw).unwrap();
    let groups = run.findings_by_verdict();
    assert_eq!(
        groups[&Open::Other("quote_mismatch".to_owned())].len(),
        1,
        "{first_verdict}"
    );
    let total: usize = groups.values().map(Vec::len).sum();
    assert_eq!(total, run.findings.len());
}

#[test]
fn a_finding_excerpt_counts_code_points_not_bytes() {
    let run: CitationCheckRun = serde_json::from_value(example("citation-checks.show")).unwrap();
    let finding = &run.findings[0];
    let start = finding.start_offset.unwrap() as usize;
    let length = (finding.end_offset.unwrap() - finding.start_offset.unwrap()) as usize;
    // Put multi-byte characters before and inside the span: a byte slice would cut a character or
    // panic.
    let prefix: String = "äöü§".chars().cycle().take(start).collect();
    let span: String = "ß€Az.".chars().cycle().take(length).collect();
    let text = format!("{prefix}{span} und mehr");
    assert_eq!(finding.excerpt(&text), Some(span));
    assert_eq!(finding.excerpt(""), None);
}
