//! The error type for every fallible call in this crate.
//!
//! One [`enum@Error`] covers every failure. The API answers errors as RFC 9457
//! `application/problem+json`; the full problem is kept in [`Problem`]. The HTTP status decides the
//! variant. The `status` member in a body is never consulted for that.

use std::fmt;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde_json::{Map, Value};

use crate::common::ApiBudget;
use crate::documents::DocumentReceipt;
use crate::open::{Open, response_struct};
use crate::types::InsufficientCreditsReason;

/// The longest body excerpt kept in [`Problem::detail`] for an answer that is not a problem.
const DETAIL_LIMIT: usize = 1_000;

/// The `code` of a problem built from an answer that was not a problem body.
pub const UNKNOWN_CODE: &str = "unknown";

/// The `code` of a 402 answer that says the credit limit of one API key is used up.
const TOKEN_LIMIT_REACHED_CODE: &str = "token_limit_reached";

/// The `code` of [`Error::ClientValidation`].
pub const CLIENT_VALIDATION_CODE: &str = "client_validation";

response_struct! {
    /// One decision that an ambiguous Aktenzeichen matches. Pass its `id` to read that decision.
    Candidate {
        /// The decision id.
        opt id: String,
        /// The court short form.
        opt court: String,
        /// The decision date, `YYYY-MM-DD`.
        opt date: String,
        /// The Aktenzeichen.
        opt aktenzeichen: String,
    }
}

/// An RFC 9457 problem as the API sends it.
///
/// `type_`, `title`, `status`, `detail` and `code` are the standard members. Every other member of the
/// body stays readable in [`Problem::extra`]. Five members that carry a decision are also typed:
/// [`candidates`](Self::candidates), [`successor`](Self::successor), [`unit`](Self::unit),
/// [`cap`](Self::cap) and [`resets_at`](Self::resets_at). They are `None` when the answer has no such
/// member or it has another type, and they are copies: `extra` still holds them. The members of the
/// 402 answers (`reason`, `top_up_url`, `spent`, `settings_url`) are typed on [`Error`] and stay in
/// `extra` too.
///
/// For an answer that is not a problem (an HTML 502 from a proxy, `{"message": "Server Error"}`, an
/// empty body) the SDK builds a problem with `code = "unknown"` and the raw body text in `detail`, cut
/// to at most 1,000 characters.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Problem {
    /// The `type` member: a name URI such as `https://klaracase.de/docs/api/errors#not_found`.
    /// Treat it as a name. Do not fetch it.
    pub type_: String,
    /// The `title` member.
    pub title: String,
    /// The HTTP status of the answer.
    pub status: u16,
    /// The `detail` member: a sentence for a person.
    pub detail: String,
    /// The machine code, for example `not_found`, `insufficient_credits` or `rate_limited`.
    pub code: String,
    /// The value of the `X-Klaracase-Api-Release` header, if the answer carried it.
    pub api_release: Option<String>,
    /// Every member of the body except `type`, `title`, `status`, `detail` and `code`.
    pub extra: Map<String, Value>,
    /// On 422 `ambiguous_aktenzeichen`: the decisions the Aktenzeichen matches. Call again with one
    /// of their ids.
    pub candidates: Option<Vec<Candidate>>,
    /// On 404 `statute_superseded`: the statute that replaced the one cited, for example `BauGB`.
    pub successor: Option<String>,
    /// On 429 `volume_cap`: the capped unit, `rows` or `documents`. On 402 `token_limit_reached`:
    /// always `credits`.
    pub unit: Option<String>,
    /// On 429 `volume_cap`: the organization's daily cap for that unit. On 402
    /// `token_limit_reached`: the credit limit of the API key for the period.
    pub cap: Option<u64>,
    /// On 429 `volume_cap`: when the daily cap resets, an RFC 3339 time. On 402
    /// `token_limit_reached`: when the credit period of the key ends and its count starts again.
    pub resets_at: Option<String>,
    headers: HeaderMap,
}

