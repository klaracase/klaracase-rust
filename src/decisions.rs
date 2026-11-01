//! Decisions: search, read and citation graph.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::{Klaracase, Request};
use crate::common::{Count, Credits, QueryExpansion, Snippet};
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::open::{Open, response_struct};
use crate::pager::{PageEnd, PageRequest, Pager};
use crate::params::{check_date, check_date_range, invalid, option_setters, require_text};
use crate::response::Response;
use crate::types::{
    ArtifactType, Branch, Confidence, Depth, Direction, DocumentType, Jurisdiction, Mode,
    ResolutionStatus, Statute,
};

/// Parameters of [`Decisions::search`].
///
/// Start with [`SearchDecisionsParams::new`] and chain setters. A field you do not set is not sent;
/// `limit(0)` and `bridge(false)` are values and are sent. The struct is `#[non_exhaustive]`, so a
/// parameter added in a later release does not break your code.
///
/// Setters that take a closed list accept the typed value (`Branch::Verwaltung`) or the server's own
/// spelling (`"verwaltung"`). The string form is the escape hatch for a value this crate does not
/// know yet.
///
/// ```
/// use klaracase::{Branch, Mode, SearchDecisionsParams, Statute};
///
/// let params = SearchDecisionsParams::new("Vermögensbetreuungspflicht Untreue")
///     .court("BGH")
///     .branch(Branch::Ordentlich)
///     .cited_statute(Statute::StGB)
///     .date_from("2015-01-01")
///     .mode(Mode::Hybrid)
///     .limit(5);
/// assert!(params.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SearchDecisionsParams {
    /// German legal-concept phrasing. The server requires 2 to 500 characters. Required.
    pub query: String,
    /// Court name, as listed by `corpus().facets(FacetField::CourtName, ..)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub court: Option<String>,
    /// The Gerichtsbarkeit of the deciding court.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<Open<Branch>>,
    /// One decision document type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_type: Option<Open<DocumentType>>,
    /// Decisions whose Normenkette cites this norm, for example `§ 906 BGB`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cited_norm: Option<String>,
    /// Any section of these statutes. An empty list is not sent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cited_statutes: Vec<Open<Statute>>,
    /// Earliest decision date, `YYYY-MM-DD`, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_from: Option<String>,
    /// Latest decision date, `YYYY-MM-DD`, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_to: Option<String>,
    /// Retrieval mode. The server default is hybrid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<Open<Mode>>,
    /// Maximum results. The server caps it at 20.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// 1-based page, up to 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Whether the server may add curated register terms to the query. The server default is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<bool>,
}

