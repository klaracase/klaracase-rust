//! Uploads for the citation check: `/citation-check-documents`.

use std::time::Duration;

use bytes::Bytes;

use crate::client::{Body, Klaracase, Request};
use crate::common::Credits;
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::open::{Open, response_struct};
use crate::params::require_text;
use crate::response::Response;
use crate::types::{DocumentErrorCode, DocumentStatus};
use reqwest::Method;
use reqwest::multipart::Part;

response_struct! {
    /// The receipt of an upload. It never holds the extracted text.
    DocumentReceipt {
        /// The upload id. Pass it as `document_id` to a citation check.
        req id: String,
        /// The file name you sent.
        opt filename: String,
        /// The detected format: `pdf`, `docx`, `md` or `txt`.
        opt format: String,
        /// The state of the upload.
        opt status: Open<DocumentStatus>,
        /// Characters of extracted text. `None` until extraction ends.
        opt char_count: u64,
        /// Pages of a PDF.
        opt page_count: u64,
        /// Pages of a PDF without a text layer.
        opt pages_without_text_layer: u64,
        /// The longest text one check takes.
        opt check_max_chars: u64,
        /// Credits a check of this upload costs. `None` above 20,000 characters.
        opt quoted_credits: u64,
        /// Why extraction failed.
        opt error_code: Open<DocumentErrorCode>,
        /// When an unused upload expires.
        opt expires_at: String,
        /// When the upload was created.
        opt created_at: String,
        /// The API version of the answer.
        opt api_version: String,
        /// Link to the changelog.
        opt changelog_url: String,
        /// What the call cost.
        opt credits: Credits,
    }
}

impl DocumentReceipt {
    /// True when the text is extracted and a check can consume the upload.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.status == Some(Open::Known(DocumentStatus::Ready))
    }

    /// True when extraction failed. [`error_code`](Self::error_code) says why.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.status == Some(Open::Known(DocumentStatus::Failed))
    }

    /// The failure reason as the server spelled it, if the upload failed.
    #[must_use]
    pub fn error_code_text(&self) -> Option<&str> {
        self.error_code.as_ref().map(Open::as_str)
    }

    /// The status as the server spelled it, or `unknown` when the key was absent.
    #[must_use]
    pub fn status_text(&self) -> &str {
        self.status.as_ref().map_or("unknown", Open::as_str)
    }
}

/// Parameters of [`CitationCheckDocuments::upload`].
///
/// The content is reference counted, so a retry of the upload does not copy the buffer.
#[derive(Clone, PartialEq, Eq)]
pub struct UploadParams {
    /// The file content. PDF, DOCX, Markdown or plain text, up to 25 MB.
    bytes: Bytes,
    /// The file name, for example `schriftsatz.pdf`.
    pub filename: String,
    /// The media type, for example `application/pdf`.
    pub content_type: String,
}

impl UploadParams {
    /// Creates upload parameters.
    #[must_use]
    pub fn new(
        bytes: impl Into<Bytes>,
        filename: impl Into<String>,
        content_type: impl Into<String>,
    ) -> Self {
        Self {
            bytes: bytes.into(),
            filename: filename.into(),
            content_type: content_type.into(),
        }
    }

    /// Reads a file from disk and guesses its name and media type.
    ///
    /// The file name is the last part of the path. The media type comes from the extension:
    /// `pdf` is `application/pdf`, `docx` is the Word media type, `md` is `text/markdown` and `txt`
    /// is `text/plain`. Any other extension is refused, because a wrong media type would only make
    /// the server refuse the upload later. Use [`UploadParams::new`] for those.
    ///
    /// The read is synchronous (`std::fs::read`). It blocks the thread for the time the disk takes.
    ///
    /// ```no_run
    /// use klaracase::UploadParams;
    ///
    /// # fn run() -> Result<(), klaracase::Error> {
    /// let params = UploadParams::from_path("schriftsatz.pdf")?;
    /// assert_eq!(params.content_type, "application/pdf");
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] when the file cannot be read or has an extension this function
    /// does not know.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let content_type = match extension.as_str() {
            "pdf" => "application/pdf",
            "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "md" => "text/markdown",
            "txt" => "text/plain",
            _ => {
                return Err(Error::client_validation(format!(
                    "cannot guess the media type of `{}`: use a .pdf, .docx, .md or .txt file, or \
                     build the upload with UploadParams::new",
                    path.display()
                )));
            }
        };
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bytes = std::fs::read(path).map_err(|error| {
            Error::client_validation(format!("cannot read `{}`: {error}", path.display()))
        })?;
        Ok(Self::new(bytes, filename, content_type))
    }

    /// The file content.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Checks the shape of the input without any request: a file name that is not blank.
    ///
    /// # Errors
    ///
    /// [`Error::ClientValidation`] with a message that names the argument.
    pub fn validate(&self) -> Result<(), Error> {
        require_text("filename", &self.filename)
    }
}

impl std::fmt::Debug for UploadParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadParams")
            .field("bytes", &format_args!("{} bytes", self.bytes.len()))
            .field("filename", &self.filename)
            .field("content_type", &self.content_type)
            .finish()
    }
}

