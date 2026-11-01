//! Decisions: search, read and citation graph.

use serde_json::Value;

use crate::client::{Klaracase, Request};
use crate::common::{Count, Credits, QueryExpansion, Snippet};
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::open::{Open, response_struct};
use crate::response::Response;
use crate::types::{
    ArtifactType, Branch, Confidence, Depth, Direction, DocumentType, Jurisdiction, Mode,
    ResolutionStatus, Statute,
};

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

response_struct! {
    /// One decision hit, from a decision search or the decisions section of a cross-type search.
    DecisionHit {
        /// The artifact id.
        req id: String,
        /// `DECISION`. Present in a cross-type search, absent in a decision-only search.
        opt artifact_type: Open<ArtifactType>,
        /// The Aktenzeichen.
        opt aktenzeichen: String,
        /// The court short form.
        opt court: String,
        /// The decision date, `YYYY-MM-DD`.
        opt date: String,
        /// The document type, for example `Urteil`.
        opt document_type: String,
        /// A title for display.
        opt title: String,
        /// The best passage, with `<mark>` tags.
        opt snippet: String,
        /// The matching passages.
        list snippets: Snippet,
        /// The title of the matching chunk, for example `Gründe`.
        opt chunk_title: String,
        /// The Randnummer range of the matching chunk, for example `21-23`.
        opt rn_range: String,
        /// A Fundstelle, when known.
        opt fundstelle: String,
        /// Whether the later history of the decision was checked. It is never promised.
        opt fortgeltung_checked: bool,
        /// The German note about Fortgeltung.
        opt fortgeltung_note: String,
        /// How sure the ranker is about this hit.
        opt confidence: Open<Confidence>,
    }
}

