//! Uploads for the citation check: `/citation-check-documents`.

use std::time::Duration;

use reqwest::Method;
use reqwest::multipart::Part;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::{Body, Klaracase, Request};
use crate::common::Credits;
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::response::{Response, impl_extras};

/// The receipt of an upload. It never holds the extracted text.
///
/// The `status` is one of the [`DocumentStatus`] constants. Unknown values are kept as they are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentReceipt {
    /// The upload id. Pass it as `document_id` to a citation check.
    pub id: String,
    /// The file name you sent.
    #[serde(default)]
    pub filename: String,
    /// The detected format: `pdf`, `docx`, `md` or `txt`.
    #[serde(default)]
    pub format: String,
    /// See [`DocumentStatus`].
    #[serde(default)]
    pub status: String,
    /// Characters of extracted text. `None` until extraction ends.
    #[serde(default)]
    pub char_count: Option<u64>,
    /// Pages of a PDF.
    #[serde(default)]
    pub page_count: Option<u64>,
    /// Pages of a PDF without a text layer.
    #[serde(default)]
    pub pages_without_text_layer: Option<u64>,
    /// The longest text one check takes.
    #[serde(default)]
    pub check_max_chars: Option<u64>,
    /// Credits a check of this upload costs. `None` above 20,000 characters.
    #[serde(default)]
    pub quoted_credits: Option<u64>,
    /// Why extraction failed, for example `encrypted_pdf`.
    #[serde(default)]
    pub error_code: Option<String>,
    /// When an unused upload expires.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// When the upload was created.
    #[serde(default)]
    pub created_at: Option<String>,
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

impl DocumentReceipt {
    /// True when the text is extracted and a check can consume the upload.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.status == DocumentStatus::READY
    }

    /// True when extraction failed. [`error_code`](Self::error_code) says why.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.status == DocumentStatus::FAILED
    }
}

/// The statuses of an upload.
///
/// `ocr_pending` is not terminal: keep polling.
#[derive(Debug, Clone, Copy)]
pub struct DocumentStatus;

impl DocumentStatus {
    /// The file arrived.
    pub const UPLOADED: &'static str = "uploaded";
    /// The text is being extracted.
    pub const EXTRACTING: &'static str = "extracting";
    /// Text recognition on scanned pages is pending.
    pub const OCR_PENDING: &'static str = "ocr_pending";
    /// The text is extracted.
    pub const READY: &'static str = "ready";
    /// Extraction failed.
    pub const FAILED: &'static str = "failed";
}

impl_extras!(DocumentReceipt);

/// Parameters of [`CitationCheckDocuments::upload`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadParams {
    /// The file content. PDF, DOCX, Markdown or plain text, up to 25 MB.
    pub bytes: Vec<u8>,
    /// The file name, for example `schriftsatz.pdf`.
    pub filename: String,
    /// The media type, for example `application/pdf`.
    pub content_type: String,
}

impl UploadParams {
    /// Creates upload parameters.
    #[must_use]
    pub fn new(
        bytes: impl Into<Vec<u8>>,
        filename: impl Into<String>,
        content_type: impl Into<String>,
    ) -> Self {
        Self {
            bytes: bytes.into(),
            filename: filename.into(),
            content_type: content_type.into(),
        }
    }
}

/// Parameters of [`CitationCheckDocuments::wait_until_ready`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitParams {
    /// Time between polls. The default is 2 seconds.
    pub poll_interval: Duration,
    /// Total time to wait. The default is 120 seconds.
    pub timeout: Duration,
}

impl Default for WaitParams {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(2),
            timeout: Duration::from_secs(120),
        }
    }
}

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
    /// use klaracase::{Klaracase, UploadParams, WaitParams};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let bytes = std::fs::read("schriftsatz.pdf").expect("read the file");
    /// let receipt = client
    ///     .citation_check_documents()
    ///     .upload(UploadParams::new(bytes, "schriftsatz.pdf", "application/pdf"))
    ///     .await?;
    /// let ready = client
    ///     .citation_check_documents()
    ///     .wait_until_ready(&receipt.id, WaitParams::default())
    ///     .await?;
    /// println!("{:?} credits quoted", ready.quoted_credits);
    /// # Ok(()) }
    /// ```
    pub async fn upload(&self, params: UploadParams) -> Result<Response<DocumentReceipt>, Error> {
        Part::bytes(Vec::new())
            .mime_str(&params.content_type)
            .map_err(|_| {
                Error::config(
                    "client_validation",
                    format!("`{}` is not a valid media type", params.content_type),
                )
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

    /// Polls [`get`](Self::get) until the upload is `ready`.
    ///
    /// `uploaded`, `extracting` and `ocr_pending` keep the loop going. It returns the receipt when the
    /// status is `ready`. When extraction ends in `failed` it returns [`Error::DocumentFailed`]. When
    /// the timeout passes first it returns [`Error::DocumentTimeout`] with the last receipt. An error
    /// answer while polling returns the matching error at once.
    pub async fn wait_until_ready(
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
