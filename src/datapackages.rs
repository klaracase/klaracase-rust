//! Data packages: `POST /datapackages`.

use serde_json::{Map, Value, json};

use crate::client::{Klaracase, Request};
use crate::common::Credits;
use crate::decisions::CitationEdge;
use crate::error::Error;
use crate::open::{Open, response_struct};
use crate::response::Response;
use crate::types::{Jurisdiction, TextMode};

/// Parameters of [`Datapackages::create`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateDatapackageParams {
    /// 1 to 10 decision ids (UUID or Aktenzeichen). Order matters: the response budget drops
    /// decisions from the end. Required.
    pub ids: Vec<String>,
    /// Include each decision's outgoing citation edges. The server default is true.
    pub include_citations: Option<bool>,
    /// Serve whole judgments instead of excerpts of about 700 characters. The server default is false.
    pub full_text: Option<bool>,
    /// Per-decision text cap when `full_text` is true, 500 to 200,000. The server default is 60,000.
    pub max_chars: Option<u32>,
}

impl CreateDatapackageParams {
    /// Parameters with only the ids set.
    #[must_use]
    pub fn new<I, S>(ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            ids: ids.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    fn to_body(&self) -> Value {
        let mut body = Map::new();
        body.insert("ids".to_owned(), json!(self.ids));
        if let Some(value) = self.include_citations {
            body.insert("include_citations".to_owned(), json!(value));
        }
        if let Some(value) = self.full_text {
            body.insert("full_text".to_owned(), json!(value));
        }
        if let Some(value) = self.max_chars {
            body.insert("max_chars".to_owned(), json!(value));
        }
        Value::Object(body)
    }
}

response_struct! {
    /// One decision in a data package.
    PackageDecision {
        /// The artifact id.
        req id: String,
        /// The Aktenzeichen.
        opt aktenzeichen: String,
        /// The court short form.
        opt court: String,
        /// The decision date.
        opt date: String,
        /// The document type.
        opt document_type: String,
        /// The ECLI.
        opt ecli: String,
        /// The jurisdiction code.
        opt jurisdiction: Open<Jurisdiction>,
        /// The text: an excerpt, or the full text when `full_text` was set.
        opt excerpt: String,
        /// Which part of the decision `excerpt` comes from, for example `document_start`.
        opt excerpt_span: String,
        /// The length of the whole text in characters.
        opt text_length: u64,
        /// True when `excerpt` is shorter than the decision.
        opt truncated: bool,
        /// Whether the later history was checked. It is never promised.
        opt fortgeltung_checked: bool,
        /// The German note about Fortgeltung.
        opt fortgeltung_note: String,
    }
}

response_struct! {
    /// How many edges of one decision the package holds and how many exist.
    EdgeAccounting {
        /// The decision the edges start at.
        opt from_artifact_id: String,
        /// What the counts count, for example `edge_rows`.
        opt unit: String,
        /// All edges.
        opt edges_total: u64,
        /// Edges in the package.
        opt edges_included: u64,
        /// Resolved edges in the package.
        opt edges_resolved_included: u64,
        /// Unresolved edges in the package.
        opt edges_unresolved_included: u64,
        /// All resolved edges.
        opt edges_resolved_total: u64,
        /// All unresolved edges.
        opt edges_unresolved_total: u64,
        /// Distinct resolved targets in the package.
        opt targets_resolved_included: u64,
        /// All distinct resolved targets.
        opt targets_resolved_total: u64,
    }
}

response_struct! {
    /// The answer of [`Datapackages::create`].
    Datapackage {
        /// The decisions.
        list decisions: PackageDecision,
        /// The citation edges that start at those decisions.
        list citation_edges: CitationEdge,
        /// Per decision: how many edges the package holds and how many exist.
        list edge_accounting: EdgeAccounting,
        /// Cap on resolved targets per source decision.
        opt per_source_resolved_target_cap: u64,
        /// Cap on unresolved edge rows per source decision.
        opt per_source_unresolved_edge_row_cap: u64,
        /// How many ids you asked for.
        opt requested: u64,
        /// How many decisions the package holds.
        opt included: u64,
        /// How many ids were duplicates.
        opt deduped: u64,
        /// Excerpts or full text.
        opt text_mode: Open<TextMode>,
        /// The per-decision text cap.
        opt max_chars: u64,
        /// Whether any decision text was cut.
        opt truncated: bool,
        /// True when the payload was cut to fit the response budget.
        opt payload_truncated: bool,
        /// When the package was assembled.
        opt assembled_at: String,
        /// The API version of the answer.
        opt api_version: String,
        /// Link to the changelog.
        opt changelog_url: String,
        /// What the call cost.
        opt credits: Credits,
    }
}

/// The data package operations. Get it with [`Klaracase::datapackages`].
#[derive(Debug, Clone, Copy)]
pub struct Datapackages<'a> {
    client: &'a Klaracase,
}

impl Klaracase {
    /// The data package operations.
    #[must_use]
    pub fn datapackages(&self) -> Datapackages<'_> {
        Datapackages { client: self }
    }
}

impl Datapackages<'_> {
    /// Bundles up to 10 decisions with their citations. `POST /datapackages`. Five credits.
    ///
    /// A 429 or 503 is retried, because both guarantee that nothing ran. A dropped connection is not
    /// retried. Repeating the same package in the same month costs nothing more.
    ///
    /// ```no_run
    /// use klaracase::{CreateDatapackageParams, Klaracase};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let package = client
    ///     .datapackages()
    ///     .create(CreateDatapackageParams {
    ///         include_citations: Some(false),
    ///         ..CreateDatapackageParams::new(["1 StR 185/01", "VIII ZR 271/17"])
    ///     })
    ///     .await?;
    /// println!("{:?} of {:?} included", package.included, package.requested);
    /// # Ok(()) }
    /// ```
    pub async fn create(
        &self,
        params: CreateDatapackageParams,
    ) -> Result<Response<Datapackage>, Error> {
        self.client
            .execute(Request::post_json("/datapackages", &params.to_body()))
            .await
    }
}
