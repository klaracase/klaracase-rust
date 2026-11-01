//! The client, its builder and the request pipeline every operation goes through.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use reqwest::Method;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT};
use reqwest::multipart::{Form, Part};
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use crate::deprecation::{self, DeprecationNotice};
use crate::encode::Query;
use crate::error::{Error, Problem};
use crate::response::{Response, ResponseMeta};
use crate::retry::{self, Clock, MAX_STATED_WAIT_SECONDS, Sleeper, SystemClock};

/// The production base URL of API v1.
pub const DEFAULT_BASE_URL: &str = "https://klaracase.de/api/v1";
/// The environment variable [`Klaracase::from_env`] and the builder read the token from.
pub const TOKEN_ENV_VAR: &str = "KLARACASE_API_TOKEN";
/// The default total timeout of one attempt.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// The default number of retries.
pub const DEFAULT_MAX_RETRIES: u32 = 2;
/// The version of this crate. It is part of the `User-Agent`.
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

type DeprecationHook = Arc<dyn Fn(DeprecationNotice) + Send + Sync>;

/// The Klaracase API client.
///
/// The client is cheap to clone: clones share one connection pool and one configuration. Build it
/// once and reuse it.
///
/// ```no_run
/// use klaracase::{Klaracase, SearchDecisionsParams};
///
/// # async fn run() -> Result<(), klaracase::Error> {
/// let client = Klaracase::new("klc_your_token_here")?;
/// let found = client
///     .decisions()
///     .search(SearchDecisionsParams::new("Vermögensbetreuungspflicht Untreue").court("BGH"))
///     .await?;
/// for hit in &found.results {
///     println!("{:?} {:?}", hit.aktenzeichen, hit.title);
/// }
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct Klaracase {
    inner: Arc<Inner>,
}

struct Inner {
    authorization: HeaderValue,
    base_url: String,
    http: reqwest::Client,
    timeout: Duration,
    max_retries: u32,
    validate_params: bool,
    user_agent: String,
    sleeper: Arc<dyn Sleeper>,
    clock: Arc<dyn Clock>,
    on_deprecation: DeprecationHook,
}

impl fmt::Debug for Klaracase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Klaracase")
            .field("base_url", &self.inner.base_url)
            .field("timeout", &self.inner.timeout)
            .field("max_retries", &self.inner.max_retries)
            .field("user_agent", &self.inner.user_agent)
            .finish_non_exhaustive()
    }
}

impl Klaracase {
    /// Creates a client with the default settings.
    ///
    /// An empty token is a mistake, not a cue to read the environment: this returns
    /// [`Error::Configuration`] and sends no request. Use [`Klaracase::from_env`] to read the token
    /// from `KLARACASE_API_TOKEN`.
    ///
    /// # Errors
    ///
    /// [`Error::Configuration`] when the token is empty or holds characters that cannot go in a
    /// header.
    pub fn new(token: impl Into<String>) -> Result<Self, Error> {
        Self::builder().token(token).build()
    }

    /// Creates a client with the token from the `KLARACASE_API_TOKEN` environment variable. This is
    /// the only constructor that reads the environment.
    ///
    /// # Errors
    ///
    /// [`Error::Configuration`] when the variable is unset or empty.
    pub fn from_env() -> Result<Self, Error> {
        Self::builder().token_from_env().build()
    }

    /// Starts a [`ClientBuilder`].
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// The base URL without a trailing slash.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    /// The `User-Agent` header value the client sends.
    #[must_use]
    pub fn user_agent(&self) -> &str {
        &self.inner.user_agent
    }

    /// Passes a parameter check through, unless client validation is off.
    pub(crate) fn checked(&self, check: Result<(), Error>) -> Result<(), Error> {
        if self.inner.validate_params {
            check
        } else {
            Ok(())
        }
    }

    pub(crate) fn clock(&self) -> &dyn Clock {
        self.inner.clock.as_ref()
    }

    pub(crate) async fn sleep(&self, duration: Duration) {
        self.inner.sleeper.sleep(duration).await;
    }

