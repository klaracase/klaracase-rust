//! Corpus facts: `GET /corpus/coverage` and `GET /corpus/facets/{field}`. Both are free.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::client::{Klaracase, Request};
use crate::common::{Count, Credits};
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::open::response_struct;
use crate::response::Response;
use crate::types::{Branch, FacetField, FacetType};

/// Parameters of [`Corpus::coverage`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CoverageParams {
    /// Scope the report to one Gerichtsbarkeit. Omit it to report every branch.
    pub branch: Option<Branch>,
    /// Cap on the per-court rows, busiest first. The server default is 20 and the maximum is 100.
    pub top: Option<u32>,
}

impl CoverageParams {
    fn to_query(&self) -> Query {
        let mut query = Query::new();
        query.push_opt("branch", &self.branch);
        query.push_opt("top", &self.top);
        query
    }
}

response_struct! {
    /// One court in the coverage report.
    CourtCoverage {
        /// The court short form.
        req court: String,
        /// The Gerichtsbarkeit.
        opt branch: String,
        /// How many decisions.
        opt count: Count,
        /// The earliest decision month, for example `1950-10`.
        opt earliest: String,
        /// The latest decision month.
        opt latest: String,
        /// The year the court was founded, the floor for its dates.
        opt court_founding_year: u64,
        /// Where the founding year comes from.
        opt court_founding_basis: String,
        /// Whether the corpus holds dates below that floor.
        opt has_dates_below_floor: bool,
    }
}

response_struct! {
    /// How the decisions of a branch spread over instances.
    InstanceMix {
        /// Federal courts.
        opt federal: Count,
        /// Appellate courts.
        opt appellate: Count,
        /// First-instance and regional courts.
        opt first_instance_and_regional: Count,
        /// Everything else.
        opt other: Count,
        /// A note in German with the shares.
        opt note: String,
    }
}

response_struct! {
    /// One branch in the coverage report.
    BranchCoverage {
        /// The Gerichtsbarkeit slug.
        req branch: String,
        /// How many decisions.
        opt count: Count,
        /// The earliest decision month.
        opt earliest: String,
        /// The latest decision month.
        opt latest: String,
        /// The year the weakest court of the branch was founded.
        opt court_founding_year: u64,
        /// Where the founding year comes from.
        opt court_founding_basis: String,
        /// Whether the corpus holds dates below that floor.
        opt has_dates_below_floor: bool,
        /// The split by instance.
        opt instance_mix: InstanceMix,
        /// The busiest courts of the branch.
        list top_courts: CourtCoverage,
    }
}

response_struct! {
    /// One ingestion range: what the corpus holds for a court or bucket.
    CoverageRange {
        /// The bucket name, for example `BGH_STRAFRECHT`.
        opt bucket: String,
        /// The court short form.
        opt court: String,
        /// A label for display.
        opt label: String,
        /// The first date the range covers.
        opt from: String,
        /// The last date the range covers. `None` is an open end, not an unknown one.
        opt until: String,
        /// The sources the range comes from.
        list sources: String,
        /// Whether the range is exhaustive.
        opt exhaustive: bool,
        /// Whether scanned pages went through text recognition.
        opt ocr: bool,
        /// A note.
        opt note: String,
        /// The earliest decision the corpus holds in this range.
        opt holdings_earliest: String,
    }
}

response_struct! {
    /// The ingestion boundary every `out_of_coverage` verdict is decided against.
    CoverageRanges {
        /// A note.
        opt note: String,
        /// What the ranges cover.
        opt scope: String,
        /// When the ranges were verified.
        opt verified_at: String,
        /// The ranges.
        list ranges: CoverageRange,
    }
}

response_struct! {
    /// Which statutes the corpus does not hold.
    LawCoverage {
        /// A note.
        opt note: String,
        /// EU law the corpus holds.
        list eu: String,
        /// International law the corpus holds.
        list international: String,
    }
}

response_struct! {
    /// The answer of [`Corpus::coverage`].
    CorpusCoverage {
        /// All decisions.
        opt total_decisions: Count,
        /// Per branch.
        list branches: BranchCoverage,
        /// Per court, busiest first.
        list courts: CourtCoverage,
        /// How many court rows this answer holds.
        opt courts_returned: u64,
        /// How many courts exist.
        opt courts_total: u64,
        /// A note in German about the earliest decisions held per branch.
        opt corpus_floors: String,
        /// The ingestion boundary.
        opt coverage_ranges: CoverageRanges,
        /// When the figures were computed.
        opt as_of: String,
        /// Caveats for filters.
        opt filter_coverage_caveats: String,
        /// Statute coverage.
        opt law_coverage: LawCoverage,
        /// The API version of the answer.
        opt api_version: String,
        /// Link to the changelog.
        opt changelog_url: String,
        /// What the call cost.
        opt credits: Credits,
    }
}

