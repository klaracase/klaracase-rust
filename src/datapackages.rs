//! Data packages: `POST /datapackages`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::{Klaracase, Request};
use crate::common::Credits;
use crate::decisions::CitationEdge;
use crate::error::Error;
use crate::open::{Open, response_struct};
use crate::params::{invalid, option_setters, require_text};
use crate::response::Response;
use crate::types::{Jurisdiction, TextMode};

/// Parameters of [`Datapackages::create`].
///
/// ```
/// use klaracase::CreateDatapackageParams;
///
/// let params = CreateDatapackageParams::new(["1 StR 185/01", "VIII ZR 271/17"])
///     .include_citations(true)
///     .full_text(false);
/// assert!(params.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(::schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CreateDatapackageParams {
    /// 1 to 10 decision ids (UUID or Aktenzeichen). Order matters: the response budget drops
    /// decisions from the end. Required.
    pub ids: Vec<String>,
    /// Include each decision's outgoing citation edges. The server default is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_citations: Option<bool>,
    /// Serve whole judgments instead of excerpts of about 700 characters. The server default is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_text: Option<bool>,
    /// Per-decision text cap when `full_text` is true, 500 to 200,000. The server default is 60,000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<u32>,
}

/// The most ids one data package takes.
const MAX_IDS: usize = 10;

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
            include_citations: None,
            full_text: None,
            max_chars: None,
        }
    }

    option_setters! {
        direct {
            /// Includes or leaves out each decision's outgoing citation edges.
            plain(bool) include_citations,
            /// Serves whole judgments instead of excerpts.
            plain(bool) full_text,
            /// Caps the text per decision when `full_text` is true.
            plain(u32) max_chars,
        }
    }

    /// Checks the shape of the input without any request: 1 to 10 ids, none of them blank.
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] with a message that names the argument.
    pub fn validate(&self) -> Result<(), Error> {
        if self.ids.is_empty() || self.ids.len() > MAX_IDS {
            return Err(invalid(format!(
                "`ids` must hold 1 to {MAX_IDS} ids, got {}",
                self.ids.len()
            )));
        }
        for id in &self.ids {
            require_text("ids[]", id)?;
        }
        Ok(())
    }

    fn to_body(&self) -> Value {
        serde_json::to_value(self).expect("plain parameter structs always serialize")
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
    /// retried. Repeating the same package in the same credit period costs nothing more.
    ///
    /// ```no_run
    /// use klaracase::{CreateDatapackageParams, Klaracase};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let package = client
    ///     .datapackages()
    ///     .create(
    ///         CreateDatapackageParams::new(["1 StR 185/01", "VIII ZR 271/17"])
    ///             .include_citations(false),
    ///     )
    ///     .await?;
    /// println!("{:?} of {:?} included", package.included, package.requested);
    /// # Ok(()) }
    /// ```
    pub async fn create(
        &self,
        params: CreateDatapackageParams,
    ) -> Result<Response<Datapackage>, Error> {
        self.client.checked(params.validate())?;
        self.client
            .execute(Request::post_json("/datapackages", &params.to_body()))
            .await
    }
}