impl SearchDecisionsParams {
    /// Parameters with only the query set.
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            court: None,
            branch: None,
            document_type: None,
            cited_norm: None,
            cited_statutes: Vec::new(),
            date_from: None,
            date_to: None,
            mode: None,
            limit: None,
            page: None,
            bridge: None,
        }
    }

    option_setters! {
        direct {
            /// Sets the court name filter.
            text court,
            /// Sets the Gerichtsbarkeit filter.
            open(Branch) branch,
            /// Restricts the search to one document type.
            open(DocumentType) document_type,
            /// Restricts the search to decisions that cite this norm, for example `§ 906 BGB`.
            text cited_norm,
            /// Sets the earliest decision date, `YYYY-MM-DD`.
            text date_from,
            /// Sets the latest decision date, `YYYY-MM-DD`.
            text date_to,
            /// Sets the retrieval mode.
            open(Mode) mode,
            /// Sets the maximum number of results.
            plain(u32) limit,
            /// Sets the 1-based page.
            plain(u32) page,
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

    /// Checks the shape of the input without any request.
    ///
    /// The client calls this before it sends. It refuses a blank query, a date that is not
    /// `YYYY-MM-DD` and `date_from` after `date_to`. It never checks a number the server owns, such as
    /// a length or a cap.
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

/// Parameters of [`Decisions::get`] and [`Decisions::get_with`].
///
/// ```
/// use klaracase::GetDecisionParams;
///
/// let params = GetDecisionParams::default().section("Gründe").max_chars(4000);
/// assert!(params.validate().is_ok());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct GetDecisionParams {
    /// Maximum characters of text. The server default is 40,000 and the minimum is 500.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<u32>,
    /// First Randnummer to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rn_from: Option<u32>,
    /// Last Randnummer to return, inclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rn_to: Option<u32>,
    /// One named section: `Tenor`, `Leitsatz`, `Tatbestand` or `Gründe`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Court short form that disambiguates an Aktenzeichen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub court: Option<String>,
    /// Decision date (`YYYY-MM-DD`) that disambiguates an Aktenzeichen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

impl GetDecisionParams {
    option_setters! {
        direct {
            /// Caps the characters of text returned.
            plain(u32) max_chars,
            /// Sets the first Randnummer to return.
            plain(u32) rn_from,
            /// Sets the last Randnummer to return, inclusive.
            plain(u32) rn_to,
            /// Selects one named section.
            text section,
            /// Sets the court short form that disambiguates an Aktenzeichen.
            text court,
            /// Sets the decision date (`YYYY-MM-DD`) that disambiguates an Aktenzeichen.
            text date,
        }
    }

    /// Checks the shape of the input without any request: `date` is `YYYY-MM-DD` and `rn_from` is
    /// not above `rn_to`.
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] with a message that names the argument.
    pub fn validate(&self) -> Result<(), Error> {
        check_date("date", self.date.as_deref())?;
        if let (Some(from), Some(to)) = (self.rn_from, self.rn_to) {
            if from > to {
                return Err(invalid("`rn_from` must not be above `rn_to`"));
            }
        }
        Ok(())
    }

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

/// Parameters of [`Decisions::citations`] and [`Decisions::citations_with`].
///
/// ```
/// use klaracase::{CitationsParams, Direction};
///
/// let params = CitationsParams::default().direction(Direction::Incoming).page(2);
/// assert!(params.validate().is_ok());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct CitationsParams {
    /// Which neighbours to list. The server default is both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Open<Direction>>,
    /// How far to follow. The server default is one hop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<Open<Depth>>,
    /// Court short form that disambiguates an Aktenzeichen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub court: Option<String>,
    /// Decision date (`YYYY-MM-DD`) that disambiguates an Aktenzeichen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// 1-based page, up to 20. Defined at depth 1 only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
}

impl CitationsParams {
    option_setters! {
        direct {
            /// Sets which neighbours to list.
            open(Direction) direction,
            /// Sets how far to follow citations.
            open(Depth) depth,
            /// Sets the court short form that disambiguates an Aktenzeichen.
            text court,
            /// Sets the decision date (`YYYY-MM-DD`) that disambiguates an Aktenzeichen.
            text date,
            /// Sets the 1-based page.
            plain(u32) page,
        }
    }

    /// Checks the shape of the input without any request: `date` is `YYYY-MM-DD`.
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] with a message that names the argument.
    pub fn validate(&self) -> Result<(), Error> {
        check_date("date", self.date.as_deref())
    }

    pub(crate) fn to_query(&self) -> Query {
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

/// The most pages a decision search serves.
const SEARCH_PAGE_CAP: u32 = 10;
/// The most pages a citation graph serves.
const CITATIONS_PAGE_CAP: u32 = 20;

impl<'a> Decisions<'a> {
    /// Starts a search of the court decisions. `GET /decisions/search`. Chain setters, then call
    /// [`send`](SearchRequest::send) or [`pages`](SearchRequest::pages).
    ///
    /// One credit per distinct query, filter set and mode per credit period. Paging the same query
    /// within a credit period costs no extra credits. A changed query, filter or mode is a new charge.
    ///
    /// ```no_run
    /// use klaracase::{Klaracase, Mode};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let found = client
    ///     .decisions()
    ///     .search("Widerrufsrecht Fernabsatzvertrag")
    ///     .mode(Mode::Hybrid)
    ///     .bridge(false)
    ///     .send()
    ///     .await?;
    /// println!("{} hits, status {}", found.results.len(), found.meta.status);
    /// # Ok(()) }
    /// ```
    pub fn search(&self, query: impl Into<String>) -> SearchRequest<'a> {
        SearchRequest {
            client: self.client,
            params: SearchDecisionsParams::new(query),
        }
    }

    /// Searches the court decisions with a ready [`SearchDecisionsParams`]. `GET /decisions/search`.
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] for input of the wrong shape, before any request, or any error
    /// the call returns.
    pub async fn search_with(
        &self,
        params: SearchDecisionsParams,
    ) -> Result<Response<DecisionSearchResults>, Error> {
        self.client.checked(params.validate())?;
        self.client
            .execute(Request::get("/decisions/search", params.to_query()))
            .await
    }

    /// Reads one decision. `GET /decisions/{id}`. The id is a UUID, an Aktenzeichen or an ECLI. The
    /// client encodes it as one path segment. One credit per decision per month.
    ///
    /// ```no_run
    /// use klaracase::Klaracase;
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let decision = client.decisions().get("1 StR 185/01").await?;
    /// println!("{:?}", decision.full_text);
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    ///
    /// Any [`Error`] the call returns.
    pub async fn get(&self, id: &str) -> Result<Response<Decision>, Error> {
        self.get_with(id, GetDecisionParams::default()).await
    }

    /// Reads one decision, or a slice of it. `GET /decisions/{id}`.
    ///
    /// ```no_run
    /// use klaracase::{GetDecisionParams, Klaracase};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let decision = client
    ///     .decisions()
    ///     .get_with("1 StR 185/01", GetDecisionParams::default().section("Gründe").max_chars(4000))
    ///     .await?;
    /// # let _ = decision;
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] for input of the wrong shape, before any request, or any error
    /// the call returns.
    pub async fn get_with(
        &self,
        id: &str,
        params: GetDecisionParams,
    ) -> Result<Response<Decision>, Error> {
        self.client.checked(params.validate())?;
        let path = format!("/decisions/{}", path_segment("id", id)?);
        self.client
            .execute(Request::get(path, params.to_query()))
            .await
    }

    /// Starts a read of the citations of one decision. `GET /decisions/{id}/citations`. Chain
    /// setters, then call [`send`](CitationsRequest::send) or [`pages`](CitationsRequest::pages).
    ///
    /// Two credits per decision, direction and depth per credit period. Paging the same graph within
    /// a credit period costs no extra credits. Check `credits` on each response.
    pub fn citations(&self, id: impl Into<String>) -> CitationsRequest<'a> {
        CitationsRequest {
            client: self.client,
            id: id.into(),
            params: CitationsParams::default(),
        }
    }

    /// Lists the citations of one decision with ready [`CitationsParams`].
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] for input of the wrong shape, before any request, or any error
    /// the call returns.
    pub async fn citations_with(
        &self,
        id: &str,
        params: CitationsParams,
    ) -> Result<Response<CitationGraph>, Error> {
        self.client.checked(params.validate())?;
        let path = format!("/decisions/{}/citations", path_segment("id", id)?);
        self.client
            .execute(Request::get(path, params.to_query()))
            .await
    }
}

