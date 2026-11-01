//! Types shared by several responses.

use crate::open::response_struct;

response_struct! {
    /// What a call cost. Every operation body carries it.
    ///
    /// For a superadmin account `metered` is `false` and both numbers are `None`.
    Credits {
        /// Whether the call was metered.
        opt metered: bool,
        /// Credits this call cost. `0` for a free call or a repeat in the same period.
        opt cost: u64,
        /// Credits left after the call.
        opt credits_remaining: i64,
    }
}

response_struct! {
    /// A count that may be a lower bound. `is_floor` absent is not "exact": use [`Count::is_exact`].
    Count {
        /// The number.
        req value: u64,
        /// True when the real number may be higher.
        opt is_floor: bool,
    }
}

impl Count {
    /// True only when the server said the number is exact (`is_floor` is `false`).
    #[must_use]
    pub fn is_exact(&self) -> bool {
        self.is_floor == Some(false)
    }
}

response_struct! {
    /// One highlighted passage of a hit. `text` carries `<mark>` tags around the matched terms.
    Snippet {
        /// The passage.
        opt text: String,
        /// The Randnummer the passage starts at, if known.
        opt rn: u64,
    }
}

response_struct! {
    /// What the server added to a query, and why.
    QueryExpansion {
        /// For example `applied` or `skipped`.
        opt status: String,
        /// Whether register terms were added.
        opt applied: bool,
        /// Why the expansion was applied or skipped.
        opt reason: String,
        /// A note for a person.
        opt note: String,
    }
}

/// Returns the code points `start..end` of `text`, or `None` for an inverted or empty span or one
/// that reaches past the end of `text`.
///
/// Offsets in this API count Unicode code points of the exact string, not bytes. Slicing a Rust
/// string by those numbers would panic or cut a character.
pub(crate) fn slice_code_points(text: &str, start: u64, end: u64) -> Option<String> {
    if end <= start {
        return None;
    }
    let skip = usize::try_from(start).ok()?;
    let length = usize::try_from(end - start).ok()?;
    let span: String = text.chars().skip(skip).take(length).collect();
    (span.chars().count() == length).then_some(span)
}
