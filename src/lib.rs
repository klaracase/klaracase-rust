//! Official Rust client for the Klaracase REST API.
//!
//! Klaracase covers German law only. It is a research tool. It gives no legal advice and does not
//! replace a lawyer's review. A `not_found` verdict means the citation was not found in the corpus;
//! it is not a claim that the citation is fabricated.

mod citation_checks;
mod client;
mod common;
mod corpus;
mod datapackages;
mod decisions;
mod deprecation;
mod documents;
mod encode;
mod error;
mod norms;
mod response;
mod retry;
mod search;
mod types;

pub use citation_checks::{
    CheckProgress, CitationCheckRun, CitationChecks, CreateCitationCheckParams, Finding,
    MatchedDecision, ReasonCount, RunStatus, UniqueCitation, Verdict,
};
pub use client::{ClientBuilder, Klaracase};
pub use common::{Count, Credits, Snippet};
pub use corpus::{
    BranchCoverage, Corpus, CorpusCoverage, CourtCoverage, CoverageParams, Facets, FacetsParams,
};
pub use datapackages::{CreateDatapackageParams, Datapackage, Datapackages, PackageDecision};
pub use decisions::{
    AvailableRange, CitationEdge, CitationGraph, CitationPagination, CitationSource,
    CitationsParams, CorpusRef, Decision, DecisionMetadata, DecisionSearchResults, Decisions,
    GetDecisionParams, SearchDecisionsParams, SearchHit,
};
pub use deprecation::{DeprecationNotice, reset_deprecation_guard};
pub use documents::{
    CitationCheckDocuments, DocumentReceipt, DocumentStatus, UploadParams, WaitParams,
};
pub use error::{Error, Problem};
pub use norms::{
    GetNormParams, Norm, NormBody, NormLaw, NormLinks, NormNeighbours, NormVersion, Norms,
};
pub use response::{Extras, Response, ResponseMeta};
pub use retry::{Clock, SleepFuture, Sleeper, SystemClock};
pub use search::{
    CrossTypeSearchParams, CrossTypeSearchResults, SearchSections, SectionTotals,
    SectionTotalsBySection,
};
pub use types::*;

#[cfg(feature = "tokio")]
pub use retry::TokioSleeper;
