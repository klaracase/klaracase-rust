//! Citation checks: `POST /citation-checks` and `GET /citation-checks/{run}`.
//!
//! A citation check looks up every court-decision and statute citation of a text in the corpus and
//! answers one verdict per citation.
//!
//! A `not_found` verdict means the citation was not found in the corpus. It is not a claim that the
//! citation is fabricated. `verified_corpus` proves that the cited decision or statute exists in the
//! corpus. It does not prove that a quote or a legal proposition is correct.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::{Klaracase, Request};
use crate::common::Credits;
use crate::common::slice_code_points;
use crate::documents::{UploadParams, WaitParams};
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::open::{Open, response_struct};
use crate::params::{check_date, invalid, option_setters, require_text};
use crate::response::Response;
use crate::types::{ReferenceType, RunStatus, Verdict};

/// Parameters of [`CitationChecks::create`]. Send exactly one of `text` and `document_id`.
///
/// Giving both or neither fails on the client with [`Error::ClientValidation`] before any request.
///
/// ```
/// use klaracase::CreateCitationCheckParams;
///
/// let params = CreateCitationCheckParams::from_text("Siehe BGH, Urteil vom 15.11.2001 - 1 StR 185/01.")
///     .reference_date("2024-05-01");
/// assert!(params.validate().is_ok());
///
/// let both = CreateCitationCheckParams::from_text("a").document_id("b");
/// assert!(both.validate().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CreateCitationCheckParams {
    /// The German legal text. The server requires 50 to 20,000 characters. Offsets in the answer are
    /// Unicode code point offsets into this exact string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The id of an upload with status `ready`. The check consumes the upload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// The date of the document, `YYYY-MM-DD`. It applies to statute citations whose sentence names
    /// no date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_date: Option<String>,
}

impl CreateCitationCheckParams {
    /// Parameters that check a text.
    #[must_use]
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            document_id: None,
            reference_date: None,
        }
    }

    /// Parameters that check an uploaded document.
    #[must_use]
    pub fn from_document(document_id: impl Into<String>) -> Self {
        Self {
            text: None,
            document_id: Some(document_id.into()),
            reference_date: None,
        }
    }

    option_setters! {
        direct {
            /// Sets the text to check. Use it or `document_id`, never both.
            text text,
            /// Sets the upload to check. Use it or `text`, never both.
            text document_id,
            /// Sets the document date (`YYYY-MM-DD`).
            text reference_date,
        }
    }

    /// Checks the shape of the input without any request: exactly one of `text` and `document_id`,
    /// neither of them blank, and a `reference_date` formatted `YYYY-MM-DD`.
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] with a message that names the argument.
    pub fn validate(&self) -> Result<(), Error> {
        match (&self.text, &self.document_id) {
            (Some(_), Some(_)) => {
                return Err(invalid("send either `text` or `document_id`, not both"));
            }
            (None, None) => return Err(invalid("send either `text` or `document_id`")),
            (Some(text), None) => require_text("text", text)?,
            (None, Some(document_id)) => require_text("document_id", document_id)?,
        }
        check_date("reference_date", self.reference_date.as_deref())
    }

    fn to_body(&self) -> Value {
        serde_json::to_value(self).expect("plain parameter structs always serialize")
    }
}

response_struct! {
    /// How far a run has got.
    CheckProgress {
        /// Citations found in the text.
        opt citations_total: u64,
        /// Citations checked so far.
        opt citations_checked: u64,
        /// Distinct citations found.
        opt unique_citations_total: u64,
    }
}

response_struct! {
    /// How many citations ended in one reason category.
    ReasonCount {
        /// The reason category.
        opt category: String,
        /// How many citations.
        opt count: u64,
    }
}

response_struct! {
    /// One distinct citation, with all its spellings in the text.
    UniqueCitation {
        /// What the key is built from.
        opt key_basis: String,
        /// The matched artifact, if any.
        opt target_artifact_id: String,
        /// The canonical form.
        opt canonical: String,
        /// Every spelling found in the text.
        list spellings: String,
        /// How often it occurs.
        opt occurrences: u64,
        /// The verdict.
        opt verdict: Open<Verdict>,
        /// A decision or a statute.
        opt reference_type: Open<ReferenceType>,
    }
}