impl Problem {
    /// Builds a problem from an error answer.
    ///
    /// A body is a problem when it is a JSON object with a string `code`. Any other body gives
    /// `code = "unknown"`.
    pub(crate) fn from_answer(
        status: u16,
        api_release: Option<String>,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Self {
        let mut problem = Self::from_body(status, api_release, body);
        problem.headers = safe_headers(headers);
        problem
    }

    fn from_body(status: u16, api_release: Option<String>, body: &[u8]) -> Self {
        if let Some((code, mut members)) = problem_members(body) {
            let text = |members: &mut Map<String, Value>, key: &str| match members.remove(key) {
                Some(Value::String(value)) => value,
                Some(other) => {
                    members.insert(key.to_owned(), other);
                    String::new()
                }
                None => String::new(),
            };
            let type_ = text(&mut members, "type");
            let title = text(&mut members, "title");
            let detail = text(&mut members, "detail");
            let body_status = match members.remove("status") {
                Some(Value::Number(n)) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
                Some(other) => {
                    members.insert("status".to_owned(), other);
                    None
                }
                None => None,
            };
            return Self {
                candidates: members
                    .get("candidates")
                    .and_then(|value| serde_json::from_value(value.clone()).ok()),
                successor: members
                    .get("successor")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                unit: members
                    .get("unit")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                cap: members.get("cap").and_then(Value::as_u64),
                resets_at: members
                    .get("resets_at")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                headers: HeaderMap::new(),
                type_: if type_.is_empty() {
                    "about:blank".to_owned()
                } else {
                    type_
                },
                title,
                status: body_status.unwrap_or(status),
                detail,
                code,
                api_release,
                extra: members,
            };
        }
        let text = String::from_utf8_lossy(body);
        Self {
            type_: "about:blank".to_owned(),
            title: reason_phrase(status).to_owned(),
            status,
            detail: text.chars().take(DETAIL_LIMIT).collect(),
            code: UNKNOWN_CODE.to_owned(),
            api_release,
            extra: Map::new(),
            candidates: None,
            successor: None,
            unit: None,
            cap: None,
            resets_at: None,
            headers: HeaderMap::new(),
        }
    }

    /// The response headers of the answer, without any credential header.
    ///
    /// `Authorization`, `Proxy-Authorization` and `Set-Cookie` are never kept.
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Seconds the server asks you to wait: the `retry_after_seconds` member, else the `Retry-After`
    /// header.
    #[must_use]
    pub fn retry_after_seconds(&self) -> Option<u64> {
        self.extra
            .get("retry_after_seconds")
            .and_then(Value::as_u64)
            .or_else(|| {
                self.headers
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(crate::retry::parse_retry_after)
            })
    }

    /// The ability a 403 `token_ability_missing` answer asks for, for example `citation-check`.
    #[must_use]
    pub fn required_ability(&self) -> Option<&str> {
        self.extra.get("required_ability").and_then(Value::as_str)
    }

    /// The field errors of a 422 `validation_failed` answer: a map from field name to a list of
    /// messages.
    #[must_use]
    pub fn errors(&self) -> Option<&Map<String, Value>> {
        self.extra.get("errors").and_then(Value::as_object)
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): {}", self.status, self.code, self.detail)
    }
}

/// Copies the headers of an answer, minus the ones that carry a credential.
fn safe_headers(headers: &HeaderMap) -> HeaderMap {
    let mut kept = headers.clone();
    for name in ["authorization", "proxy-authorization", "set-cookie"] {
        kept.remove(HeaderName::from_static(name));
    }
    kept
}

/// Splits a body into its `code` and its other members, when the body is a problem.
fn problem_members(body: &[u8]) -> Option<(String, Map<String, Value>)> {
    let Ok(Value::Object(mut members)) = serde_json::from_slice::<Value>(body) else {
        return None;
    };
    match members.remove("code") {
        Some(Value::String(code)) => Some((code, members)),
        _ => None,
    }
}