    /// Sends a request and parses the answer as `T`, applying the retry policy.
    pub(crate) async fn execute<T: DeserializeOwned>(
        &self,
        request: Request,
    ) -> Result<Response<T>, Error> {
        let started = Instant::now();
        let url = self.url_for(&request)?;
        let may_retry_transport = request.method == Method::GET;
        let mut retries_done: u32 = 0;
        loop {
            let can_retry = retries_done < self.inner.max_retries;
            let answer = match self.send_once(&request, url.clone()).await {
                Ok(answer) => answer,
                Err(error) => {
                    if may_retry_transport && can_retry {
                        self.sleep(retry::backoff(retries_done)).await;
                        retries_done += 1;
                        continue;
                    }
                    return Err(if error.is_timeout() {
                        Error::Timeout(error)
                    } else {
                        Error::Transport(error)
                    });
                }
            };

            if let Some(notice) = DeprecationNotice::from_headers(&answer.headers) {
                deprecation::deliver(notice, self.inner.on_deprecation.as_ref());
            }
            let api_release = header_text(&answer.headers, "x-klaracase-api-release");

            if (200..300).contains(&answer.status) {
                let decode_error = |source| Error::decode(answer.status, &answer.body, source);
                let raw: Value = serde_json::from_slice(&answer.body).map_err(decode_error)?;
                let body = T::deserialize(&raw).map_err(decode_error)?;
                let meta = ResponseMeta {
                    status: answer.status,
                    api_release,
                    request_duration: started.elapsed(),
                };
                return Ok(Response::new(body, meta, raw));
            }

            let retry_after_header = header_text(&answer.headers, "retry-after")
                .and_then(|value| retry::parse_retry_after(&value));
            let problem = Problem::from_answer(answer.status, api_release, &answer.body);
            let error = Error::from_problem(answer.status, problem, retry_after_header);

            if !matches!(answer.status, 429 | 503) || !can_retry {
                return Err(error);
            }
            let stated = match &error {
                Error::RateLimit {
                    retry_after_seconds,
                    ..
                } => *retry_after_seconds,
                other => other
                    .problem()
                    .and_then(|p| p.extra.get("retry_after_seconds"))
                    .and_then(Value::as_u64)
                    .or(retry_after_header),
            };
            let wait = match stated {
                Some(seconds) if seconds > MAX_STATED_WAIT_SECONDS => return Err(error),
                Some(seconds) => Duration::from_secs(seconds),
                None => retry::backoff(retries_done),
            };
            self.sleep(wait).await;
            retries_done += 1;
        }
    }

    fn url_for(&self, request: &Request) -> Result<Url, Error> {
        let mut text = format!("{}{}", self.inner.base_url, request.path);
        if let Some(query) = request.query.to_query_string() {
            text.push('?');
            text.push_str(&query);
        }
        Url::parse(&text).map_err(|error| {
            Error::config(
                "invalid_url",
                format!("could not build the request URL: {error}"),
            )
        })
    }

    async fn send_once(&self, request: &Request, url: Url) -> Result<RawAnswer, reqwest::Error> {
        let mut builder = self
            .inner
            .http
            .request(request.method.clone(), url)
            .timeout(self.inner.timeout)
            .header(AUTHORIZATION, self.inner.authorization.clone())
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, self.inner.user_agent.as_str());
        builder = match &request.body {
            Body::None => builder,
            Body::Json(bytes) => builder
                .header(CONTENT_TYPE, "application/json")
                .body(bytes.clone()),
            Body::File {
                bytes,
                filename,
                content_type,
            } => {
                let length = bytes.len() as u64;
                let part = Part::stream_with_length(reqwest::Body::from(bytes.clone()), length)
                    .file_name(filename.clone());
                let part = part.mime_str(content_type)?;
                builder.multipart(Form::new().part("file", part))
            }
        };
        let response = builder.send().await?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response.bytes().await?.to_vec();
        Ok(RawAnswer {
            status,
            headers,
            body,
        })
    }
}

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

struct RawAnswer {
    status: u16,
    headers: HeaderMap,
    body: Vec<u8>,
}

/// What a request carries.
#[derive(Debug, Clone)]
pub(crate) enum Body {
    None,
    Json(Bytes),
    File {
        bytes: Bytes,
        filename: String,
        content_type: String,
    },
}

/// One request, before it is sent. `path` is already encoded and starts with `/`.
#[derive(Debug, Clone)]
pub(crate) struct Request {
    pub(crate) method: Method,
    pub(crate) path: String,
    pub(crate) query: Query,
    pub(crate) body: Body,
}

impl Request {
    pub(crate) fn get(path: impl Into<String>, query: Query) -> Self {
        Self {
            method: Method::GET,
            path: path.into(),
            query,
            body: Body::None,
        }
    }