response_struct! {
    /// The decision a finding matched in the corpus.
    MatchedDecision {
        /// The artifact id.
        req artifact_id: String,
        /// The ECLI.
        opt ecli: String,
        /// The court short form.
        opt court: String,
        /// The decision date.
        opt date: String,
    }
}

response_struct! {
    /// What the check could read to decide a subdivision such as an Absatz.
    CheckScope {
        /// The subdivision.
        opt subdivision: String,
        /// Whether the article text was available.
        opt article_text_available: bool,
        /// Whether the article text was read.
        opt article_text_read: bool,
        /// The court short form.
        opt court: String,
    }
}

response_struct! {
    /// The evidence behind a verdict.
    Evidence {
        /// The reason.
        opt reason: String,
        /// How strong the match is.
        opt match_strength: String,
        /// Where the citation date came from.
        opt citation_date_source: String,
    }
}

response_struct! {
    /// One citation found in the text, with its verdict.
    Finding {
        /// The finding id.
        req id: String,
        /// The text as written.
        opt raw_text: String,
        /// The canonical form.
        opt canonical: String,
        /// A decision or a statute.
        #[serde(rename = "type")]
        opt reference_type: Open<ReferenceType>,
        /// The role of the citation in the text.
        opt role: String,
        /// The verdict.
        opt verdict: Open<Verdict>,
        /// The status of the external check.
        opt external_status: String,
        /// Where `raw_text` starts in the submitted text, in Unicode code points, not bytes.
        opt start_offset: u64,
        /// Where `raw_text` ends, in Unicode code points, not bytes.
        opt end_offset: u64,
        /// The index of this occurrence among the equal citations.
        opt occurrence_index: u64,
        /// The API has always sent `null`, so the type stays `Value` until a sample exists.
        opt confidence: Value,
        /// The matched decision.
        opt matched_decision: MatchedDecision,
        /// The result of the subdivision check.
        opt subdivision_check: String,
        /// A German advisory about the subdivision.
        opt subdivision_advisory: String,
        /// What the check could read.
        opt check_scope: CheckScope,
        /// A German advisory about ambiguity.
        opt ambiguity_advisory: String,
        /// The API has always sent `null`, so the type stays `Value` until a sample exists.
        opt coverage: Value,
        /// The API has always sent `null`, so the type stays `Value` until a sample exists.
        opt coverage_boundary: Value,
        /// A German advisory about coverage.
        opt coverage_advisory: String,
        /// The evidence behind the verdict.
        opt evidence: Evidence,
        /// When the verdict was disputed.
        opt disputed_at: String,
    }
}

impl Finding {
    /// The text this finding covers, cut from the text you submitted.
    ///
    /// Offsets count Unicode code points, not bytes, so this walks characters. It returns `None`
    /// when the finding has no offsets or they are empty.
    #[must_use]
    pub fn excerpt(&self, text: &str) -> Option<String> {
        slice_code_points(text, self.start_offset?, self.end_offset?)
    }
}

response_struct! {
    /// What extraction found and dropped.
    Extraction {
        /// Candidates detected.
        opt detected: u64,
        /// Candidates dropped.
        opt dropped: u64,
        /// Dropped as unreadable.
        opt dropped_unreadable: u64,
        /// Dropped as truncated.
        opt dropped_truncation: u64,
        /// Dropped for a trailing noun.
        opt dropped_trailing_noun: u64,
        /// Dropped for a stacked subdivision.
        opt dropped_stacked_subdivision: u64,
        /// Dropped as a refused collision.
        opt dropped_refused_collision: u64,
        /// Drops by token. The API has never sent an element, so the element type stays `Value`.
        list dropped_by_token: Value,
        /// The unreadable texts. The API has never sent an element, so the element type stays
        /// `Value`.
        list dropped_unreadable_texts: Value,
        /// Refused collisions. The API has never sent an element, so the element type stays `Value`.
        list refused_contract_collision: Value,
        /// Citations that cannot be verified.
        opt not_verifiable: u64,
        /// Those, by type. The API has never sent an element, so the element type stays `Value`.
        list not_verifiable_by_type: Value,
        /// Possible citations that were not parsed.
        opt possible_citations_unparsed: u64,
        /// Those texts. The API has never sent an element, so the element type stays `Value`.
        list possible_citations_unparsed_texts: Value,
    }
}

