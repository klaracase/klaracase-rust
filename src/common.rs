//! Types shared by several responses.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::response::impl_extras;

/// What a call cost. Every operation body carries it.
///
/// For a superadmin account `metered` is `false` and both numbers are `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Credits {
    /// Whether the call was metered.
    #[serde(default)]
    pub metered: bool,
    /// Credits this call cost. `0` for a free call or a repeat in the same period.
    #[serde(default)]
    pub cost: Option<i64>,
    /// Credits left after the call.
    #[serde(default)]
    pub credits_remaining: Option<i64>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A count that may be a lower bound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Count {
    /// The number.
    #[serde(default)]
    pub value: u64,
    /// True when the real number may be higher.
    #[serde(default)]
    pub is_floor: bool,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One highlighted passage of a hit. `text` carries `<mark>` tags around the matched terms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    /// The passage.
    #[serde(default)]
    pub text: String,
    /// The Randnummer the passage starts at, if known.
    #[serde(default)]
    pub rn: Option<u64>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl_extras!(Credits, Count, Snippet);