    pub(crate) fn post_json(path: impl Into<String>, body: &Value) -> Self {
        Self {
            method: Method::POST,
            path: path.into(),
            query: Query::new(),
            body: Body::Json(Bytes::from(body.to_string())),
        }
    }
}

/// Builds a [`Klaracase`] client.
///
/// ```no_run
/// use std::time::Duration;
/// use klaracase::Klaracase;
///
/// # fn run() -> Result<(), klaracase::Error> {
/// let client = Klaracase::builder()
///     .token("klc_your_token_here")
///     .timeout(Duration::from_secs(60))
///     .max_retries(4)
///     .user_agent_suffix("my-app/1.2")
///     .build()?;
/// # Ok(()) }
/// ```
#[derive(Default)]
pub struct ClientBuilder {
    token: Option<String>,
    base_url: Option<String>,
    timeout: Option<Duration>,
    max_retries: Option<u32>,
    client_validation: Option<bool>,
    user_agent_suffix: Option<String>,
    http_client: Option<reqwest::Client>,
    sleeper: Option<Arc<dyn Sleeper>>,
    clock: Option<Arc<dyn Clock>>,
    on_deprecation: Option<DeprecationHook>,
}

impl fmt::Debug for ClientBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientBuilder")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .finish_non_exhaustive()
    }
}

impl ClientBuilder {
    /// Sets the API token. An empty token makes [`build`](Self::build) fail.
    #[must_use]
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Reads the API token from the `KLARACASE_API_TOKEN` environment variable now.
    #[must_use]
    pub fn token_from_env(mut self) -> Self {
        self.token = Some(std::env::var(TOKEN_ENV_VAR).unwrap_or_default());
        self
    }

    /// Sets the base URL. The default is `https://klaracase.de/api/v1`. A trailing slash is removed.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Sets the total timeout of one attempt. The default is 30 seconds.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Sets the number of retries on 429 and 503, and on transport errors for GET. The default is 2.
    /// Zero turns retries off.
    #[must_use]
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    /// Turns the client-side check of arguments on or off. The default is on.
    ///
    /// The check refuses input whose shape is wrong before any request: a blank query, a date that
    /// is not `YYYY-MM-DD`, an inverted date range, no ids or more than ten, both or neither of
    /// `text` and `document_id`. It never enforces a number the server owns. Turn it off to let the
    /// server judge every call, for example to test how it answers an inverted range.
    #[must_use]
    pub fn client_validation(mut self, on: bool) -> Self {
        self.client_validation = Some(on);
        self
    }