/// Parameters of [`CitationCheckDocuments::wait_until_ready_with`].
///
/// ```
/// use std::time::Duration;
/// use klaracase::WaitParams;
///
/// let params = WaitParams::default()
///     .poll_interval(Duration::from_secs(1))
///     .timeout(Duration::from_secs(30));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct WaitParams {
    /// Time between polls. The default is 2 seconds.
    pub poll_interval: Duration,
    /// Total time to wait. The default is 120 seconds.
    pub timeout: Duration,
}

impl Default for WaitParams {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
            timeout: DEFAULT_WAIT_TIMEOUT,
        }
    }
}

impl WaitParams {
    /// Sets the time between polls.
    #[must_use]
    pub fn poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    /// Sets the total time to wait.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// The default time between polls of an upload.
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// The default total time to wait for an upload.
const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(120);

/// The upload operations. Get it with [`Klaracase::citation_check_documents`].
#[derive(Debug, Clone, Copy)]
pub struct CitationCheckDocuments<'a> {
    client: &'a Klaracase,
}

impl Klaracase {
    /// The upload operations for the citation check.
    #[must_use]
    pub fn citation_check_documents(&self) -> CitationCheckDocuments<'_> {
        CitationCheckDocuments { client: self }
    }
}

impl CitationCheckDocuments<'_> {
    /// Uploads a file. `POST /citation-check-documents`, multipart field `file`. The answer is 202
    /// with a receipt. The text is extracted in the background. Free.
    ///
    /// A 429 or 503 is retried. A dropped connection is not.
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use klaracase::{Klaracase, UploadParams};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let receipt = client
    ///     .citation_check_documents()
    ///     .upload(UploadParams::from_path("schriftsatz.pdf")?)
    ///     .await?;
    /// let ready = client
    ///     .citation_check_documents()
    ///     .wait_until_ready(&receipt.id, Duration::from_secs(120))
    ///     .await?;
    /// println!("{:?} credits quoted", ready.quoted_credits);
    /// # Ok(()) }
    /// ```
    pub async fn upload(&self, params: UploadParams) -> Result<Response<DocumentReceipt>, Error> {
        self.client.checked(params.validate())?;
        Part::bytes(Vec::new())
            .mime_str(&params.content_type)
            .map_err(|_| {
                Error::client_validation(format!(
                    "`{}` is not a valid media type",
                    params.content_type
                ))
            })?;
        let request = Request {
            method: Method::POST,
            path: "/citation-check-documents".to_owned(),
            query: Query::new(),
            body: Body::File {
                bytes: params.bytes,
                filename: params.filename,
                content_type: params.content_type,
            },
        };
        self.client.execute(request).await
    }

    /// Reads the receipt of an upload. `GET /citation-check-documents/{document}`. Free.
    pub async fn get(&self, document_id: &str) -> Result<Response<DocumentReceipt>, Error> {
        let path = format!(
            "/citation-check-documents/{}",
            path_segment("document_id", document_id)?
        );
        self.client.execute(Request::get(path, Query::new())).await
    }

    /// Polls [`get`](Self::get) every 2 seconds until the upload is `ready`, for at most `timeout`.
    ///
    /// See [`wait_until_ready_with`](Self::wait_until_ready_with) for the rules.
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use klaracase::Klaracase;
    ///
    /// # async fn run(client: Klaracase, id: String) -> Result<(), klaracase::Error> {
    /// let ready = client
    ///     .citation_check_documents()
    ///     .wait_until_ready(&id, Duration::from_secs(60))
    ///     .await?;
    /// println!("{:?} credits quoted", ready.quoted_credits);
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    ///
    /// [`Error::DocumentFailed`], [`Error::DocumentTimeout`], or any error a poll returns.
    pub async fn wait_until_ready(
        &self,
        document_id: &str,
        timeout: Duration,
    ) -> Result<Response<DocumentReceipt>, Error> {
        self.wait_until_ready_with(document_id, WaitParams::default().timeout(timeout))
            .await
    }

    /// Polls [`get`](Self::get) until the upload is `ready`.
    ///
    /// `uploaded`, `extracting` and `ocr_pending` keep the loop going. It returns the receipt when the
    /// status is `ready`. When extraction ends in `failed` it returns [`Error::DocumentFailed`]. When
    /// the timeout passes first it returns [`Error::DocumentTimeout`] with the last receipt. An error
    /// answer while polling returns the matching error at once.
    ///
    /// # Errors
    ///
    /// As described above.
    pub async fn wait_until_ready_with(
        &self,
        document_id: &str,
        params: WaitParams,
    ) -> Result<Response<DocumentReceipt>, Error> {
        let started = self.client.clock().now();
        loop {
            let response = self.get(document_id).await?;
            if response.is_ready() {
                return Ok(response);
            }
            if response.is_failed() {
                return Err(Error::DocumentFailed {
                    receipt: Box::new(response.into_body()),
                });
            }
            if self.client.clock().now().saturating_duration_since(started) >= params.timeout {
                return Err(Error::DocumentTimeout {
                    receipt: Box::new(response.into_body()),
                });
            }
            self.client.sleep(params.poll_interval).await;
        }
    }
}
