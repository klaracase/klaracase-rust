//! Helpers shared by the parameter structs: setters and structural checks.
//!
//! The checks look at the shape of the input only. They never enforce a number the server owns
//! (a query length, a page cap, a limit): those caps can change with an API release, and the server
//! answers them with a 422 that carries the field errors.

use crate::error::Error;

/// Generates consuming setters for `Option` fields.
///
/// `direct` writes the field of `self`. `delegate` forwards to the same setter of `self.params`, for
/// a request builder that wraps a parameter struct.
///
/// Kinds: `text` takes `impl Into<String>`, `open(E)` takes `impl Into<Open<E>>`, `plain(T)` takes
/// a `T`.
macro_rules! option_setters {
    ($mode:ident { $( $(#[$meta:meta])* $kind:ident $( ( $ty:ty ) )? $name:ident ),* $(,)? }) => {
        $( $crate::params::option_setters!(@one $mode $kind $( ( $ty ) )? $name [ $(#[$meta])* ]); )*
    };
    (@one direct text $name:ident [ $(#[$meta:meta])* ]) => {
        $(#[$meta])*
        #[must_use]
        pub fn $name(mut self, value: impl Into<String>) -> Self {
            self.$name = Some(value.into());
            self
        }
    };
    (@one delegate text $name:ident [ $(#[$meta:meta])* ]) => {
        $(#[$meta])*
        pub fn $name(mut self, value: impl Into<String>) -> Self {
            self.params = self.params.$name(value);
            self
        }
    };
    (@one direct open($ty:ty) $name:ident [ $(#[$meta:meta])* ]) => {
        $(#[$meta])*
        #[must_use]
        pub fn $name(mut self, value: impl Into<$crate::Open<$ty>>) -> Self {
            self.$name = Some(value.into());
            self
        }
    };
    (@one delegate open($ty:ty) $name:ident [ $(#[$meta:meta])* ]) => {
        $(#[$meta])*
        pub fn $name(mut self, value: impl Into<$crate::Open<$ty>>) -> Self {
            self.params = self.params.$name(value);
            self
        }
    };
    (@one direct plain($ty:ty) $name:ident [ $(#[$meta:meta])* ]) => {
        $(#[$meta])*
        #[must_use]
        pub fn $name(mut self, value: $ty) -> Self {
            self.$name = Some(value);
            self
        }
    };
    (@one delegate plain($ty:ty) $name:ident [ $(#[$meta:meta])* ]) => {
        $(#[$meta])*
        pub fn $name(mut self, value: $ty) -> Self {
            self.params = self.params.$name(value);
            self
        }
    };
}
pub(crate) use option_setters;

/// The error for input the client refuses before any request.
pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::client_validation(message)
}

/// Refuses a blank value (empty or only white space).
pub(crate) fn require_text(field: &str, value: &str) -> Result<(), Error> {
    if value.trim().is_empty() {
        return Err(invalid(format!("`{field}` must not be blank")));
    }
    Ok(())
}

/// Reads `YYYY-MM-DD` into year, month and day. The date must exist in the calendar.
fn parse_date(value: &str) -> Option<(u32, u32, u32)> {
    let bytes = value.as_bytes();
    let shape_ok = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    if !shape_ok {
        return None;
    }
    let year: u32 = value[0..4].parse().ok()?;
    let month: u32 = value[5..7].parse().ok()?;
    let day: u32 = value[8..10].parse().ok()?;
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    (1..=days_in_month)
        .contains(&day)
        .then_some((year, month, day))
}

/// Refuses a date that is not `YYYY-MM-DD`.
pub(crate) fn check_date(field: &str, value: Option<&str>) -> Result<(), Error> {
    match value {
        Some(text) if parse_date(text).is_none() => Err(invalid(format!(
            "`{field}` must be a date formatted YYYY-MM-DD, got `{text}`"
        ))),
        _ => Ok(()),
    }
}

/// Refuses an inverted date range. Both dates must already be well formed.
pub(crate) fn check_date_range(from: Option<&str>, to: Option<&str>) -> Result<(), Error> {
    if let (Some(start), Some(end)) = (from.and_then(parse_date), to.and_then(parse_date)) {
        if start > end {
            return Err(invalid("`date_from` must not be later than `date_to`"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_must_exist_in_the_calendar() {
        for good in ["2026-11-01", "2024-02-29", "1950-10-02"] {
            assert!(parse_date(good).is_some(), "{good}");
        }
        for bad in [
            "2026-13-01",
            "2026-02-29",
            "2026-00-10",
            "2026-11-31",
            "26-11-01",
            "2026/11/01",
            "2026-11-1",
            "2026-11-01T00:00:00Z",
            "",
            "ab26-11-01",
        ] {
            assert!(parse_date(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn an_inverted_range_is_refused_and_an_equal_one_is_not() {
        assert!(check_date_range(Some("2020-01-02"), Some("2020-01-01")).is_err());
        assert!(check_date_range(Some("2020-01-01"), Some("2020-01-01")).is_ok());
        assert!(check_date_range(Some("2020-01-01"), None).is_ok());
    }
}
