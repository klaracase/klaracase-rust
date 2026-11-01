//! Cross-type search over statutes and decisions: `GET /search`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::{Klaracase, Request};
use crate::common::{Count, Credits, QueryExpansion, Snippet};
use crate::decisions::DecisionHit;
use crate::encode::Query;
use crate::error::Error;
use crate::open::{Open, response_struct};
use crate::params::{check_date, check_date_range, option_setters, require_text};
use crate::response::Response;
use crate::types::{ArtifactType, Branch, Confidence, DocumentType, Jurisdiction, Mode, Statute};

/// Parameters of [`Klaracase::search`].
///
/// Start with [`CrossTypeSearchParams::new`] and chain setters. A field you do not set is not sent;
/// `limit(0)` and `bridge(false)` are values and are sent.
///
/// ```
/// use klaracase::{CrossTypeSearchParams, Jurisdiction};
///
/// let params = CrossTypeSearchParams::new("Kündigung Mietvertrag").jurisdiction(Jurisdiction::De);
/// assert!(params.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CrossTypeSearchParams {
    /// German legal-concept phrasing. The server requires 2 to 500 characters. Required.
    pub query: String,
    /// Jurisdiction code (`DE`, `EU` or `XI`). It narrows the laws section only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<Open<Jurisdiction>>,
    /// Court name filter on the decisions section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub court: Option<String>,
    /// The Gerichtsbarkeit of the deciding court.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<Open<Branch>>,
    /// Earliest decision date, `YYYY-MM-DD`, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_from: Option<String>,
    /// Latest decision date, `YYYY-MM-DD`, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_to: Option<String>,
    /// Retrieval mode. The server default is hybrid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<Open<Mode>>,
    /// One decision document type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_type: Option<Open<DocumentType>>,
    /// Decisions whose Normenkette cites this norm, for example `§ 906 BGB`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cited_norm: Option<String>,
    /// Any section of these statutes. An empty list is not sent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cited_statutes: Vec<Open<Statute>>,
    /// Maximum results per section. The server caps it at 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Whether the server may add terms to the query. The server default is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<bool>,
}

impl CrossTypeSearchParams {
    /// Parameters with only the query set.
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            jurisdiction: None,
            court: None,
            branch: None,
            date_from: None,
            date_to: None,
            mode: None,
            document_type: None,
            cited_norm: None,
            cited_statutes: Vec::new(),
            limit: None,
            bridge: None,
        }
    }

    option_setters! {
        direct {
            /// Narrows the laws section to one jurisdiction.
            open(Jurisdiction) jurisdiction,
            /// Sets the court name filter on the decisions section.
            text court,
            /// Sets the Gerichtsbarkeit filter.
            open(Branch) branch,
            /// Sets the earliest decision date, `YYYY-MM-DD`.
            text date_from,
            /// Sets the latest decision date, `YYYY-MM-DD`.
            text date_to,
            /// Sets the retrieval mode.
            open(Mode) mode,
            /// Restricts the decisions section to one document type.
            open(DocumentType) document_type,
            /// Restricts the decisions section to decisions that cite this norm.
            text cited_norm,
            /// Sets the maximum number of results per section.
            plain(u32) limit,
            /// Turns the register-term expansion on or off.
            plain(bool) bridge,
        }
    }

    /// Adds one statute to the `cited_statutes` filter.
    #[must_use]
    pub fn cited_statute(mut self, statute: impl Into<Open<Statute>>) -> Self {
        self.cited_statutes.push(statute.into());
        self
    }

    /// Replaces the `cited_statutes` filter.
    #[must_use]
    pub fn cited_statutes<I>(mut self, statutes: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Open<Statute>>,
    {
        self.cited_statutes = statutes.into_iter().map(Into::into).collect();
        self
    }

    /// Checks the shape of the input without any request: a query that is not blank, dates that are
    /// `YYYY-MM-DD`, and `date_from` not after `date_to`.
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] with a message that names the argument.
    pub fn validate(&self) -> Result<(), Error> {
        require_text("query", &self.query)?;
        check_date("date_from", self.date_from.as_deref())?;
        check_date("date_to", self.date_to.as_deref())?;
        check_date_range(self.date_from.as_deref(), self.date_to.as_deref())
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
        self.checked(params.validate())?;
        self.execute(Request::get("/search", params.to_query()))
            .await
    }
}
