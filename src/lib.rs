#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

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
mod open;
mod params;
mod response;
mod retry;
mod search;
mod types;

pub use citation_checks::{
    CheckProgress, CheckScope, CitationCheckRun, CitationChecks, CreateCitationCheckParams,
    Evidence, Extraction, Finding, MatchedDecision, ReasonCount, UniqueCitation,
};
pub use client::{ClientBuilder, Klaracase};
pub use common::{Count, Credits, QueryExpansion, Snippet};
pub use corpus::{
    BranchCoverage, Corpus, CorpusCoverage, CourtCoverage, CoverageParams, CoverageRange,
    CoverageRanges, FacetValues, Facets, FacetsParams, InstanceMix, LawCoverage,
};
pub use datapackages::{
    CreateDatapackageParams, Datapackage, Datapackages, EdgeAccounting, PackageDecision,
};
pub use decisions::{
    AvailableRange, CitationEdge, CitationGraph, CitationPagination, CitationSource,
    CitationsParams, CorpusRef, Decision, DecisionHit, DecisionMetadata, DecisionSearchResults,
    Decisions, GetDecisionParams, ReturnedRange, SearchDecisionsParams,
};
pub use deprecation::{DeprecationNotice, reset_deprecation_guard};
pub use documents::{CitationCheckDocuments, DocumentReceipt, UploadParams, WaitParams};
pub use error::{Error, Problem};
pub use norms::{
    GetNormParams, Norm, NormBody, NormLaw, NormLinks, NormNeighbours, NormVersion, Norms,
    QualifierCheck, ToolHint,
};
pub use open::{Open, WireValue};
pub use response::{Extras, Response, ResponseMeta};
pub use retry::{Clock, SleepFuture, Sleeper, SystemClock};
pub use search::{
    CrossTypeSearchParams, CrossTypeSearchResults, LawHit, SearchSections, SectionTotals,
    SectionTotalsBySection,
};
pub use types::*;

#[cfg(feature = "tokio")]
pub use retry::TokioSleeper;
