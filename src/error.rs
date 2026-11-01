//! The error type for every fallible call in this crate.
//!
//! One [`enum@Error`] covers every failure. The API answers errors as RFC 9457
//! `application/problem+json`; the full problem is kept in [`Problem`]. The HTTP status decides the
//! variant. The `status` member in a body is never consulted for that.

use std::fmt;

use serde_json::{Map, Value};

use crate::documents::DocumentReceipt;

/// The longest body excerpt kept in [`Problem::detail`] for an answer that is not a problem.
const DETAIL_LIMIT: usize = 1_000;

/// The `code` of a problem built from an answer that was not a problem body.
pub const UNKNOWN_CODE: &str = "unknown";

/// An RFC 9457 problem as the API sends it.
///
/// `type_`, `title`, `status`, `detail` and `code` are the standard members. Every other member of the
/// body, typed ones such as `cost` included, stays readable in [`Problem::extra`].
///
/// For an answer that is not a problem (an HTML 502 from a proxy, `{"message": "Server Error"}`, an
/// empty body) the SDK builds a problem with `code = "unknown"` and the raw body text in `detail`, cut
/// to at most 1,000 characters.
#[derive(Debug, Clone, PartialEq)]
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
}

impl Problem {
    /// Builds a problem from an error answer.
    ///
    /// A body is a problem when it is a JSON object with a string `code`. Any other body gives
    /// `code = "unknown"`.
    pub(crate) fn from_answer(status: u16, api_release: Option<String>, body: &[u8]) -> Self {
        if let Ok(Value::Object(mut members)) = serde_json::from_slice::<Value>(body)
            && let Some(Value::String(code)) = members.remove("code")
        {
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
        }
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
    /// 401. The token is missing, expired, revoked or of the wrong kind.
    #[error("authentication failed: {0}")]
    Authentication(Box<Problem>),

    /// 403. The token lacks an ability, or the organization or two-factor rules refuse it.
    /// [`Problem::required_ability`] names the missing ability.
    #[error("permission denied: {0}")]
    Permission(Box<Problem>),

    /// 404. The resource does not exist, or it belongs to another organization.
    #[error("not found: {0}")]
    NotFound(Box<Problem>),

    /// 409. For example `document_unavailable` or `token_org_missing`.
    #[error("conflict: {0}")]
    Conflict(Box<Problem>),

    /// 422. The server refused the input. [`Problem::errors`] holds the field errors.
    #[error("validation failed: {0}")]
    Validation(Box<Problem>),

    /// 402. The organization has too few credits. Retrying does not help.
    #[error("insufficient credits: {problem}")]
    InsufficientCredits {
        /// Credits the call costs.
        cost: Option<i64>,
        /// Credits left.
        balance: Option<i64>,
        /// The full problem.
        problem: Box<Problem>,
    },

    /// 429. Either `rate_limited` or `volume_cap`. Nothing ran and nothing was charged.
    #[error("rate limited: {problem}")]
    RateLimit {
        /// Seconds the server asks you to wait.
        retry_after_seconds: Option<u64>,
        /// The full problem.
        problem: Box<Problem>,
    },

    /// 503. Nothing ran and nothing was charged.
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
    #[error("could not decode the answer (status {status}): {source}")]
    Decode {
        /// The HTTP status of the answer.
        status: u16,
        /// The JSON error.
        #[source]
        source: serde_json::Error,
    },

    /// A client-side mistake found before any request: no token, an invalid base URL, both `text` and
    /// `document_id`, or a path parameter that cannot be encoded safely.
    #[error("configuration error ({code}): {message}")]
    Configuration {
        /// A short stable code, for example `missing_token` or `client_validation`.
        code: &'static str,
        /// A sentence for a person.
        message: String,
    },

    /// [`wait_until_ready`](crate::CitationCheckDocuments::wait_until_ready) saw the upload end in
    /// `failed`. [`DocumentReceipt::error_code`] names the reason.
    #[error("document processing failed: {}", .receipt.error_code.as_deref().unwrap_or("unknown"))]
    DocumentFailed {
        /// The last receipt.
        receipt: Box<DocumentReceipt>,
    },

    /// [`wait_until_ready`](crate::CitationCheckDocuments::wait_until_ready) ran out of time.
    #[error("document was not ready in time (last status: {})", .receipt.status)]
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
            Self::InsufficientCredits { problem, .. } | Self::RateLimit { problem, .. } => {
                Some(problem)
            }
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
            Self::Configuration { code, .. } => Some(code),
            Self::DocumentFailed { receipt } => receipt.error_code.as_deref(),
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
            402 => Self::InsufficientCredits {
                cost: problem.extra.get("cost").and_then(Value::as_i64),
                balance: problem.extra.get("balance").and_then(Value::as_i64),
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

    /// A client-side mistake.
    pub(crate) fn config(code: &'static str, message: impl Into<String>) -> Self {
        Self::Configuration {
            code,
            message: message.into(),
        }
    }
}
