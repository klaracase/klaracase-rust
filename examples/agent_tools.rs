//! Tools for an AI agent, built from this crate.
//!
//! This file shows how to turn the request types into tool definitions, how to run a tool call by
//! name, and how to keep the answers small, so a model does not read more than it needs.
//!
//! - `tool_definitions` builds the definitions from the JSON schemas of the parameter types
//!   (feature `schemars`).
//! - `call_tool` runs one call by name and always returns JSON. A failed call becomes
//!   `{"error": {"code", "status", "detail", "retry_after_seconds"}}`, so the model can read the
//!   reason and decide whether to try again. A 402 `insufficient_credits` adds `reason` when the
//!   server gave one. A 402 `token_limit_reached` is never retried by the client and reads the
//!   same way; both mean the model must stop and tell its user. The page links of those two
//!   answers (`top_up_url`, `settings_url`) are for a human admin and never go into the output as
//!   members, so the model has nothing to open.
//! - The answers are cut down. `get_decision` returns at most 4,000 characters of text unless the
//!   model asks for more. `check_citations` returns the verdict counts, the unique citations and
//!   one short entry per finding. A pass-through answer drops `credits.api_budget`: the budget
//!   is money of the organization and is for your program, not for the model. Read it from the
//!   typed response if you need it.
//!
//! Print the definitions:
//!
//! ```sh
//! cargo run --example agent_tools --features schemars
//! ```
//!
//! Run one call (this costs credits and needs a real token):
//!
//! ```sh
//! KLARACASE_API_TOKEN=<token> cargo run --example agent_tools --features schemars -- \
//!     search_decisions '{"query": "Vermögensbetreuungspflicht Untreue", "limit": 3}'
//! ```
//!
//! If you want an MCP server instead, use the hosted one at <https://klaracase.de/mcp>. It offers
//! the same tools without any code of your own.

use klaracase::{
    CitationCheckRun, CreateCitationCheckParams, CrossTypeSearchParams, Decision, Error,
    GetDecisionParams, GetNormParams, Klaracase, SearchDecisionsParams,
};
use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

/// The text a `get_decision` call returns when the model does not set `max_chars`.
const DEFAULT_MAX_CHARS: u32 = 4_000;

/// The JSON schema of `T` as one self-contained object: no `$ref`, no `$schema`, no title, and no
/// type-level doc comment (it is written for Rust readers).
fn input_schema<T: JsonSchema>() -> Map<String, Value> {
    let settings = SchemaSettings::draft2020_12().with(|settings| {
        settings.inline_subschemas = true;
        settings.meta_schema = None;
    });
    let schema = settings.into_generator().into_root_schema_for::<T>();
    let Value::Object(mut object) = schema.to_value() else {
        unreachable!("a params struct has an object schema");
    };
    object.remove("title");
    object.remove("description");
    object
}

/// One tool definition in the shape most model APIs take.
fn tool(name: &str, description: &str, schema: Map<String, Value>) -> Value {
    json!({ "name": name, "description": description, "input_schema": schema })
}

/// The schema of `T` with one extra required string property in front.
fn with_required_property<T: JsonSchema>(name: &str, description: &str) -> Map<String, Value> {
    let mut schema = input_schema::<T>();
    let mut properties = Map::new();
    properties.insert(
        name.to_owned(),
        json!({ "type": "string", "description": description }),
    );
    if let Some(Value::Object(existing)) = schema.remove("properties") {
        properties.extend(existing);
    }
    schema.insert("properties".to_owned(), Value::Object(properties));
    let mut required = vec![json!(name)];
    if let Some(Value::Array(existing)) = schema.remove("required") {
        required.extend(existing);
    }
    schema.insert("required".to_owned(), Value::Array(required));
    schema
}

/// The four tool definitions.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        tool(
            "search_decisions",
            "Search German court decisions. Use German legal terms. Costs one credit per distinct query.",
            input_schema::<SearchDecisionsParams>(),
        ),
        tool(
            "get_decision",
            "Read one German court decision by UUID, Aktenzeichen or ECLI. The text is cut to 4000 \
             characters unless you set max_chars.",
            with_required_property::<GetDecisionParams>("id", "UUID, Aktenzeichen or ECLI"),
        ),
        tool(
            "get_norm",
            "Read the current text of one German statute norm, for example \"§ 535 BGB\".",
            with_required_property::<GetNormParams>("reference", "Citation such as \"§ 535 BGB\""),
        ),
        tool(
            "check_citations",
            "Check whether the citations in a text exist in the corpus. Give text or document_id. \
             A not_found verdict means not found in the corpus, not that the citation is invented.",
            input_schema::<CreateCitationCheckParams>(),
        ),
        tool(
            "cross_type_search",
            "Search statutes and decisions together.",
            input_schema::<CrossTypeSearchParams>(),
        ),
    ]
}

