//! Data packages: `POST /datapackages`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::client::{Klaracase, Request};
use crate::common::Credits;
use crate::decisions::CitationEdge;
use crate::error::Error;
use crate::response::{Response, impl_extras};

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

/// One decision in a data package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageDecision {
    /// The artifact id.
    #[serde(default)]
    pub id: String,
    /// The Aktenzeichen.
    #[serde(default)]
    pub aktenzeichen: String,
    /// The court short form.
    #[serde(default)]
    pub court: String,
    /// The decision date.
    #[serde(default)]
    pub date: Option<String>,
    /// The document type.
    #[serde(default)]
    pub document_type: Option<String>,
    /// The ECLI.
    #[serde(default)]
    pub ecli: Option<String>,
    /// The jurisdiction code.
    #[serde(default)]
    pub jurisdiction: Option<String>,
    /// The excerpt. With `full_text` the text is under another key, kept in `extra`.
    #[serde(default)]
    pub excerpt: Option<String>,
    /// Where the excerpt comes from, for example `document_start`.
    #[serde(default)]
    pub excerpt_span: Option<String>,
    /// The length of the whole text.
    #[serde(default)]
    pub text_length: u64,
    /// True when the served text is shorter than the decision.
    #[serde(default)]
    pub truncated: bool,
    /// Whether the later history was checked. It is never promised.
    #[serde(default)]
    pub fortgeltung_checked: bool,
    /// The German note about Fortgeltung.
    #[serde(default)]
    pub fortgeltung_note: Option<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The answer of [`Datapackages::create`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Datapackage {
    /// The decisions, in the order you asked.
    #[serde(default)]
    pub decisions: Vec<PackageDecision>,
    /// The outgoing citation edges of those decisions.
    #[serde(default)]
    pub citation_edges: Vec<CitationEdge>,
    /// Per source decision, how many edges exist and how many are included.
    #[serde(default)]
    pub edge_accounting: Vec<Value>,
    /// Cap on resolved targets per source.
    #[serde(default)]
    pub per_source_resolved_target_cap: u64,
    /// Cap on unresolved edge rows per source.
    #[serde(default)]
    pub per_source_unresolved_edge_row_cap: u64,
    /// How many ids you sent.
    #[serde(default)]
    pub requested: u64,
    /// How many decisions the package includes.
    #[serde(default)]
    pub included: u64,
    /// How many duplicate ids collapsed.
    #[serde(default)]
    pub deduped: u64,
    /// `excerpt` or `full_text`.
    #[serde(default)]
    pub text_mode: String,
    /// The per-decision text cap applied.
    #[serde(default)]
    pub max_chars: u64,
    /// Whether any text was cut.
    #[serde(default)]
    pub truncated: bool,
    /// True when the payload was cut to fit the response budget.
    #[serde(default)]
    pub payload_truncated: bool,
    /// When the package was assembled.
    #[serde(default)]
    pub assembled_at: Option<String>,
    /// The API version of the answer.
    #[serde(default)]
    pub api_version: Option<String>,
    /// Link to the changelog.
    #[serde(default)]
    pub changelog_url: Option<String>,
    /// What the call cost.
    #[serde(default)]
    pub credits: Option<Credits>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl_extras!(PackageDecision, Datapackage);

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
    /// println!("{} of {} included", package.included, package.requested);
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
