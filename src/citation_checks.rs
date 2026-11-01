//! Citation checks: `POST /citation-checks` and `GET /citation-checks/{run}`.
//!
//! A citation check looks up every court-decision and statute citation of a text in the corpus and
//! answers one verdict per citation.
//!
//! A `not_found` verdict means the citation was not found in the corpus. It is not a claim that the
//! citation is fabricated. `verified_corpus` proves that the cited decision or statute exists in the
//! corpus. It does not prove that a quote or a legal proposition is correct.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::client::{Klaracase, Request};
use crate::common::Credits;
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::response::{Response, impl_extras};

/// Parameters of [`CitationChecks::create`]. Send exactly one of `text` and `document_id`.
///
/// Giving both or neither fails on the client with [`Error::Configuration`] (code
/// `client_validation`) before any request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateCitationCheckParams {
    /// The German legal text: 50 to 20,000 characters. Offsets in the answer are Unicode codepoint
    /// offsets into this exact string.
    pub text: Option<String>,
    /// The id of an upload with status `ready`. The check consumes the upload.
    pub document_id: Option<String>,
    /// The date of the document, `YYYY-MM-DD`. It applies to statute citations whose sentence names
    /// no date.
    pub reference_date: Option<String>,
}

impl CreateCitationCheckParams {
    /// Parameters that check a text.
    #[must_use]
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }

    /// Parameters that check an uploaded document.
    #[must_use]
    pub fn from_document(document_id: impl Into<String>) -> Self {
        Self {
            document_id: Some(document_id.into()),
            ..Self::default()
        }
    }

    /// Sets the document date (`YYYY-MM-DD`).
    #[must_use]
    pub fn with_reference_date(mut self, reference_date: impl Into<String>) -> Self {
        self.reference_date = Some(reference_date.into());
        self
    }

    fn to_body(&self) -> Result<Value, Error> {
        match (&self.text, &self.document_id) {
            (Some(_), Some(_)) => {
                return Err(Error::config(
                    "client_validation",
                    "send either `text` or `document_id`, not both",
                ));
            }
            (None, None) => {
                return Err(Error::config(
                    "client_validation",
                    "send either `text` or `document_id`",
                ));
            }
            _ => {}
        }
        let mut body = Map::new();
        if let Some(text) = &self.text {
            body.insert("text".to_owned(), json!(text));
        }
        if let Some(document_id) = &self.document_id {
            body.insert("document_id".to_owned(), json!(document_id));
        }
        if let Some(reference_date) = &self.reference_date {
            body.insert("reference_date".to_owned(), json!(reference_date));
        }
        Ok(Value::Object(body))
    }
}

/// The verdicts of a citation. The set is closed on the server; unknown values stay readable.
///
/// Match a verdict with these constants:
///
/// ```
/// use klaracase::Verdict;
///
/// fn describe(verdict: &str) -> &'static str {
///     match verdict {
///         Verdict::VERIFIED_CORPUS => "the cited source exists in the corpus",
///         Verdict::NOT_FOUND => "not found in the corpus; this does not mean it is invented",
///         Verdict::DISCREPANCY => "the source exists but a detail differs",
///         _ => "needs a human look",
///     }
/// }
/// assert_eq!(describe("not_found"), "not found in the corpus; this does not mean it is invented");
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Verdict;

impl Verdict {
    /// The citation resolves to a decision or statute in the corpus.
    pub const VERIFIED_CORPUS: &'static str = "verified_corpus";
    /// The citation resolves in an external register.
    pub const VERIFIED_EXTERNAL: &'static str = "verified_external";
    /// The citation was not found in the corpus. That is all the verdict says.
    pub const NOT_FOUND: &'static str = "not_found";
    /// The citation points outside the period or courts the corpus covers.
    pub const OUT_OF_COVERAGE: &'static str = "out_of_coverage";
    /// More than one source fits the citation.
    pub const AMBIGUOUS: &'static str = "ambiguous";
    /// The citation cannot be checked.
    pub const UNVERIFIABLE: &'static str = "unverifiable";
    /// The source exists but a detail of the citation differs.
    pub const DISCREPANCY: &'static str = "discrepancy";
}

/// The statuses of a citation check run.
#[derive(Debug, Clone, Copy)]
pub struct RunStatus;