/// Runs one tool call and returns JSON the model can read. It never panics and never returns a
/// Rust error: a failure is `{"error": {...}}`.
pub async fn call_tool(client: &Klaracase, name: &str, arguments: Value) -> Value {
    match run(client, name, arguments).await {
        Ok(output) => output,
        Err(error) => json!({ "error": error_json(&error) }),
    }
}

/// The error as the model reads it: the four members of `Error`'s JSON form, plus `reason` for a
/// 402 `insufficient_credits` that has one. The `detail` text is the server's own sentence.
fn error_json(error: &Error) -> Value {
    let mut out = serde_json::to_value(error).unwrap_or_else(|_| json!({}));
    if let (
        Error::InsufficientCredits {
            reason: Some(reason),
            ..
        },
        Some(members),
    ) = (error, out.as_object_mut())
    {
        members.insert("reason".to_owned(), json!(reason.as_str()));
    }
    out
}

/// A pass-through answer without the organization's API budget.
fn without_budget(mut answer: Value) -> Value {
    if let Some(credits) = answer.get_mut("credits").and_then(Value::as_object_mut) {
        credits.remove("api_budget");
    }
    answer
}

async fn run(client: &Klaracase, name: &str, arguments: Value) -> Result<Value, Error> {
    match name {
        "search_decisions" => {
            let params: SearchDecisionsParams = parse(arguments)?;
            let found = client.decisions().search_with(params).await?;
            Ok(without_budget(found.raw().clone()))
        }
        "cross_type_search" => {
            let params: CrossTypeSearchParams = parse(arguments)?;
            let found = client.search_with(params).await?;
            Ok(without_budget(found.raw().clone()))
        }
        "get_decision" => {
            let (id, rest) = take_string(arguments, "id")?;
            let mut params: GetDecisionParams = parse(rest)?;
            if params.max_chars.is_none() {
                params = params.max_chars(DEFAULT_MAX_CHARS);
            }
            let decision = client.decisions().get_with(&id, params).await?;
            Ok(slim_decision(&decision))
        }
        "get_norm" => {
            let (reference, rest) = take_string(arguments, "reference")?;
            let params: GetNormParams = parse(rest)?;
            let norm = client.norms().get_with(&reference, params).await?;
            Ok(without_budget(norm.raw().clone()))
        }
        "check_citations" => {
            let params: CreateCitationCheckParams = parse(arguments)?;
            let run = client.citation_checks().create(params).await?;
            Ok(slim_check(&run))
        }
        other => Err(arguments_error(format!("unknown tool `{other}`"))),
    }
}

/// The decision without its metadata block and without the notes meant for other readers.
fn slim_decision(decision: &Decision) -> Value {
    json!({
        "id": decision.id,
        "aktenzeichen": decision.aktenzeichen,
        "court": decision.court,
        "date": decision.date,
        "text": decision.full_text,
        "text_length": decision.text_length,
        "truncated": decision.truncated,
        "fortgeltung_checked": decision.fortgeltung_checked,
    })
}

/// The run reduced to what a model needs to answer: the counts, the unique citations and one short
/// entry per finding.
fn slim_check(run: &CitationCheckRun) -> Value {
    let findings: Vec<Value> = run
        .findings
        .iter()
        .map(|finding| {
            json!({
                "id": finding.id,
                "canonical": finding.canonical,
                "verdict": finding.verdict,
                "raw_text": finding.raw_text,
                "start_offset": finding.start_offset,
                "end_offset": finding.end_offset,
            })
        })
        .collect();
    json!({
        "id": run.id,
        "status": run.status,
        "verdict_counts": run.verdict_counts,
        "unique_citations": run.unique_citations,
        "findings": findings,
    })
}

/// Reads tool arguments into a params type. A wrong shape or an unknown key is an error the model
/// can read.
fn parse<T: DeserializeOwned>(arguments: Value) -> Result<T, Error> {
    serde_json::from_value(arguments).map_err(|error| arguments_error(error.to_string()))
}

/// Splits a required string argument from the rest of the arguments.
fn take_string(arguments: Value, key: &str) -> Result<(String, Value), Error> {
    let Value::Object(mut object) = arguments else {
        return Err(arguments_error(
            "the arguments must be a JSON object".to_owned(),
        ));
    };
    match object.remove(key) {
        Some(Value::String(text)) => Ok((text, Value::Object(object))),
        _ => Err(arguments_error(format!(
            "`{key}` is required and must be a string"
        ))),
    }
}

/// An error for arguments the model got wrong. It has the same JSON shape as every other error.
fn arguments_error(message: String) -> Error {
    Error::ClientValidation {
        code: "client_validation",
        message,
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let Some(name) = args.next() else {
        println!(
            "{}",
            serde_json::to_string_pretty(&tool_definitions()).unwrap_or_default()
        );
        return Ok(());
    };
    let arguments = match args.next() {
        Some(text) => serde_json::from_str(&text).unwrap_or_else(|_| json!({})),
        None => json!({}),
    };
    let client = Klaracase::from_env()?;
    let output = call_tool(&client, &name, arguments).await;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_default()
    );
    Ok(())
}