/// A decision search under construction. Get it with [`Decisions::search`].
///
/// It has the setters of [`SearchDecisionsParams`].
#[derive(Debug, Clone)]
#[must_use = "a search request does nothing until you call send or pages"]
pub struct SearchRequest<'a> {
    client: &'a Klaracase,
    params: SearchDecisionsParams,
}

impl SearchRequest<'_> {
    option_setters! {
        delegate {
            /// Sets the court name filter.
            text court,
            /// Sets the Gerichtsbarkeit filter.
            open(Branch) branch,
            /// Restricts the search to one document type.
            open(DocumentType) document_type,
            /// Restricts the search to decisions that cite this norm, for example `§ 906 BGB`.
            text cited_norm,
            /// Sets the earliest decision date, `YYYY-MM-DD`.
            text date_from,
            /// Sets the latest decision date, `YYYY-MM-DD`.
            text date_to,
            /// Sets the retrieval mode.
            open(Mode) mode,
            /// Sets the maximum number of results.
            plain(u32) limit,
            /// Sets the 1-based page.
            plain(u32) page,
            /// Turns the register-term expansion on or off.
            plain(bool) bridge,
        }
    }

    /// Adds one statute to the `cited_statutes` filter.
    pub fn cited_statute(mut self, statute: impl Into<Open<Statute>>) -> Self {
        self.params = self.params.cited_statute(statute);
        self
    }

    /// Replaces the `cited_statutes` filter.
    pub fn cited_statutes<I>(mut self, statutes: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Open<Statute>>,
    {
        self.params = self.params.cited_statutes(statutes);
        self
    }

    /// The parameters built so far.
    #[must_use]
    pub fn params(&self) -> &SearchDecisionsParams {
        &self.params
    }

    /// Sends the search.
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] for input of the wrong shape, before any request, or any error
    /// the call returns.
    pub async fn send(self) -> Result<Response<DecisionSearchResults>, Error> {
        self.client.decisions().search_with(self.params).await
    }

    /// Walks the pages of this search, at most `max_pages` of them, starting at the `page` set (the
    /// first page by default).
    ///
    /// `max_pages` is required and is clamped to the 10 pages the server serves. The walk stops at
    /// the first empty page. Paging the same query within a credit period costs no extra credits. A
    /// changed query, filter or mode is a new charge. Check `credits` on each response. Each page is
    /// still a request against the rate limit. See [`Pager`].
    pub fn pages(self, max_pages: u32) -> Pager<DecisionSearchResults> {
        let first = self.params.page.unwrap_or(1);
        let params = self.params;
        let client = self.client.clone();
        let request: PageRequest = Box::new(move |page| {
            let params = params.clone().page(page);
            client.checked(params.validate())?;
            Ok(Request::get("/decisions/search", params.to_query()))
        });
        Pager::new(
            self.client.clone(),
            request,
            |page: &DecisionSearchResults| {
                if page.results.is_empty() {
                    PageEnd::Empty
                } else {
                    PageEnd::More
                }
            },
            first,
            max_pages,
            SEARCH_PAGE_CAP,
        )
    }
}