/// A string member of the problem body, or `None` when it is absent or not a string.
fn extra_text(problem: &Problem, key: &str) -> Option<String> {
    problem
        .extra
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn extra_api_budget(problem: &Problem) -> Option<Box<ApiBudget>> {
    problem
        .extra
        .get("api_budget")
        .filter(|value| value.is_object())
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .map(Box::new)
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    }
}

/// Everything that can go wrong in a call.
///
/// The enum is `#[non_exhaustive]`: match the variants you care about and end with `_`.
///
/// Besides the variants it has helpers that cover most handling: [`code`](Error::code),
/// [`status`](Error::status), [`is_retryable`](Error::is_retryable),
/// [`retry_after`](Error::retry_after) and [`headers`](Error::headers).
///
/// It implements `Serialize`, so a program can hand it to a caller as JSON: an object with
/// `code`, `status`, `detail` and `retry_after_seconds`. Members that do not apply are `null`.
///
/// ```
/// use klaracase::Error;
///
/// fn explain(error: &Error) -> String {
///     match error {
///         Error::InsufficientCredits { cost, balance, .. } => {
///             format!("needs {cost:?} credits, has {balance:?}")
///         }
///         Error::RateLimit { retry_after_seconds, .. } => {
///             format!("wait {retry_after_seconds:?} seconds")
///         }
///         Error::NotFound(problem) => format!("not found: {}", problem.detail),
///         other => other.to_string(),
///     }
/// }
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// HTTP 401. The token is missing, expired, revoked or of the wrong kind.
    #[error("authentication failed: {0}")]
    Authentication(Box<Problem>),

    /// HTTP 403. The token lacks an ability, or the organization or two-factor rules refuse it.
    /// [`Problem::required_ability`] names the missing ability.
    #[error("permission denied: {0}")]
    Permission(Box<Problem>),

    /// HTTP 404. The resource does not exist, or it belongs to another organization.
    #[error("not found: {0}")]
    NotFound(Box<Problem>),

    /// HTTP 409. For example `document_unavailable` or `token_org_missing`.
    #[error("conflict: {0}")]
    Conflict(Box<Problem>),

    /// HTTP 422. The server refused the input. [`Problem::errors`] holds the field errors.
    #[error("validation failed: {0}")]
    Validation(Box<Problem>),

    /// HTTP 402 `insufficient_credits`. The organization has too few plan credits, and the prepaid
    /// API budget, if any, does not cover the call. Nothing ran and nothing was charged. Retrying
    /// does not help: stop and report to your user.
    ///
    /// `reason` and `top_up_url` appear only while the prepaid API budget is available. A reason this
    /// crate does not know reads as [`Open::Other`]: treat it like
    /// [`InsufficientCreditsReason::ApiBudgetNotAvailable`].
    #[error("insufficient credits: {problem}")]
    InsufficientCredits {
        /// Credits the call costs.
        cost: Option<i64>,
        /// Plan credits left.
        balance: Option<i64>,
        /// Why the call could not be paid, when the answer says. `None` for an answer without
        /// `reason`.
        reason: Option<Open<InsufficientCreditsReason>>,
        /// The page where an organization admin loads the API budget. For a human admin only: show
        /// it to your user, and never open or follow it from a program or an AI agent. `None` when
        /// the organization may not load a budget.
        top_up_url: Option<String>,
        /// The prepaid API budget as it stands, the same object as `credits.api_budget` on an
        /// answer. The API sends it only while the API budget is available and the organization
        /// holds a budget or may load one. Boxed, like the problem, to keep [`enum@Error`] small. `None`
        /// when the problem has no `api_budget` object.
        api_budget: Option<Box<ApiBudget>>,
        /// The full problem.
        problem: Box<Problem>,
    },

    /// HTTP 402 `token_limit_reached`. The credit limit of this API key for the period is used up.
    /// Nothing ran and nothing was charged. Retrying does not help before `resets_at`: stop and
    /// report to your user.
    ///
    /// The answer has no `cost` and no `balance`. Only an organization admin can change the limit.
    #[error("API key limit reached: {problem}")]
    TokenLimitReached {
        /// The unit of the limit: always `credits`.
        unit: Option<String>,
        /// The credit limit of the key for the period.
        cap: Option<u64>,
        /// The credits the key has used in the period, plan credits and API budget together.
        spent: Option<u64>,
        /// When the credit period of the key ends and its count starts again, an RFC 3339 time as
        /// the server sent it.
        resets_at: Option<String>,
        /// The page where an organization admin changes the limit. For a human admin only: show it
        /// to your user, and never open or follow it from a program or an AI agent.
        settings_url: Option<String>,
        /// The full problem.
        problem: Box<Problem>,
    },

    /// HTTP 429. Either `rate_limited` or `volume_cap`. Nothing ran and nothing was charged.
    #[error("rate limited: {problem}")]
    RateLimit {
        /// Seconds the server asks you to wait.
        retry_after_seconds: Option<u64>,
        /// The full problem.
        problem: Box<Problem>,
    },

    /// HTTP 503. Nothing ran and nothing was charged.
    #[error("service unavailable: {0}")]
    ServiceUnavailable(Box<Problem>),

    /// Any other error status, or a non-problem body on an error status.
    #[error("API error: {0}")]
    Api(Box<Problem>),

    /// The request got no answer: connection refused, DNS, TLS, or a dropped connection.
    #[error("transport error: {0}")]
    Transport(#[source] reqwest::Error),

    /// The request timed out. See [`ClientBuilder::timeout`](crate::ClientBuilder::timeout).
    #[error("request timed out: {0}")]
    Timeout(#[source] reqwest::Error),

    /// A 2xx answer whose body could not be read as the expected JSON.
    ///
    /// The call may have been charged, so the body is kept: `body` holds the first 1,000
    /// characters of what the server sent.
    #[error("could not decode the answer (status {status}): {source}")]
    Decode {
        /// The HTTP status of the answer.
        status: u16,
        /// The first 1,000 characters of the body, so a paid answer is not lost.
        body: String,
        /// The JSON error.
        #[source]
        source: serde_json::Error,
    },

    /// A mistake in how the client was set up, found before any request: no token, an empty token,
    /// an invalid base URL or a bad timeout.
    #[error("configuration error ({code}): {message}")]
    Configuration {
        /// A short stable code, for example `missing_token` or `invalid_base_url`.
        code: &'static str,
        /// A sentence for a person.
        message: String,
    },

    /// A mistake in the arguments of a call, found before any request: a blank query, a date that is
    /// not `YYYY-MM-DD`, both or neither of `text` and `document_id`, no ids, or a path parameter
    /// that cannot be encoded safely. Nothing was sent and nothing was charged.
    ///
    /// The shape is checked, never a number the server owns. The server answers a cap that is too
    /// high with a 422, [`Error::Validation`].
    #[error("invalid input ({code}): {message}")]
    ClientValidation {
        /// Always `client_validation`.
        code: &'static str,
        /// A sentence for a person that names the argument.
        message: String,
    },

    /// [`wait_until_ready`](crate::CitationCheckDocuments::wait_until_ready) saw the upload end in
    /// `failed`. [`DocumentReceipt::error_code`] names the reason.
    #[error("document processing failed: {}", .receipt.error_code_text().unwrap_or("unknown"))]
    DocumentFailed {
        /// The last receipt.
        receipt: Box<DocumentReceipt>,
    },

    /// [`wait_until_ready`](crate::CitationCheckDocuments::wait_until_ready) ran out of time.
    #[error("document was not ready in time (last status: {})", .receipt.status_text())]
    DocumentTimeout {
        /// The last receipt.
        receipt: Box<DocumentReceipt>,
    },
}

impl Error {
    /// The problem the API sent, for every error that came from an answer.
    #[must_use]
    pub fn problem(&self) -> Option<&Problem> {
        match self {
            Self::Authentication(problem)
            | Self::Permission(problem)
            | Self::NotFound(problem)
            | Self::Conflict(problem)
            | Self::Validation(problem)
            | Self::ServiceUnavailable(problem)
            | Self::Api(problem) => Some(problem),
            Self::InsufficientCredits { problem, .. }
            | Self::TokenLimitReached { problem, .. }
            | Self::RateLimit { problem, .. } => Some(problem),
            _ => None,
        }
    }

    /// The machine code of the error.
    ///
    /// For an API error this is the problem `code`. For [`Error::DocumentFailed`] it is the receipt's
    /// `error_code`. For [`Error::DocumentTimeout`] it is `wait_timeout`. For [`Error::Configuration`]
    /// it is the configuration code. Transport errors have none.
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Configuration { code, .. } | Self::ClientValidation { code, .. } => Some(code),
            Self::DocumentFailed { receipt } => receipt.error_code_text(),
            Self::DocumentTimeout { .. } => Some("wait_timeout"),
            other => other.problem().map(|problem| problem.code.as_str()),
        }
    }

    /// The HTTP status, when the server answered.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Decode { status, .. } => Some(*status),
            other => other.problem().map(|problem| problem.status),
        }
    }

    /// The response headers of the answer, for an error that came from one. `Authorization` is never
    /// in it.
    #[must_use]
    pub fn headers(&self) -> Option<&HeaderMap> {
        self.problem().map(Problem::headers)
    }

    /// True when the same call can succeed if you send it again later.
    ///
    /// That holds for [`Error::Transport`], [`Error::Timeout`], [`Error::RateLimit`] and
    /// [`Error::ServiceUnavailable`]. It does not hold for a wrong token, a missing resource, bad
    /// input, too few credits, a document that failed to extract or a body that did not decode.
    /// The client already retried the call as often as [`max_retries`](crate::ClientBuilder::max_retries)
    /// allows before it returned the error.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport(_)
                | Self::Timeout(_)
                | Self::RateLimit { .. }
                | Self::ServiceUnavailable(_)
        )
    }

    /// How long the server asks you to wait before you retry, if it said.
    ///
    /// It reads `retry_after_seconds` from the body, then the `Retry-After` header.
    #[must_use]
    pub fn retry_after(&self) -> Option<Duration> {
        let seconds = match self {
            Self::RateLimit {
                retry_after_seconds,
                ..
            } => *retry_after_seconds,
            other => other.problem().and_then(Problem::retry_after_seconds),
        };
        seconds.map(Duration::from_secs)
    }

    /// The code used when an error is written as JSON. Errors without a server code get a stable
    /// name of their own.
    fn serialized_code(&self) -> &str {
        match self {
            Self::Transport(_) => "transport_error",
            Self::Timeout(_) => "timeout",
            Self::Decode { .. } => "decode_error",
            other => other.code().unwrap_or(UNKNOWN_CODE),
        }
    }

    /// True for [`Error::Transport`] and [`Error::Timeout`].
    #[must_use]
    pub fn is_transport(&self) -> bool {
        matches!(self, Self::Transport(_) | Self::Timeout(_))
    }

    /// Builds the variant for an error answer. The HTTP status decides.
    pub(crate) fn from_problem(
        status: u16,
        problem: Problem,
        retry_after_header: Option<u64>,
    ) -> Self {
        let problem = Box::new(problem);
        match status {
            401 => Self::Authentication(problem),
            402 if problem.code == TOKEN_LIMIT_REACHED_CODE => Self::TokenLimitReached {
                unit: problem.unit.clone(),
                cap: problem.cap,
                spent: problem.extra.get("spent").and_then(Value::as_u64),
                resets_at: problem.resets_at.clone(),
                settings_url: extra_text(&problem, "settings_url"),
                problem,
            },
            402 => Self::InsufficientCredits {
                cost: problem.extra.get("cost").and_then(Value::as_i64),
                balance: problem.extra.get("balance").and_then(Value::as_i64),
                reason: extra_text(&problem, "reason").map(Open::from),
                top_up_url: extra_text(&problem, "top_up_url"),
                api_budget: extra_api_budget(&problem),
                problem,
            },
            403 => Self::Permission(problem),
            404 => Self::NotFound(problem),
            409 => Self::Conflict(problem),
            422 => Self::Validation(problem),
            429 => Self::RateLimit {
                retry_after_seconds: problem
                    .extra
                    .get("retry_after_seconds")
                    .and_then(Value::as_u64)
                    .or(retry_after_header),
                problem,
            },
            503 => Self::ServiceUnavailable(problem),
            _ => Self::Api(problem),
        }
    }

    /// A 2xx answer that does not decode. Keeps the status and the start of the body.
    pub(crate) fn decode(status: u16, body: &[u8], source: serde_json::Error) -> Self {
        Self::Decode {
            status,
            body: String::from_utf8_lossy(body)
                .chars()
                .take(DETAIL_LIMIT)
                .collect(),
            source,
        }
    }

    /// A mistake in the arguments, found before any request.
    pub(crate) fn client_validation(message: impl Into<String>) -> Self {
        Self::ClientValidation {
            code: CLIENT_VALIDATION_CODE,
            message: message.into(),
        }
    }

    /// A client-side mistake.
    pub(crate) fn config(code: &'static str, message: impl Into<String>) -> Self {
        Self::Configuration {
            code,
            message: message.into(),
        }
    }
}

