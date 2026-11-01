//! Types shared by several responses.

use crate::open::response_struct;

response_struct! {
    /// What a call cost. Every operation body carries it.
    ///
    /// For a superadmin account `metered` is `false` and both numbers are `None`.
    ///
    /// Plan credits are spent first. While the prepaid API budget is available, a call the plan
    /// credits do not cover is paid from the budget, in whole, never in part.
    Credits {
        /// Whether the call was metered.
        opt metered: bool,
        /// Credits this call cost, plan credits and API budget together. `0` for a free call or a
        /// repeat in the same period.
        opt cost: u64,
        /// The plan credits left in the credit period after the call. It never counts the API
        /// budget.
        opt credits_remaining: i64,
        /// The prepaid API budget after the call. `None` when the organization has no budget and may
        /// not load one: the key is absent then, never `null`.
        opt api_budget: ApiBudget,
    }
}

response_struct! {
    /// The prepaid API budget of the organization after a call.
    ///
    /// It pays for API and MCP calls once the plan credits are used up, and it does not expire. The
    /// web app never draws on it.
    ApiBudget {
        /// The budget after the call, in euro cents. It can be negative after a payment was lost.
        opt cents: i64,
        /// What one credit costs from the budget, in whole euro cents. This is the rate of the
        /// organization, which a contract can set apart from the list rate.
        opt rate_cents_per_credit: u64,
        /// How many further credits the spendable part of the budget pays for at that rate. `0`
        /// when the budget is empty or on hold.
        opt credits_at_rate: u64,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credits_without_a_budget_read_as_none_and_write_no_key() {
        let raw = json!({"metered": true, "cost": 1, "credits_remaining": 9});
        let credits: Credits = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(credits.api_budget, None);
        assert_eq!(serde_json::to_value(&credits).unwrap(), raw);
    }

    #[test]
    fn a_budget_is_typed_keeps_unknown_keys_and_round_trips() {
        let raw = json!({
            "metered": true,
            "cost": 1,
            "credits_remaining": 0,
            "api_budget": {
                "cents": -120,
                "rate_cents_per_credit": 9,
                "credits_at_rate": 0,
                "on_hold_since": "2026-11-01"
            }
        });
        let credits: Credits = serde_json::from_value(raw.clone()).unwrap();
        let budget = credits.api_budget.as_ref().expect("budget");
        assert_eq!(budget.cents, Some(-120));
        assert_eq!(budget.rate_cents_per_credit, Some(9));
        assert_eq!(budget.credits_at_rate, Some(0));
        assert_eq!(budget.extra["on_hold_since"], "2026-11-01");
        assert_eq!(serde_json::to_value(&credits).unwrap(), raw);
    }

    #[test]
    fn a_null_budget_reads_as_none() {
        let credits: Credits = serde_json::from_value(json!({"api_budget": null})).unwrap();
        assert_eq!(credits.api_budget, None);
    }
}
