//! Corpus facts: `GET /corpus/coverage` and `GET /corpus/facets/{field}`. Both are free.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::{Klaracase, Request};
use crate::common::{Count, Credits};
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::response::{Response, impl_extras};
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

/// One court in the coverage report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CourtCoverage {
    /// The court short form.
    #[serde(default)]
    pub court: String,
    /// The Gerichtsbarkeit.
    #[serde(default)]
    pub branch: Option<String>,
    /// How many decisions.
    #[serde(default)]
    pub count: Option<Count>,
    /// The earliest decision month, for example `1950-10`.
    #[serde(default)]
    pub earliest: Option<String>,
    /// The latest decision month.
    #[serde(default)]
    pub latest: Option<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One branch in the coverage report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchCoverage {
    /// The Gerichtsbarkeit slug.
    #[serde(default)]
    pub branch: String,
    /// How many decisions.
    #[serde(default)]
    pub count: Option<Count>,
    /// The earliest decision month.
    #[serde(default)]
    pub earliest: Option<String>,
    /// The latest decision month.
    #[serde(default)]
    pub latest: Option<String>,
    /// The busiest courts of the branch.
    #[serde(default)]
    pub top_courts: Vec<CourtCoverage>,
    /// Keys this crate does not model, such as `instance_mix`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The answer of [`Corpus::coverage`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorpusCoverage {
    /// How many decisions the corpus holds.
    #[serde(default)]
    pub total_decisions: Option<Count>,
    /// The branch rollup. It is never capped.
    #[serde(default)]
    pub branches: Vec<BranchCoverage>,
    /// The busiest courts.
    #[serde(default)]
    pub courts: Vec<CourtCoverage>,
    /// How many court rows this answer holds.
    #[serde(default)]
    pub courts_returned: u64,
    /// How many courts exist.
    #[serde(default)]
    pub courts_total: u64,
    /// A German note about the earliest decisions held per branch.
    #[serde(default)]
    pub corpus_floors: Option<String>,
    /// The ingestion boundary every `out_of_coverage` verdict is decided against.
    #[serde(default)]
    pub coverage_ranges: Value,
    /// When the figures were computed.
    #[serde(default)]
    pub as_of: Option<String>,
    /// A note about known blind spots of filters.
    #[serde(default)]
    pub filter_coverage_caveats: Option<String>,
    /// The statutes behind the citation check.
    #[serde(default)]
    pub law_coverage: Value,
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

/// The answer of [`Corpus::facets`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Facets {
    /// The field, as the server names it.
    #[serde(default)]
    pub field: String,
    /// What the field applies to, for example `DECISION`.
    #[serde(default)]
    pub applies_to: Option<String>,
    /// Value to count, in the order the server sent them. Each count is an object with `value` and
    /// `is_floor`; read it with [`Facets::count`].
    #[serde(default)]
    pub values: Map<String, Value>,
    /// How many values exist.
    #[serde(default)]
    pub total_values: u64,
    /// Whether `values` was cut at `value_limit`.
    #[serde(default)]
    pub truncated: bool,
    /// The most values one answer holds.
    #[serde(default)]
    pub value_limit: u64,
    /// What the counts count.
    #[serde(default)]
    pub count_basis: Option<String>,
    /// When the figures were computed.
    #[serde(default)]
    pub as_of: Option<String>,
    /// For `cited_norms`: the canonical norm that was listed.
    #[serde(default)]
    pub norm: Option<String>,
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

impl Facets {
    /// The count of one facet value, or `None` when the value is absent or malformed.
    #[must_use]
    pub fn count(&self, value: &str) -> Option<Count> {
        self.values
            .get(value)
            .and_then(|raw| serde_json::from_value(raw.clone()).ok())
    }
}

impl_extras!(CourtCoverage, BranchCoverage, CorpusCoverage, Facets);

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
    /// println!("{:?}", courts.count("BGH"));
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