impl Serialize for Error {
    /// Writes `{code, status, detail, retry_after_seconds}`. The detail is the problem `detail` for an
    /// API error, else the message of the error. No header and no token is written.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let detail = match self.problem() {
            Some(problem) => problem.detail.clone(),
            None => self.to_string(),
        };
        let mut out = serializer.serialize_struct("Error", 4)?;
        out.serialize_field("code", self.serialized_code())?;
        out.serialize_field("status", &self.status())?;
        out.serialize_field("detail", &detail)?;
        out.serialize_field(
            "retry_after_seconds",
            &self.retry_after().map(|d| d.as_secs()),
        )?;
        out.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn error(status: u16, body: &Value) -> Error {
        let problem = Problem::from_answer(
            status,
            Some("0.96.0".to_owned()),
            &HeaderMap::new(),
            body.to_string().as_bytes(),
        );
        Error::from_problem(status, problem, None)
    }

    #[test]
    fn insufficient_credits_exposes_reason_and_top_up_url() {
        let error = error(
            402,
            &json!({
                "code": "insufficient_credits", "cost": 1, "balance": 0,
                "reason": "api_budget_empty",
                "top_up_url": "https://klaracase.de/org/billing/api-budget"
            }),
        );
        let Error::InsufficientCredits {
            cost,
            balance,
            reason,
            top_up_url,
            ..
        } = &error
        else {
            panic!("{error:?}");
        };
        assert_eq!((*cost, *balance), (Some(1), Some(0)));
        assert_eq!(
            *reason,
            Some(Open::Known(InsufficientCreditsReason::ApiBudgetEmpty))
        );
        assert_eq!(
            top_up_url.as_deref(),
            Some("https://klaracase.de/org/billing/api-budget")
        );
        assert!(!error.is_retryable());
    }

    #[test]
    fn insufficient_credits_exposes_the_api_budget_object() {
        let error = error(
            402,
            &json!({
                "code": "insufficient_credits", "cost": 1, "balance": 0,
                "reason": "api_budget_empty",
                "api_budget": {"cents": 0, "rate_cents_per_credit": 25, "credits_at_rate": 0}
            }),
        );
        let Error::InsufficientCredits {
            api_budget,
            problem,
            ..
        } = error
        else {
            panic!("{error:?}");
        };
        let budget = api_budget.expect("api_budget");
        assert_eq!(budget.cents, Some(0));
        assert_eq!(budget.rate_cents_per_credit, Some(25));
        assert_eq!(budget.credits_at_rate, Some(0));
        assert_eq!(problem.extra["api_budget"]["cents"], 0);
    }

    #[test]
    fn a_missing_or_wrong_typed_api_budget_reads_none() {
        for body in [
            json!({"code": "insufficient_credits", "cost": 1, "balance": 0}),
            json!({"code": "insufficient_credits", "api_budget": "free"}),
        ] {
            let error = error(402, &body);
            let Error::InsufficientCredits { api_budget, .. } = error else {
                panic!("{error:?}");
            };
            assert_eq!(api_budget, None);
        }
    }

    #[test]
    fn the_old_402_shape_still_reads_with_no_reason_and_no_url() {
        let error = error(
            402,
            &json!({"code": "insufficient_credits", "cost": 2, "balance": 1}),
        );
        let Error::InsufficientCredits {
            reason, top_up_url, ..
        } = error
        else {
            panic!("{error:?}");
        };
        assert_eq!((reason, top_up_url), (None, None));
    }

    #[test]
    fn a_reason_the_crate_does_not_know_stays_a_string() {
        let error = error(
            402,
            &json!({"code": "insufficient_credits", "reason": "plan_on_hold"}),
        );
        let Error::InsufficientCredits { reason, .. } = error else {
            panic!("{error:?}");
        };
        let reason = reason.expect("reason");
        assert_eq!(reason.known(), None);
        assert_eq!(reason.as_str(), "plan_on_hold");
    }

    #[test]
    fn the_reserved_reason_is_known() {
        let reason: Open<InsufficientCreditsReason> = "plan_credits_exhausted".into();
        assert_eq!(reason, InsufficientCreditsReason::PlanCreditsExhausted);
    }

    #[test]
    fn a_reason_that_is_not_a_string_is_ignored() {
        let error = error(402, &json!({"code": "insufficient_credits", "reason": 7}));
        let Error::InsufficientCredits {
            reason, problem, ..
        } = error
        else {
            panic!("{error:?}");
        };
        assert_eq!(reason, None);
        assert_eq!(problem.extra["reason"], 7);
    }

    fn token_limit_body() -> Value {
        json!({
            "type": "https://klaracase.de/docs/api/errors#token_limit_reached",
            "title": "Payment Required", "status": 402, "detail": "This API token reached its limit.",
            "code": "token_limit_reached", "unit": "credits", "cap": 500, "spent": 500,
            "resets_at": "2026-12-01T00:00:00+00:00",
            "settings_url": "https://klaracase.de/org/api-keys"
        })
    }

    #[test]
    fn token_limit_reached_is_its_own_variant_with_typed_members() {
        let error = error(402, &token_limit_body());
        let Error::TokenLimitReached {
            unit,
            cap,
            spent,
            resets_at,
            settings_url,
            problem,
        } = &error
        else {
            panic!("{error:?}");
        };
        assert_eq!(unit.as_deref(), Some("credits"));
        assert_eq!((*cap, *spent), (Some(500), Some(500)));
        assert_eq!(resets_at.as_deref(), Some("2026-12-01T00:00:00+00:00"));
        assert_eq!(
            settings_url.as_deref(),
            Some("https://klaracase.de/org/api-keys")
        );
        assert_eq!(problem.extra["spent"], 500);
        assert_eq!(error.code(), Some("token_limit_reached"));
        assert_eq!(error.status(), Some(402));
        assert!(!error.is_retryable());
        assert_eq!(error.retry_after(), None);
    }

    #[test]
    fn token_limit_reached_has_no_cost_and_no_balance_and_tolerates_missing_members() {
        let error = error(402, &json!({"code": "token_limit_reached"}));
        let Error::TokenLimitReached {
            unit,
            cap,
            spent,
            resets_at,
            settings_url,
            ..
        } = error
        else {
            panic!("{error:?}");
        };
        assert_eq!(
            (unit, cap, spent, resets_at, settings_url),
            (None, None, None, None, None)
        );
    }

    #[test]
    fn token_limit_reached_serializes_like_any_other_error() {
        let value = serde_json::to_value(error(402, &token_limit_body())).unwrap();
        assert_eq!(
            value,
            json!({
                "code": "token_limit_reached", "status": 402,
                "detail": "This API token reached its limit.", "retry_after_seconds": null
            })
        );
    }

    #[test]
    fn the_code_decides_only_for_a_402() {
        let error = error(429, &json!({"code": "token_limit_reached"}));
        assert!(matches!(error, Error::RateLimit { .. }), "{error:?}");
    }
}
