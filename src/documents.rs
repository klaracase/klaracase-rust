//! Uploads for the citation check: `/citation-check-documents`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::common::Credits;
use crate::response::impl_extras;

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
