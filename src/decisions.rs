//! Decisions: search, read and citation graph.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::{Klaracase, Request};
use crate::common::{Count, Credits, Snippet};
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::response::{Response, impl_extras};
use crate::types::{Branch, Depth, Direction, DocumentType, Mode, Statute};

/// Parameters of [`Decisions::search`].
///
/// Build it with a struct literal and `..Default::default()`, or with [`SearchDecisionsParams::new`].
/// A `None` field is not sent. `Some(false)` and `Some(0)` are sent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchDecisionsParams {
    /// German legal-concept phrasing, 2 to 500 characters. Required.
    pub query: String,
    /// Court name, as listed by `corpus().facets(FacetField::CourtName, ..)`.
    pub court: Option<String>,
    /// The Gerichtsbarkeit of the deciding court.
    pub branch: Option<Branch>,
    /// One decision document type.
    pub document_type: Option<DocumentType>,
    /// Decisions whose Normenkette cites this norm, for example `§ 906 BGB`.
    pub cited_norm: Option<String>,
    /// Any section of these statutes. An empty list is not sent.
    pub cited_statutes: Vec<Statute>,
    /// Earliest decision date, `YYYY-MM-DD`, inclusive.
    pub date_from: Option<String>,
    /// Latest decision date, `YYYY-MM-DD`, inclusive.
    pub date_to: Option<String>,
    /// Retrieval mode. The server default is hybrid.
    pub mode: Option<Mode>,
    /// Maximum results. The server caps it at 20.
    pub limit: Option<u32>,
    /// 1-based page, up to 10.
    pub page: Option<u32>,
    /// Whether the server may add curated register terms to the query. The server default is true.
    pub bridge: Option<bool>,
}

impl SearchDecisionsParams {
    /// Parameters with only the query set.
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Self::default()
        }
    }

    pub(crate) fn to_query(&self) -> Query {
        let mut query = Query::new();
        query.push("query", self.query.as_str());
        query.push_opt("court", &self.court);
        query.push_opt("branch", &self.branch);
        query.push_opt("document_type", &self.document_type);
        query.push_opt("cited_norm", &self.cited_norm);
        query.push_list("cited_statutes", &self.cited_statutes);
        query.push_opt("date_from", &self.date_from);
        query.push_opt("date_to", &self.date_to);
        query.push_opt("mode", &self.mode);
        query.push_opt("limit", &self.limit);
        query.push_opt("page", &self.page);
        query.push_opt("bridge", &self.bridge);
        query
    }
}