/// Facet values with their counts, in the order the server sent them (busiest first).
///
/// A `BTreeMap` would sort by name and lose that order. Use [`FacetValues::get`] to look one up and
/// [`FacetValues::iter`] to walk them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FacetValues(pub Vec<(String, Count)>);

impl FacetValues {
    /// The count of one facet value, or `None` when the value is absent.
    #[must_use]
    pub fn get(&self, value: &str) -> Option<&Count> {
        self.0
            .iter()
            .find(|(name, _)| name == value)
            .map(|(_, count)| count)
    }

    /// The values and counts in server order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Count)> {
        self.0.iter().map(|(name, count)| (name.as_str(), count))
    }

    /// How many values this answer holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when the answer holds no value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for FacetValues {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().map(|(name, count)| (name, count)))
    }
}

impl<'de> Deserialize<'de> for FacetValues {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let map = Map::<String, Value>::deserialize(deserializer)?;
        map.into_iter()
            .map(|(name, raw)| {
                serde_json::from_value::<Count>(raw)
                    .map(|count| (name, count))
                    .map_err(D::Error::custom)
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }
}

response_struct! {
    /// The answer of [`Corpus::facets`].
    Facets {
        /// The field that was listed.
        req field: String,
        /// What the field applies to, for example `DECISION`.
        opt applies_to: String,
        /// Value to count, busiest first.
        dflt values: FacetValues,
        /// How many values exist.
        opt total_values: u64,
        /// Whether `values` was cut at `value_limit`.
        opt truncated: bool,
        /// The most values one answer holds.
        opt value_limit: u64,
        /// What the counts count.
        opt count_basis: String,
        /// When the figures were computed.
        opt as_of: String,
        /// For `cited_norms`: the canonical norm that was listed.
        opt norm: String,
        /// The API version of the answer.
        opt api_version: String,
        /// Link to the changelog.
        opt changelog_url: String,
        /// What the call cost.
        opt credits: Credits,
    }
}

/// Parameters of [`Corpus::facets`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FacetsParams {
    /// Restrict the counts to one artifact type. Ignored for `cited_norms`.
    pub type_: Option<FacetType>,
    /// Required with [`FacetField::CitedNorms`]: the norm whose family to list, for example
    /// `§ 543 BGB`.
    pub norm: Option<String>,
    /// Case-insensitive prefix filter. A trailing space counts.
    pub prefix: Option<String>,
}

impl FacetsParams {
    fn to_query(&self) -> Query {
        let mut query = Query::new();
        query.push_opt("type", &self.type_);
        query.push_opt("norm", &self.norm);
        query.push_opt("prefix", &self.prefix);
        query
    }
}

/// The corpus operations. Get it with [`Klaracase::corpus`].
#[derive(Debug, Clone, Copy)]
pub struct Corpus<'a> {
    client: &'a Klaracase,
}

impl Klaracase {
    /// The corpus operations: coverage and facets. Both are free.
    #[must_use]
    pub fn corpus(&self) -> Corpus<'_> {
        Corpus { client: self }
    }
}

impl Corpus<'_> {
    /// Reports what the corpus holds. `GET /corpus/coverage`. Free.
    ///
    /// ```no_run
    /// use klaracase::{CoverageParams, Klaracase};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let coverage = client.corpus().coverage(CoverageParams { top: Some(5), ..Default::default() }).await?;
    /// for court in &coverage.courts {
    ///     println!("{} {:?}", court.court, court.count.as_ref().map(|c| c.value));
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn coverage(
        &self,
        params: CoverageParams,
    ) -> Result<Response<CorpusCoverage>, Error> {
        self.client
            .execute(Request::get("/corpus/coverage", params.to_query()))
            .await
    }

    /// Lists the values of a filterable field with counts. `GET /corpus/facets/{field}`. Free.
    /// Use it to find values that exist before you filter a search.
    ///
    /// ```no_run
    /// use klaracase::{FacetField, FacetsParams, Klaracase};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let courts = client
    ///     .corpus()
    ///     .facets(FacetField::CourtName, FacetsParams { prefix: Some("BG".into()), ..Default::default() })
    ///     .await?;
    /// println!("{:?}", courts.values.get("BGH"));
    /// # Ok(()) }
    /// ```
    pub async fn facets(
        &self,
        field: FacetField,
        params: FacetsParams,
    ) -> Result<Response<Facets>, Error> {
        let path = format!("/corpus/facets/{}", path_segment("field", field.as_str())?);
        self.client
            .execute(Request::get(path, params.to_query()))
            .await
    }
}