response_struct! {
    /// The answer of [`Decisions::search`].
    DecisionSearchResults {
        /// The hits, best first.
        list results: DecisionHit,
        /// True when the payload was cut to fit the response budget.
        opt payload_truncated: bool,
        /// How many passages match.
        opt total_available: Count,
        /// What `total_available` counts.
        opt total_available_basis: String,
        /// The size of the candidate pool.
        opt candidate_pool: u64,
        /// Whether the pool holds every match.
        opt pool_exhaustive: bool,
        /// Whether more results exist than this page holds. A missing key is `None`, not `false`.
        opt truncated: bool,
        /// The page served.
        opt page: u64,
        /// The limit applied.
        opt limit: u64,
        /// The retrieval mode used.
        opt mode: Open<Mode>,
        /// Decisions that many hits cite. The API has never sent an element, so the element type
        /// stays `Value` until a sample exists.
        list frequently_cited_by_results: Value,
        /// What the server added to the query, and why.
        opt query_expansion: QueryExpansion,
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

response_struct! {
    /// Metadata of a decision.
    DecisionMetadata {
        /// The document type, for example `Urteil`.
        opt document_type: String,
        /// The ECLI, when known.
        opt ecli: String,
        /// Every Aktenzeichen of the decision.
        list file_numbers: String,
    }
}

response_struct! {
    /// The Randnummer range a decision holds.
    AvailableRange {
        /// First Randnummer.
        opt rn_from: u64,
        /// Last Randnummer.
        opt rn_to: u64,
        /// `printed` when the court printed the numbers.
        opt rn_source: String,
    }
}

response_struct! {
    /// The span of text a slice call served. Whole chunks come back, so it can be wider than the
    /// request.
    ReturnedRange {
        /// First Randnummer served.
        opt rn_from: u64,
        /// Last Randnummer served.
        opt rn_to: u64,
        /// The heading of the section the span sits in.
        opt section_heading: String,
    }
}

response_struct! {
    /// The answer of [`Decisions::get`].
    Decision {
        /// The artifact id.
        req id: String,
        /// The Aktenzeichen.
        opt aktenzeichen: String,
        /// The court short form.
        opt court: String,
        /// The decision date, `YYYY-MM-DD`.
        opt date: String,
        /// The jurisdiction code.
        opt jurisdiction: Open<Jurisdiction>,
        /// Document type, ECLI and file numbers.
        opt metadata: DecisionMetadata,
        /// Whether the later history was checked. It is never promised.
        opt fortgeltung_checked: bool,
        /// The German note about Fortgeltung.
        opt fortgeltung_note: String,
        /// The text of the decision, or the requested slice of it.
        opt full_text: String,
        /// The length of the whole text in characters.
        opt text_length: u64,
        /// The length of the served slice in characters.
        opt served_text_length: u64,
        /// True when `full_text` is shorter than the decision. A missing key is `None`, not `false`.
        opt truncated: bool,
        /// Where the Randnummern come from.
        opt rn_source: String,
        /// Where the verdict about `rn_source` came from.
        opt rn_source_basis: String,
        /// A note about `rn_source`.
        opt rn_source_note: String,
        /// The sections this decision has.
        list available_sections: String,
        /// The Randnummer range this decision holds.
        opt available_range: AvailableRange,
        /// What `available_range` covers.
        opt available_range_scope: String,
        /// The span a slice call served.
        opt returned_range: ReturnedRange,
        /// Names a widened, cut or refuted span.
        opt returned_range_note: String,
        /// Whether the text carries Randnummer anchors.
        opt has_rn_anchors: bool,
        /// The API version of the answer.
        opt api_version: String,
        /// Link to the changelog.
        opt changelog_url: String,
        /// What the call cost.
        opt credits: Credits,
    }
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

response_struct! {
    /// The corpus entry a citation resolved to.
    CorpusRef {
        /// The artifact id.
        req artifact_id: String,
        /// The court short form.
        opt court: String,
        /// The decision date.
        opt date: String,
        /// The Aktenzeichen.
        opt az: String,
        /// The document type.
        opt document_type: String,
        /// The ECLI.
        opt ecli: String,
        /// The page of the decision on klaracase.de.
        opt corpus_url: String,
    }
}

response_struct! {
    /// One citation edge.
    CitationEdge {
        /// The canonical form of the cited reference.
        req reference_canonical: String,
        /// The role of the reference, for example `leitentscheidung`.
        opt reference_role: String,
        /// How far the edge resolved.
        opt resolution_status: Open<ResolutionStatus>,
        /// Whether the other end is in the corpus.
        opt in_corpus: bool,
        /// The corpus entry, when the other end is in the corpus.
        opt corpus: CorpusRef,
        /// The artifact id the edge starts at.
        opt from_artifact_id: String,
        /// The hop count. Set by the citation graph at depth 2.
        opt hop: u64,
    }
}

response_struct! {
    /// The decision a citation graph starts at.
    CitationSource {
        /// The artifact id.
        req id: String,
        /// The Aktenzeichen.
        opt aktenzeichen: String,
        /// The court short form.
        opt court: String,
        /// The decision date.
        opt date: String,
    }
}

response_struct! {
    /// Paging facts of a citation graph.
    CitationPagination {
        /// The page served.
        opt page: u64,
        /// Neighbours per page and direction.
        opt page_size: u64,
        /// The last page.
        opt max_page: u64,
        /// The most distinct neighbours any page can reach.
        opt reachable_distinct_max: u64,
        /// Pages of references.
        opt references_pages: u64,
        /// Pages of citing decisions.
        opt referenced_by_pages: u64,
        /// Whether more references exist.
        opt has_more_references: bool,
        /// Whether more citing decisions exist.
        opt has_more_referenced_by: bool,
        /// The depth at which paging applies.
        opt applies_at_depth: u64,
    }
}

response_struct! {
    /// The answer of [`Decisions::citations`].
    CitationGraph {
        /// The decision the graph starts at.
        req source: CitationSource,
        /// Decisions the source cites.
        list references: CitationEdge,
        /// Decisions citing the source.
        list referenced_by: CitationEdge,
        /// The direction served.
        opt served_direction: Open<Direction>,
        /// References served on this page.
        opt served_references_total: u64,
        /// Citing decisions served on this page.
        opt served_referenced_by_total: u64,
        /// All references.
        opt references_total: u64,
        /// References that resolved.
        opt references_resolved_total: u64,
        /// References that did not resolve.
        opt references_unresolved_total: u64,
        /// All citing decisions.
        opt referenced_by_total: u64,
        /// Why references were withheld. The API has never sent an element, so the element type
        /// stays `Value` until a sample exists.
        list references_withheld_reasons: Value,
        /// What the totals cover.
        opt totals_scope: String,
        /// Cap per direction.
        opt per_direction_cap: u64,
        /// Cap on unresolved edges.
        opt unresolved_cap: u64,
        /// Cap on nodes at depth 2.
        opt node_cap: u64,
        /// Whether the graph was cut.
        opt truncated: bool,
        /// Nodes left out at depth 2.
        opt nodes_omitted: u64,
        /// True when the payload was cut to fit the response budget.
        opt payload_truncated: bool,
        /// Paging facts.
        opt pagination: CitationPagination,
        /// Whether the later history was checked. It is never promised.
        opt fortgeltung_checked: bool,
        /// The German note about Fortgeltung.
        opt fortgeltung_note: String,
        /// The API version of the answer.
        opt api_version: String,
        /// Link to the changelog.
        opt changelog_url: String,
        /// What the call cost.
        opt credits: Credits,
    }
}

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
    /// println!("{:?}", decision.full_text);
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