impl RunStatus {
    /// Waiting to start.
    pub const PENDING: &'static str = "pending";
    /// Checking the input.
    pub const GATING: &'static str = "gating";
    /// Extracting citations.
    pub const EXTRACTING: &'static str = "extracting";
    /// Resolving citations.
    pub const RESOLVING: &'static str = "resolving";
    /// Checking external registers.
    pub const CHECKING_EXTERNAL: &'static str = "checking_external";
    /// Finished.
    pub const COMPLETE: &'static str = "complete";
    /// Refused.
    pub const REJECTED: &'static str = "rejected";
    /// Failed.
    pub const FAILED: &'static str = "failed";
}

/// How far a run got.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckProgress {
    /// Citations found, counting repeats.
    #[serde(default)]
    pub citations_total: u64,
    /// Citations checked so far.
    #[serde(default)]
    pub citations_checked: u64,
    /// Distinct citations.
    #[serde(default)]
    pub unique_citations_total: u64,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A reason for unverified citations and how many there are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasonCount {
    /// For example `statute_unknown`.
    #[serde(default)]
    pub category: String,
    /// How many citations.
    #[serde(default)]
    pub count: u64,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One distinct citation with all its spellings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniqueCitation {
    /// How the key was formed, for example `matched_decision` or `canonical`.
    #[serde(default)]
    pub key_basis: String,
    /// The artifact id of the match, if there is one.
    #[serde(default)]
    pub target_artifact_id: Option<String>,
    /// The canonical form, for example `BGH 1 StR 185/01`.
    #[serde(default)]
    pub canonical: String,
    /// Every spelling found in the text.
    #[serde(default)]
    pub spellings: Vec<String>,
    /// How often it occurs.
    #[serde(default)]
    pub occurrences: u64,
    /// See [`Verdict`].
    #[serde(default)]
    pub verdict: String,
    /// `case` or `statute`.
    #[serde(default)]
    pub reference_type: String,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The decision a citation matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchedDecision {
    /// The artifact id.
    #[serde(default)]
    pub artifact_id: String,
    /// The ECLI.
    #[serde(default)]
    pub ecli: Option<String>,
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

/// One citation found in the text.
///
/// Offsets are Unicode codepoint offsets into the text you sent. They are not byte offsets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// The finding id.
    #[serde(default)]
    pub id: String,
    /// The citation as written in the text.
    #[serde(default)]
    pub raw_text: String,
    /// The canonical form.
    #[serde(default)]
    pub canonical: String,
    /// `case` or `statute`.
    #[serde(rename = "type", default)]
    pub type_: String,
    /// The role of the citation in the text.
    #[serde(default)]
    pub role: Option<String>,
    /// See [`Verdict`].
    #[serde(default)]
    pub verdict: String,
    /// The state of the external register check, for example `skipped`.
    #[serde(default)]
    pub external_status: Option<String>,
    /// Start of the citation, in codepoints.
    #[serde(default)]
    pub start_offset: u64,
    /// End of the citation, in codepoints, exclusive.
    #[serde(default)]
    pub end_offset: u64,
    /// The index among occurrences of the same citation.
    #[serde(default)]
    pub occurrence_index: u64,
    /// The confidence of the extraction, when given.
    #[serde(default)]
    pub confidence: Option<Value>,
    /// The decision this citation matched.
    #[serde(default)]
    pub matched_decision: Option<MatchedDecision>,
    /// The result of the check of the cited subdivision.
    #[serde(default)]
    pub subdivision_check: Option<String>,
    /// A German advisory about the subdivision.
    #[serde(default)]
    pub subdivision_advisory: Option<String>,
    /// What the check covered.
    #[serde(default)]
    pub check_scope: Option<Value>,
    /// A German advisory about ambiguity.
    #[serde(default)]
    pub ambiguity_advisory: Option<String>,
    /// Coverage facts for an `out_of_coverage` verdict.
    #[serde(default)]
    pub coverage: Option<Value>,
    /// The coverage boundary.
    #[serde(default)]
    pub coverage_boundary: Option<Value>,
    /// A German advisory about coverage.
    #[serde(default)]
    pub coverage_advisory: Option<String>,
    /// The evidence behind the verdict, with `reason` and `match_strength`.
    #[serde(default)]
    pub evidence: Option<Value>,
    /// When the finding was disputed, if it was.
    #[serde(default)]
    pub disputed_at: Option<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The answer of [`CitationChecks::create`] and [`CitationChecks::get`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitationCheckRun {
    /// The run id. Read the run again with [`CitationChecks::get`].
    pub id: String,
    /// See [`RunStatus`].
    #[serde(default)]
    pub status: String,
    /// For example `api_v1`.
    #[serde(default)]
    pub source: String,
    /// How far the run got.
    #[serde(default)]
    pub progress: Option<CheckProgress>,
    /// Citations per verdict. Keys are [`Verdict`] values.
    #[serde(default)]
    pub verdict_counts: Map<String, Value>,
    /// True when not every citation was checked.
    #[serde(default)]
    pub partial: bool,
    /// How many citations stayed unverified.
    #[serde(default)]
    pub unverified_count: u64,
    /// Reasons for unverified citations.
    #[serde(default)]
    pub reason_counts: Vec<ReasonCount>,
    /// Distinct citations.
    #[serde(default)]
    pub unique_citations: Vec<UniqueCitation>,
    /// Extraction statistics.
    #[serde(default)]
    pub extraction: Value,
    /// Notes about the run.
    #[serde(default)]
    pub notes: Vec<Value>,
    /// Why `findings` is empty, when it is.
    #[serde(default)]
    pub findings_unavailable_reason: Option<String>,
    /// When the findings were purged, if they were.
    #[serde(default)]
    pub findings_purged_at: Option<String>,
    /// One finding per citation occurrence.
    #[serde(default)]
    pub findings: Vec<Finding>,
    /// When the run was submitted.
    #[serde(default)]
    pub submitted_at: Option<String>,
    /// When the run finished.
    #[serde(default)]
    pub completed_at: Option<String>,
    /// When the run was created.
    #[serde(default)]
    pub created_at: Option<String>,
    /// True when the payload was cut to fit the response budget.
    #[serde(default)]
    pub payload_truncated: bool,
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

impl CitationCheckRun {
    /// How many citations got this verdict. Zero when none did.
    #[must_use]
    pub fn verdict_count(&self, verdict: &str) -> u64 {
        self.verdict_counts
            .get(verdict)
            .and_then(Value::as_u64)
            .unwrap_or(0)
    }
}

impl_extras!(
    CheckProgress,
    ReasonCount,
    UniqueCitation,
    MatchedDecision,
    Finding,
    CitationCheckRun,
);

/// The citation check operations. Get it with [`Klaracase::citation_checks`].
#[derive(Debug, Clone, Copy)]
pub struct CitationChecks<'a> {
    client: &'a Klaracase,
}

impl Klaracase {
    /// The citation check operations.
    #[must_use]
    pub fn citation_checks(&self) -> CitationChecks<'_> {
        CitationChecks { client: self }
    }
}

impl CitationChecks<'_> {
    /// Checks the citations of a text or of an uploaded document. `POST /citation-checks`.
    ///
    /// The call is synchronous: the answer is the finished run. It costs `max(2, ceil(chars / 5000))`
    /// credits. The same text again in the same month costs nothing more. A 429 or 503 is retried
    /// because both guarantee that nothing ran. A dropped connection is not retried; repeating the
    /// call by hand is free in the same period.
    ///
    /// ```no_run
    /// use klaracase::{CreateCitationCheckParams, Klaracase, Verdict};
    ///
    /// # async fn run(client: Klaracase, text: String) -> Result<(), klaracase::Error> {
    /// let run = client
    ///     .citation_checks()
    ///     .create(CreateCitationCheckParams::from_text(text).with_reference_date("2024-05-01"))
    ///     .await?;
    /// for finding in &run.findings {
    ///     match finding.verdict.as_str() {
    ///         Verdict::VERIFIED_CORPUS => println!("exists: {}", finding.canonical),
    ///         Verdict::NOT_FOUND => println!("not in the corpus: {}", finding.raw_text),
    ///         other => println!("{other}: {}", finding.raw_text),
    ///     }
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn create(
        &self,
        params: CreateCitationCheckParams,
    ) -> Result<Response<CitationCheckRun>, Error> {
        let body = params.to_body()?;
        self.client
            .execute(Request::post_json("/citation-checks", &body))
            .await
    }

    /// Reads a run made through the API by your organization. `GET /citation-checks/{run}`. Free.
    pub async fn get(&self, run_id: &str) -> Result<Response<CitationCheckRun>, Error> {
        let path = format!("/citation-checks/{}", path_segment("run_id", run_id)?);
        self.client.execute(Request::get(path, Query::new())).await
    }
}