    /// Appends a product token such as `my-app/1.2` to the `User-Agent`.
    #[must_use]
    pub fn user_agent_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.user_agent_suffix = Some(suffix.into());
        self
    }

    /// Uses your own `reqwest::Client`, for a proxy, a custom root certificate or a shared pool.
    /// The client still sets the headers, the timeout and the retries.
    #[must_use]
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http_client = Some(client);
        self
    }

    /// Replaces the [`Sleeper`] used between attempts and by `wait_until_ready`. For tests.
    #[must_use]
    pub fn sleeper(mut self, sleeper: Arc<dyn Sleeper>) -> Self {
        self.sleeper = Some(sleeper);
        self
    }

    /// Replaces the [`Clock`] read by the `wait_until_ready` timeout. For tests.
    #[must_use]
    pub fn clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = Some(clock);
        self
    }

    /// Sets the function that runs when the API announces a deprecation.
    ///
    /// The client calls it at most once per process with the raw `Deprecation`, `Sunset` and `Link`
    /// header values. The default hook logs one warning through the `log` crate (feature `log`) or
    /// writes one line to stderr.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use klaracase::Klaracase;
    ///
    /// # fn run() -> Result<(), klaracase::Error> {
    /// let client = Klaracase::builder()
    ///     .token("klc_your_token_here")
    ///     .on_deprecation(Arc::new(|notice| eprintln!("deprecated: {notice:?}")))
    ///     .build()?;
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub fn on_deprecation(mut self, hook: Arc<dyn Fn(DeprecationNotice) + Send + Sync>) -> Self {
        self.on_deprecation = Some(hook);
        self
    }

    /// Builds the client.
    ///
    /// # Errors
    ///
    /// [`Error::Configuration`] when there is no token or it is empty (the message names
    /// `KLARACASE_API_TOKEN`), when the token or the base URL is invalid, or when no [`Sleeper`] is
    /// available because the `tokio` feature is off and none was given.
    pub fn build(self) -> Result<Klaracase, Error> {
        let token = self
            .token
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty())
            .ok_or_else(|| {
                Error::config(
                    "missing_token",
                    format!(
                        "no API token: pass a non-empty one to Klaracase::new, or call \
                         Klaracase::from_env with the {TOKEN_ENV_VAR} environment variable set \
                         (create a token at https://klaracase.de/settings/api-tokens)"
                    ),
                )
            })?;
        let mut authorization =
            HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| {
                Error::config(
                    "invalid_token",
                    "the API token holds characters that cannot go in a header",
                )
            })?;
        authorization.set_sensitive(true);

        let base_url = normalize_base_url(self.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL))?;

        let sleeper = match self.sleeper {
            Some(sleeper) => sleeper,
            None => default_sleeper()?,
        };
        let mut user_agent = format!(
            "klaracase-rust/{SDK_VERSION} rust/{}",
            env!("KLARACASE_RUSTC_VERSION")
        );
        if let Some(suffix) = self
            .user_agent_suffix
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            user_agent.push(' ');
            user_agent.push_str(suffix);
        }
        HeaderValue::from_str(&user_agent).map_err(|_| {
            Error::config(
                "invalid_user_agent_suffix",
                "the user agent suffix holds invalid characters",
            )
        })?;

        let http = match self.http_client {
            Some(client) => client,
            None => reqwest::Client::builder().build().map_err(|error| {
                Error::config(
                    "http_client",
                    format!("could not build the HTTP client: {error}"),
                )
            })?,
        };

        Ok(Klaracase {
            inner: Arc::new(Inner {
                authorization,
                base_url,
                http,
                timeout: self.timeout.unwrap_or(DEFAULT_TIMEOUT),
                max_retries: self.max_retries.unwrap_or(DEFAULT_MAX_RETRIES),
                validate_params: self.client_validation.unwrap_or(true),
                user_agent,
                sleeper,
                clock: self.clock.unwrap_or_else(|| Arc::new(SystemClock)),
                on_deprecation: self
                    .on_deprecation
                    .unwrap_or_else(|| Arc::new(deprecation::default_hook)),
            }),
        })
    }
}

#[cfg(feature = "tokio")]
fn default_sleeper() -> Result<Arc<dyn Sleeper>, Error> {
    Ok(Arc::new(retry::TokioSleeper))
}

#[cfg(not(feature = "tokio"))]
fn default_sleeper() -> Result<Arc<dyn Sleeper>, Error> {
    Err(Error::config(
        "sleeper_required",
        "the `tokio` feature is off: pass a Sleeper to ClientBuilder::sleeper",
    ))
}

fn normalize_base_url(raw: &str) -> Result<String, Error> {
    let trimmed = raw.trim().trim_end_matches('/');
    let invalid = |why: &str| {
        Error::config(
            "invalid_base_url",
            format!("invalid base URL `{raw}`: {why}"),
        )
    };
    let parsed = Url::parse(trimmed).map_err(|error| invalid(&error.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(invalid("the scheme must be http or https"));
    }
    if parsed.host_str().is_none() {
        return Err(invalid("the URL has no host"));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(invalid("the URL must not have a query or a fragment"));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_loses_trailing_slashes() {
        assert_eq!(
            normalize_base_url("http://127.0.0.1:8080/api/v1/").unwrap(),
            "http://127.0.0.1:8080/api/v1"
        );
    }

    #[test]
    fn base_url_must_be_http_or_https() {
        for bad in [
            "",
            "ftp://example.com",
            "not a url",
            "https://example.com/?a=1",
        ] {
            let error = normalize_base_url(bad).unwrap_err();
            assert_eq!(error.code(), Some("invalid_base_url"), "{bad}");
        }
    }

    #[test]
    fn debug_output_hides_the_token() {
        let client = Klaracase::new("klc_secret_value").unwrap();
        assert!(!format!("{client:?}").contains("klc_secret_value"));
    }

    #[test]
    fn user_agent_names_sdk_and_runtime() {
        let client = Klaracase::builder()
            .token("t")
            .user_agent_suffix("acme/2")
            .build()
            .unwrap();
        assert!(
            client
                .user_agent()
                .starts_with("klaracase-rust/0.1.0 rust/")
        );
        assert!(client.user_agent().ends_with(" acme/2"));
    }
}