/// One search hit. Decisions and statutes share this shape; fields that do not apply are `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// The artifact id.
    #[serde(default)]
    pub id: String,
    /// `DECISION` or `LAW`. Absent in a decision-only search.
    #[serde(default)]
    pub artifact_type: Option<String>,
    /// The Aktenzeichen of a decision.
    #[serde(default)]
    pub aktenzeichen: Option<String>,
    /// The court short form.
    #[serde(default)]
    pub court: Option<String>,
    /// The decision date, `YYYY-MM-DD`.
    #[serde(default)]
    pub date: Option<String>,
    /// The document type, for example `Urteil`.
    #[serde(default)]
    pub document_type: Option<String>,
    /// A title for display.
    #[serde(default)]
    pub title: Option<String>,
    /// The best passage, with `<mark>` tags.
    #[serde(default)]
    pub snippet: Option<String>,
    /// The matching passages.
    #[serde(default)]
    pub snippets: Vec<Snippet>,
    /// The title of the matching chunk, for example `Gründe`.
    #[serde(default)]
    pub chunk_title: Option<String>,
    /// The Randnummer range of the matching chunk, for example `21-23`.
    #[serde(default)]
    pub rn_range: Option<String>,
    /// A Fundstelle, when known.
    #[serde(default)]
    pub fundstelle: Option<String>,
    /// Whether the later history of the decision was checked. It is never promised.
    #[serde(default)]
    pub fortgeltung_checked: Option<bool>,
    /// The German note about Fortgeltung.
    #[serde(default)]
    pub fortgeltung_note: Option<String>,
    /// Statutes only: the place in the statute.
    #[serde(default)]
    pub breadcrumb: Option<String>,
    /// Statutes only: the abbreviation, for example `StGB`.
    #[serde(default)]
    pub abbreviation: Option<String>,
    /// Statutes only: the law type.
    #[serde(default)]
    pub law_type: Option<String>,
    /// Statutes only: the jurisdiction code.
    #[serde(default)]
    pub jurisdiction: Option<String>,
    /// `high`, `medium` or `low`.
    #[serde(default)]
    pub confidence: Option<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The answer of [`Decisions::search`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionSearchResults {
    /// The hits, best first.
    #[serde(default)]
    pub results: Vec<SearchHit>,
    /// True when the payload was cut to fit the response budget.
    #[serde(default)]
    pub payload_truncated: bool,
    /// How many passages match.
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
    /// Whether more results exist than this page holds.
    #[serde(default)]
    pub truncated: bool,
    /// The page served.
    #[serde(default)]
    pub page: u64,
    /// The limit applied.
    #[serde(default)]
    pub limit: u64,
    /// The retrieval mode used.
    #[serde(default)]
    pub mode: String,
    /// Decisions that many hits cite.
    #[serde(default)]
    pub frequently_cited_by_results: Vec<Value>,
    /// What the server added to the query, and why.
    #[serde(default)]
    pub query_expansion: Value,
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

/// Parameters of [`Decisions::get`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetDecisionParams {
    /// Maximum characters of text. The server default is 40,000 and the minimum is 500.
    pub max_chars: Option<u32>,
    /// First Randnummer to return.
    pub rn_from: Option<u32>,
    /// Last Randnummer to return, inclusive.
    pub rn_to: Option<u32>,
    /// One named section: `Tenor`, `Leitsatz`, `Tatbestand` or `Gründe`.
    pub section: Option<String>,
    /// Court short form that disambiguates an Aktenzeichen.
    pub court: Option<String>,
    /// Decision date (`YYYY-MM-DD`) that disambiguates an Aktenzeichen.
    pub date: Option<String>,
}

impl GetDecisionParams {
    fn to_query(&self) -> Query {
        let mut query = Query::new();
        query.push_opt("max_chars", &self.max_chars);
        query.push_opt("rn_from", &self.rn_from);
        query.push_opt("rn_to", &self.rn_to);
        query.push_opt("section", &self.section);
        query.push_opt("court", &self.court);
        query.push_opt("date", &self.date);
        query
    }
}

/// Metadata of a decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionMetadata {
    /// The document type, for example `Urteil`.
    #[serde(default)]
    pub document_type: Option<String>,
    /// The ECLI, when known.
    #[serde(default)]
    pub ecli: Option<String>,
    /// Every Aktenzeichen of the decision.
    #[serde(default)]
    pub file_numbers: Vec<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The Randnummer range a decision holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvailableRange {
    /// First Randnummer.
    #[serde(default)]
    pub rn_from: Option<u64>,
    /// Last Randnummer.
    #[serde(default)]
    pub rn_to: Option<u64>,
    /// `printed` when the court printed the numbers.
    #[serde(default)]
    pub rn_source: Option<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The answer of [`Decisions::get`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// The artifact id.
    pub id: String,
    /// The Aktenzeichen.
    #[serde(default)]
    pub aktenzeichen: String,
    /// The court short form.
    #[serde(default)]
    pub court: String,
    /// The decision date, `YYYY-MM-DD`.
    #[serde(default)]
    pub date: Option<String>,
    /// The jurisdiction code.
    #[serde(default)]
    pub jurisdiction: Option<String>,
    /// Document type, ECLI and file numbers.
    #[serde(default)]
    pub metadata: Option<DecisionMetadata>,
    /// Whether the later history was checked. It is never promised.
    #[serde(default)]
    pub fortgeltung_checked: bool,
    /// The German note about Fortgeltung.
    #[serde(default)]
    pub fortgeltung_note: Option<String>,
    /// The text of the decision, or the requested slice of it.
    #[serde(default)]
    pub full_text: String,
    /// The length of the whole text in characters.
    #[serde(default)]
    pub text_length: u64,
    /// True when `full_text` is shorter than the decision.
    #[serde(default)]
    pub truncated: bool,
    /// Where the Randnummern come from.
    #[serde(default)]
    pub rn_source: Option<String>,
    /// A note about `rn_source`.
    #[serde(default)]
    pub rn_source_note: Option<String>,
    /// The sections this decision has.
    #[serde(default)]
    pub available_sections: Vec<String>,
    /// The Randnummer range this decision holds.
    #[serde(default)]
    pub available_range: Option<AvailableRange>,
    /// What `available_range` covers.
    #[serde(default)]
    pub available_range_scope: Option<String>,
    /// Whether the text carries Randnummer anchors.
    #[serde(default)]
    pub has_rn_anchors: bool,
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

/// Parameters of [`Decisions::citations`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CitationsParams {
    /// Which neighbours to list. The server default is both.
    pub direction: Option<Direction>,
    /// How far to follow. The server default is one hop.
    pub depth: Option<Depth>,
    /// Court short form that disambiguates an Aktenzeichen.
    pub court: Option<String>,
    /// Decision date (`YYYY-MM-DD`) that disambiguates an Aktenzeichen.
    pub date: Option<String>,
    /// 1-based page, up to 20. Defined at depth 1 only.
    pub page: Option<u32>,
}

impl CitationsParams {
    fn to_query(&self) -> Query {
        let mut query = Query::new();
        query.push_opt("direction", &self.direction);
        query.push_opt("depth", &self.depth);
        query.push_opt("court", &self.court);
        query.push_opt("date", &self.date);
        query.push_opt("page", &self.page);
        query
    }
}

/// The corpus entry a citation resolved to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorpusRef {
    /// The artifact id.
    #[serde(default)]
    pub artifact_id: String,
    /// The court short form.
    #[serde(default)]
    pub court: Option<String>,
    /// The decision date.
    #[serde(default)]
    pub date: Option<String>,
    /// The Aktenzeichen.
    #[serde(default)]
    pub az: Option<String>,
    /// The document type.
    #[serde(default)]
    pub document_type: Option<String>,
    /// The ECLI.
    #[serde(default)]
    pub ecli: Option<String>,
    /// The page of the decision on klaracase.de.
    #[serde(default)]
    pub corpus_url: Option<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One citation edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitationEdge {
    /// The canonical form of the cited reference.
    #[serde(default)]
    pub reference_canonical: String,
    /// The role of the reference, for example `leitentscheidung`.
    #[serde(default)]
    pub reference_role: Option<String>,
    /// For example `resolved`.
    #[serde(default)]
    pub resolution_status: String,
    /// Whether the other end is in the corpus.
    #[serde(default)]
    pub in_corpus: bool,
    /// The corpus entry, when the other end is in the corpus.
    #[serde(default)]
    pub corpus: Option<CorpusRef>,
    /// The artifact id the edge starts at.
    #[serde(default)]
    pub from_artifact_id: Option<String>,
    /// The hop count. Set by the citation graph.
    #[serde(default)]
    pub hop: Option<u64>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The decision a citation graph starts at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitationSource {
    /// The artifact id.
    #[serde(default)]
    pub id: String,
    /// The Aktenzeichen.
    #[serde(default)]
    pub aktenzeichen: Option<String>,
    /// The court short form.
    #[serde(default)]
    pub court: Option<String>,
    /// The decision date.
    #[serde(default)]
    pub date: Option<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Paging facts of a citation graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitationPagination {
    /// The page served.
    #[serde(default)]
    pub page: u64,
    /// Neighbours per page and direction.
    #[serde(default)]
    pub page_size: u64,
    /// The last page.
    #[serde(default)]
    pub max_page: u64,
    /// Whether more references exist.
    #[serde(default)]
    pub has_more_references: bool,
    /// Whether more citing decisions exist.
    #[serde(default)]
    pub has_more_referenced_by: bool,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The answer of [`Decisions::citations`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitationGraph {
    /// The decision the graph starts at.
    pub source: CitationSource,
    /// Decisions the source cites.
    #[serde(default)]
    pub references: Vec<CitationEdge>,
    /// Decisions citing the source.
    #[serde(default)]
    pub referenced_by: Vec<CitationEdge>,
    /// The direction served.
    #[serde(default)]
    pub served_direction: String,
    /// References served on this page.
    #[serde(default)]
    pub served_references_total: u64,
    /// Citing decisions served on this page.
    #[serde(default)]
    pub served_referenced_by_total: u64,
    /// All references.
    #[serde(default)]
    pub references_total: u64,
    /// References that resolved.
    #[serde(default)]
    pub references_resolved_total: u64,
    /// All citing decisions.
    #[serde(default)]
    pub referenced_by_total: u64,
    /// References that did not resolve.
    #[serde(default)]
    pub references_unresolved_total: u64,
    /// Why references were withheld.
    #[serde(default)]
    pub references_withheld_reasons: Vec<Value>,
    /// What the totals cover.
    #[serde(default)]
    pub totals_scope: String,
    /// Cap per direction.
    #[serde(default)]
    pub per_direction_cap: u64,
    /// Cap on unresolved edges.
    #[serde(default)]
    pub unresolved_cap: u64,
    /// Cap on nodes at depth 2.
    #[serde(default)]
    pub node_cap: u64,
    /// Whether the graph was cut.
    #[serde(default)]
    pub truncated: bool,
    /// Nodes left out at depth 2.
    #[serde(default)]
    pub nodes_omitted: u64,
    /// True when the payload was cut to fit the response budget.
    #[serde(default)]
    pub payload_truncated: bool,
    /// Paging facts.
    #[serde(default)]
    pub pagination: Option<CitationPagination>,
    /// Whether the later history was checked. It is never promised.
    #[serde(default)]
    pub fortgeltung_checked: bool,
    /// The German note about Fortgeltung.
    #[serde(default)]
    pub fortgeltung_note: Option<String>,
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
    SearchHit,
    DecisionSearchResults,
    DecisionMetadata,
    AvailableRange,
    Decision,
    CorpusRef,
    CitationEdge,
    CitationSource,
    CitationPagination,
    CitationGraph,
);

/// The decision operations. Get it with [`Klaracase::decisions`].
#[derive(Debug, Clone, Copy)]
pub struct Decisions<'a> {
    client: &'a Klaracase,
}

impl Klaracase {
    /// The decision operations: search, read and citation graph.
    #[must_use]
    pub fn decisions(&self) -> Decisions<'_> {
        Decisions { client: self }
    }
}

impl Decisions<'_> {
    /// Searches the court decisions. `GET /decisions/search`. One credit per distinct query and
    /// filter set per month; paging and a repeat cost nothing more.
    ///
    /// ```no_run
    /// use klaracase::{Klaracase, Mode, SearchDecisionsParams};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let found = client
    ///     .decisions()
    ///     .search(SearchDecisionsParams {
    ///         query: "Widerrufsrecht Fernabsatzvertrag".into(),
    ///         mode: Some(Mode::Hybrid),
    ///         bridge: Some(false),
    ///         ..Default::default()
    ///     })
    ///     .await?;
    /// println!("{} hits, status {}", found.results.len(), found.meta.status);
    /// # Ok(()) }
    /// ```
    pub async fn search(
        &self,
        params: SearchDecisionsParams,
    ) -> Result<Response<DecisionSearchResults>, Error> {
        self.client
            .execute(Request::get("/decisions/search", params.to_query()))
            .await
    }

    /// Reads one decision. `GET /decisions/{id}`. The id is a UUID, an Aktenzeichen or an ECLI. The
    /// client encodes it as one path segment. One credit per decision per month.
    ///
    /// ```no_run
    /// use klaracase::{GetDecisionParams, Klaracase};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let decision = client
    ///     .decisions()
    ///     .get(
    ///         "1 StR 185/01",
    ///         GetDecisionParams { section: Some("Gründe".into()), ..Default::default() },
    ///     )
    ///     .await?;
    /// println!("{}", decision.full_text);
    /// # Ok(()) }
    /// ```
    pub async fn get(
        &self,
        id: &str,
        params: GetDecisionParams,
    ) -> Result<Response<Decision>, Error> {
        let path = format!("/decisions/{}", path_segment("id", id)?);
        self.client
            .execute(Request::get(path, params.to_query()))
            .await
    }

    /// Lists the citations of one decision. `GET /decisions/{id}/citations`. Two credits per
    /// decision, direction and depth per month; paging is free.
    pub async fn citations(
        &self,
        id: &str,
        params: CitationsParams,
    ) -> Result<Response<CitationGraph>, Error> {
        let path = format!("/decisions/{}/citations", path_segment("id", id)?);
        self.client
            .execute(Request::get(path, params.to_query()))
            .await
    }
}
