//! Official Rust client for the Klaracase REST API.
//!
//! Klaracase covers German law only. It is a research tool. It gives no legal advice and does not
//! replace a lawyer's review. A `not_found` verdict means the citation was not found in the corpus;
//! it is not a claim that the citation is fabricated.

mod client;
mod common;
mod deprecation;
mod documents;
mod encode;
mod error;
mod response;
mod retry;
mod types;

pub use client::{ClientBuilder, Klaracase};
pub use common::{Count, Credits, Snippet};
pub use deprecation::{DeprecationNotice, reset_deprecation_guard};
pub use documents::{DocumentReceipt, DocumentStatus};
pub use error::{Error, Problem};
pub use response::{Extras, Response, ResponseMeta};
pub use retry::{Clock, SleepFuture, Sleeper, SystemClock};
pub use types::*;

#[cfg(feature = "tokio")]
pub use retry::TokioSleeper;