response_struct! {
    /// A citation check run: the answer of both `create` and `get`.
    CitationCheckRun {
        /// The run id.
        req id: String,
        /// The state of the run.
        opt status: Open<RunStatus>,
        /// Where the text came from: `text` or `document`.
        opt source: String,
        /// How far the run has got.
        opt progress: CheckProgress,
        /// How many distinct citations got each verdict, keyed by the verdict as the server spelled
        /// it. Read it with [`CitationCheckRun::verdict_count`].
        dflt verdict_counts: BTreeMap<String, u64>,
        /// Whether the run is partial.
        opt partial: bool,
        /// Citations without a final verdict.
        opt unverified_count: u64,
        /// Counts by reason category.
        list reason_counts: ReasonCount,
        /// The distinct citations.
        list unique_citations: UniqueCitation,
        /// What extraction found and dropped.
        opt extraction: Extraction,
        /// Notes. The API has never sent an element, so the element type stays `Value`.
        list notes: Value,
        /// Why findings are not served.
        opt findings_unavailable_reason: String,
        /// When findings were purged.
        opt findings_purged_at: String,
        /// One finding per citation found.
        list findings: Finding,
        /// When the run was submitted.
        opt submitted_at: String,
        /// When the run completed.
        opt completed_at: String,
        /// When the run was created.
        opt created_at: String,
        /// True when the payload was cut to fit the response budget.
        opt payload_truncated: bool,
        /// The API version of the answer.
        opt api_version: String,
        /// Link to the changelog.
        opt changelog_url: String,
        /// What the call cost.
        opt credits: Credits,
    }
}

impl CitationCheckRun {
    /// How many distinct citations got a verdict. `0` when none did.
    #[must_use]
    pub fn verdict_count(&self, verdict: Verdict) -> u64 {
        self.verdict_counts
            .get(verdict.as_str())
            .copied()
            .unwrap_or(0)
    }

    /// The findings grouped by verdict, unknown verdicts included. A finding with no verdict is left
    /// out.
    ///
    /// ```
    /// # fn run(run: klaracase::CitationCheckRun) {
    /// use klaracase::Verdict;
    /// for (verdict, findings) in run.findings_by_verdict() {
    ///     println!("{verdict}: {}", findings.len());
    /// }
    /// let missing = run.findings_by_verdict().remove(&Verdict::NotFound.into());
    /// # let _ = missing;
    /// # }
    /// ```
    #[must_use]
    pub fn findings_by_verdict(&self) -> BTreeMap<Open<Verdict>, Vec<&Finding>> {
        let mut groups: BTreeMap<Open<Verdict>, Vec<&Finding>> = BTreeMap::new();
        for finding in &self.findings {
            if let Some(verdict) = &finding.verdict {
                groups.entry(verdict.clone()).or_default().push(finding);
            }
        }
        groups
    }
}

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

