//! Cross-type search over statutes and decisions: `GET /search`.

use serde_json::Value;

use crate::client::{Klaracase, Request};
use crate::common::{Count, Credits, QueryExpansion, Snippet};
use crate::decisions::DecisionHit;
use crate::encode::Query;
use crate::error::Error;
use crate::open::{Open, response_struct};
use crate::response::Response;
use crate::types::{ArtifactType, Branch, Confidence, DocumentType, Jurisdiction, Mode, Statute};

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

response_struct! {
    /// One statute hit from the laws section of a cross-type search.
    ///
    /// The API also sends the decision-only keys (`aktenzeichen`, `court`, `date`, `document_type`
    /// and a few more) on a law row, always `null`. They are not modelled here; they stay in `extra`.
    LawHit {
        /// The artifact id.
        req id: String,
        /// `LAW`.
        opt artifact_type: Open<ArtifactType>,
        /// A title for display, for example `§ 266 StGB Untreue`.
        opt title: String,
        /// The best passage, with `<mark>` tags.
        opt snippet: String,
        /// The matching passages.
        list snippets: Snippet,
        /// The place in the statute.
        opt breadcrumb: String,
        /// The abbreviation, for example `StGB`.
        opt abbreviation: String,
        /// The law type.
        opt law_type: String,
        /// The jurisdiction code.
        opt jurisdiction: Open<Jurisdiction>,
        /// How sure the ranker is about this hit.
        opt confidence: Open<Confidence>,
    }
}

response_struct! {
    /// The two sections of a cross-type search.
    SearchSections {
        /// Statute hits.
        list laws: LawHit,
        /// Decision hits.
        list decisions: DecisionHit,
    }
}

response_struct! {
    /// Counts for one section of a cross-type search.
    SectionTotals {
        /// How many passages match in this section.
        opt total_available: Count,
        /// What `total_available` counts.
        opt total_available_basis: String,
        /// The size of the candidate pool.
        opt candidate_pool: u64,
        /// Whether the pool holds every match.
        opt pool_exhaustive: bool,
        /// Whether more results exist than the section holds. `None` can also mean the section
        /// never ran.
        opt truncated: bool,
    }
}

response_struct! {
    /// The per-section counts of a cross-type search.
    SectionTotalsBySection {
        /// Totals of the laws section.
        opt laws: SectionTotals,
        /// Totals of the decisions section.
        opt decisions: SectionTotals,
    }
}

response_struct! {
    /// The answer of [`Klaracase::search`].
    CrossTypeSearchResults {
        /// The hits, by section.
        req sections: SearchSections,
        /// Counts per section.
        opt section_totals: SectionTotalsBySection,
        /// The limit applied per section.
        opt limit: u64,
        /// How many passages match over both sections.
        opt total_available: Count,
        /// What `total_available` counts.
        opt total_available_basis: String,
        /// Whether the pools hold every match.
        opt pool_exhaustive: bool,
        /// Whether more results exist than the sections hold.
        opt truncated: bool,
        /// The retrieval mode used.
        opt mode: Open<Mode>,
        /// What the server added to the query, and why.
        opt query_expansion: QueryExpansion,
        /// Decisions that many hits cite. The API has never sent an element, so the element type
        /// stays `Value` until a sample exists.
        list frequently_cited_by_results: Value,
        /// Norms that the decision hits cite. The API has never sent an element, so the element type
        /// stays `Value` until a sample exists.
        list norms_cited_by_decision_results: Value,
        /// The confidence of the best hit.
        opt top_confidence: Open<Confidence>,
        /// The API version of the answer.
        opt api_version: String,
        /// Link to the changelog.
        opt changelog_url: String,
        /// What the call cost.
        opt credits: Credits,
    }
}

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
