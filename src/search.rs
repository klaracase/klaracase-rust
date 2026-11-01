//! Cross-type search over statutes and decisions: `GET /search`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::{Klaracase, Request};
use crate::common::{Count, Credits};
use crate::decisions::SearchHit;
use crate::encode::Query;
use crate::error::Error;
use crate::response::{Response, impl_extras};
use crate::types::{Branch, DocumentType, Mode, Statute};

/// Parameters of [`Klaracase::search`].
///
/// A `None` field is not sent. `Some(false)` and `Some(0)` are sent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CrossTypeSearchParams {
    /// German legal-concept phrasing, 2 to 500 characters. Required.
    pub query: String,
    /// Jurisdiction code (`DE`, `EU` or `XI`). It narrows the laws section only.
    pub jurisdiction: Option<String>,
    /// Court name filter on the decisions section.
    pub court: Option<String>,
    /// The Gerichtsbarkeit of the deciding court.
    pub branch: Option<Branch>,
    /// Earliest decision date, `YYYY-MM-DD`, inclusive.
    pub date_from: Option<String>,
    /// Latest decision date, `YYYY-MM-DD`, inclusive.
    pub date_to: Option<String>,
    /// Retrieval mode. The server default is hybrid.
    pub mode: Option<Mode>,
    /// One decision document type.
    pub document_type: Option<DocumentType>,
    /// Decisions whose Normenkette cites this norm, for example `§ 906 BGB`.
    pub cited_norm: Option<String>,
    /// Any section of these statutes. An empty list is not sent.
    pub cited_statutes: Vec<Statute>,
    /// Maximum results per section. The server caps it at 10.
    pub limit: Option<u32>,
    /// Whether the server may add terms to the query. The server default is true.
    pub bridge: Option<bool>,
}

impl CrossTypeSearchParams {
    /// Parameters with only the query set.
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Self::default()
        }
    }

    fn to_query(&self) -> Query {
        let mut query = Query::new();
        query.push("query", self.query.as_str());
        query.push_opt("jurisdiction", &self.jurisdiction);
        query.push_opt("court", &self.court);
        query.push_opt("branch", &self.branch);
        query.push_opt("date_from", &self.date_from);
        query.push_opt("date_to", &self.date_to);
        query.push_opt("mode", &self.mode);
        query.push_opt("document_type", &self.document_type);
        query.push_opt("cited_norm", &self.cited_norm);
        query.push_list("cited_statutes", &self.cited_statutes);
        query.push_opt("limit", &self.limit);
        query.push_opt("bridge", &self.bridge);
        query
    }
}

/// The two result sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchSections {
    /// Statute hits.
    #[serde(default)]
    pub laws: Vec<SearchHit>,
    /// Decision hits.
    #[serde(default)]
    pub decisions: Vec<SearchHit>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Totals of one section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionTotals {
    /// How many passages match in this section.
    #[serde(default)]
    pub total_available: Option<Count>,
    /// What `total_available` counts.
    #[serde(default)]
    pub total_available_basis: String,
    /// The size of the candidate pool.
    #[serde(default)]
    pub candidate_pool: u64,
    /// Whether the pool holds every match.
    #[serde(default)]
    pub pool_exhaustive: bool,
    /// Whether more results exist than the section holds.
    #[serde(default)]
    pub truncated: bool,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Totals per section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionTotalsBySection {
    /// Totals of the laws section.
    #[serde(default)]
    pub laws: Option<SectionTotals>,
    /// Totals of the decisions section.
    #[serde(default)]
    pub decisions: Option<SectionTotals>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The answer of [`Klaracase::search`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossTypeSearchResults {
    /// The hits, in two sections.
    pub sections: SearchSections,
    /// Totals per section.
    #[serde(default)]
    pub section_totals: Option<SectionTotalsBySection>,
    /// The limit per section.
    #[serde(default)]
    pub limit: u64,
    /// How many passages match across both sections.
    #[serde(default)]
    pub total_available: Option<Count>,
    /// What `total_available` counts.
    #[serde(default)]
    pub total_available_basis: String,
    /// Whether the pools hold every match.
    #[serde(default)]
    pub pool_exhaustive: bool,
    /// Whether more results exist than the sections hold.
    #[serde(default)]
    pub truncated: bool,
    /// The retrieval mode used.
    #[serde(default)]
    pub mode: String,
    /// What the server added to the query, and why.
    #[serde(default)]
    pub query_expansion: Value,
    /// Decisions that many hits cite.
    #[serde(default)]
    pub frequently_cited_by_results: Vec<Value>,
    /// Norms that many decision hits cite.
    #[serde(default)]
    pub norms_cited_by_decision_results: Vec<Value>,
    /// The confidence of the best hit.
    #[serde(default)]
    pub top_confidence: Option<String>,
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

impl_extras!(
    SearchSections,
    SectionTotals,
    SectionTotalsBySection,
    CrossTypeSearchResults
);

impl Klaracase {
    /// Searches statutes and decisions together. `GET /search`. One credit per distinct query and
    /// filter set per month.
    ///
    /// ```no_run
    /// use klaracase::{CrossTypeSearchParams, Klaracase};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let found = client.search(CrossTypeSearchParams::new("Untreue Vermögensbetreuungspflicht")).await?;
    /// for law in &found.sections.laws {
    ///     println!("{:?}", law.title);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn search(
        &self,
        params: CrossTypeSearchParams,
    ) -> Result<Response<CrossTypeSearchResults>, Error> {
        self.execute(Request::get("/search", params.to_query()))
            .await
    }
}