impl<'a> CitationChecks<'a> {
    /// Starts a check of a file on disk: upload, wait until the text is extracted, then check.
    ///
    /// Chain [`timeout`](CheckFileRequest::timeout) and
    /// [`reference_date`](CheckFileRequest::reference_date) if you need them, then call
    /// [`send`](CheckFileRequest::send). The upload and the wait are free. Only the check costs
    /// credits, `max(2, ceil(chars / 5000))`, and it runs only when the upload is `ready`.
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use klaracase::Klaracase;
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let run = client
    ///     .citation_checks()
    ///     .check_file("schriftsatz.pdf")
    ///     .timeout(Duration::from_secs(90))
    ///     .reference_date("2024-05-01")
    ///     .send()
    ///     .await?;
    /// println!("{} findings", run.findings.len());
    /// # Ok(()) }
    /// ```
    pub fn check_file(&self, path: impl Into<std::path::PathBuf>) -> CheckFileRequest<'a> {
        CheckFileRequest {
            client: self.client,
            path: path.into(),
            wait: WaitParams::default(),
            reference_date: None,
        }
    }

    /// Checks the citations of a text or of an uploaded document. `POST /citation-checks`.
    ///
    /// The call is synchronous: the answer is the finished run. It costs `max(2, ceil(chars / 5000))`
    /// credits. The same text again in the same month costs nothing more. A 429 or 503 is retried
    /// because both guarantee that nothing ran. A dropped connection is not retried; repeating the
    /// call by hand is free in the same period.
    ///
    /// ```no_run
    /// use klaracase::{CreateCitationCheckParams, Klaracase, Open, Verdict};
    ///
    /// # async fn run(client: Klaracase, text: String) -> Result<(), klaracase::Error> {
    /// let run = client
    ///     .citation_checks()
    ///     .create(CreateCitationCheckParams::from_text(text).reference_date("2024-05-01"))
    ///     .await?;
    /// for finding in &run.findings {
    ///     match finding.verdict.as_ref().and_then(Open::known) {
    ///         Some(Verdict::VerifiedCorpus) => println!("exists: {:?}", finding.canonical),
    ///         Some(Verdict::NotFound) => println!("not in the corpus: {:?}", finding.raw_text),
    ///         other => println!("{other:?}: {:?}", finding.raw_text),
    ///     }
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn create(
        &self,
        params: CreateCitationCheckParams,
    ) -> Result<Response<CitationCheckRun>, Error> {
        self.client.checked(params.validate())?;
        let body = params.to_body();
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

/// A check of a file under construction. Get it with [`CitationChecks::check_file`].
#[derive(Debug, Clone)]
#[must_use = "a file check does nothing until you call send"]
pub struct CheckFileRequest<'a> {
    client: &'a Klaracase,
    path: std::path::PathBuf,
    wait: WaitParams,
    reference_date: Option<String>,
}

impl CheckFileRequest<'_> {
    /// Sets how long to wait for the text to be extracted. The default is 120 seconds.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.wait = self.wait.timeout(timeout);
        self
    }

    /// Sets the time between polls while the text is extracted. The default is 2 seconds.
    pub fn poll_interval(mut self, poll_interval: Duration) -> Self {
        self.wait = self.wait.poll_interval(poll_interval);
        self
    }

    /// Sets the document date (`YYYY-MM-DD`).
    pub fn reference_date(mut self, reference_date: impl Into<String>) -> Self {
        self.reference_date = Some(reference_date.into());
        self
    }

    /// Reads the file, uploads it, waits until it is `ready` and checks it.
    ///
    /// The date and the file are checked before any request. If the wait ends in
    /// [`Error::DocumentFailed`] or [`Error::DocumentTimeout`], no check runs and nothing is
    /// charged. The upload stays on the server and expires after 48 hours if it is not used.
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] for a file that cannot be read or a bad date, then any error of
    /// the three calls.
    pub async fn send(self) -> Result<Response<CitationCheckRun>, Error> {
        let mut params = CreateCitationCheckParams::from_document("pending");
        params.reference_date = self.reference_date.clone();
        self.client.checked(params.validate())?;
        let upload = UploadParams::from_path(&self.path)?;

        let documents = self.client.citation_check_documents();
        let receipt = documents.upload(upload).await?;
        let ready = documents
            .wait_until_ready_with(&receipt.id, self.wait)
            .await?;

        params.document_id = Some(ready.id.clone());
        self.client.citation_checks().create(params).await
    }
}