/// A citation graph read under construction. Get it with [`Decisions::citations`].
///
/// It has the setters of [`CitationsParams`].
#[derive(Debug, Clone)]
#[must_use = "a citations request does nothing until you call send or pages"]
pub struct CitationsRequest<'a> {
    client: &'a Klaracase,
    id: String,
    params: CitationsParams,
}

impl CitationsRequest<'_> {
    option_setters! {
        delegate {
            /// Sets which neighbours to list.
            open(Direction) direction,
            /// Sets how far to follow citations.
            open(Depth) depth,
            /// Sets the court short form that disambiguates an Aktenzeichen.
            text court,
            /// Sets the decision date (`YYYY-MM-DD`) that disambiguates an Aktenzeichen.
            text date,
            /// Sets the 1-based page.
            plain(u32) page,
        }
    }

    /// Sends the read.
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] for input of the wrong shape, before any request, or any error
    /// the call returns.
    pub async fn send(self) -> Result<Response<CitationGraph>, Error> {
        self.client
            .decisions()
            .citations_with(&self.id, self.params)
            .await
    }

    /// Walks the pages of this graph, at most `max_pages` of them, starting at the `page` set (the
    /// first page by default).
    ///
    /// `max_pages` is required and is clamped to the 20 pages the server serves. At depth 2 the
    /// server serves page 1 only, so the walk is one page. The walk stops at the first empty page and
    /// after a page that says no more neighbours follow. Paging the same graph within a credit period
    /// costs no extra credits. A changed decision, direction or depth is a new charge. Check
    /// `credits` on each response. Each page is still a request against the rate limit. See
    /// [`Pager`].
    pub fn pages(self, max_pages: u32) -> Pager<CitationGraph> {
        let first = self.params.page.unwrap_or(1);
        let depth_two = self.params.depth.as_ref().is_some_and(|d| *d == Depth::Two);
        let max_pages = if depth_two {
            max_pages.min(1)
        } else {
            max_pages
        };
        let params = self.params;
        let client = self.client.clone();
        let id = self.id;
        let request: PageRequest = Box::new(move |page| {
            let params = params.clone().page(page);
            client.checked(params.validate())?;
            let path = format!("/decisions/{}/citations", path_segment("id", &id)?);
            Ok(Request::get(path, params.to_query()))
        });
        Pager::new(
            self.client.clone(),
            request,
            |graph: &CitationGraph| {
                let empty = graph.references.is_empty() && graph.referenced_by.is_empty();
                let more = graph.pagination.as_ref().map(|p| {
                    p.has_more_references == Some(true) || p.has_more_referenced_by == Some(true)
                });
                match (empty, more) {
                    (true, _) => PageEnd::Empty,
                    (false, Some(false)) => PageEnd::Last,
                    (false, _) => PageEnd::More,
                }
            },
            first,
            max_pages,
            CITATIONS_PAGE_CAP,
        )
    }
}
