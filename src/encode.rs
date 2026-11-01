//! Percent-encoding for path segments and query strings.
//!
//! The rule is fixed by the conformance pack. Every byte of the UTF-8 form outside the unreserved set
//! of RFC 3986 (`A-Z a-z 0-9 - . _ ~`) becomes `%XX` with upper-case hex. A space is `%20`, a slash is
//! `%2F`, `§` is `%C2%A7`. Standard-library helpers leave some of `! * ' ( ) : @ & = + $ , ;` alone.
//! This module does not.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::error::Error;

/// Everything except `A-Z a-z 0-9 - . _ ~`.
const UNRESERVED_ONLY: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Encodes one value (a path segment or a query component).
pub(crate) fn encode(value: &str) -> String {
    utf8_percent_encode(value, UNRESERVED_ONLY).to_string()
}

/// Encodes one path parameter as ONE segment.
///
/// An empty value, `.` and `..` are refused: the URL parser would drop or collapse such a segment and
/// the request would reach another endpoint.
pub(crate) fn path_segment(name: &str, value: &str) -> Result<String, Error> {
    if value.is_empty() || value == "." || value == ".." {
        return Err(Error::client_validation(format!(
            "the path parameter `{name}` must not be empty, `.` or `..`"
        )));
    }
    Ok(encode(value))
}

/// An ordered list of query parameters.
///
/// `None` is never pushed. `false` and `0` are values and are pushed. A list is sent as
/// `name[]=a&name[]=b`, the form PHP and Laravel read.
#[derive(Debug, Default, Clone)]
pub(crate) struct Query {
    pairs: Vec<(String, String)>,
}

impl Query {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, name: &str, value: impl Into<String>) {
        self.pairs.push((name.to_owned(), value.into()));
    }

    pub(crate) fn push_opt<V: ToString>(&mut self, name: &str, value: &Option<V>) {
        if let Some(value) = value {
            self.push(name, value.to_string());
        }
    }

    pub(crate) fn push_list<V: ToString>(&mut self, name: &str, values: &[V]) {
        let key = format!("{name}[]");
        for value in values {
            self.push(&key, value.to_string());
        }
    }

    /// The encoded query string without the leading `?`, or `None` when there is no parameter.
    pub(crate) fn to_query_string(&self) -> Option<String> {
        if self.pairs.is_empty() {
            return None;
        }
        let encoded: Vec<String> = self
            .pairs
            .iter()
            .map(|(name, value)| {
                let name = match name.strip_suffix("[]") {
                    Some(base) => format!("{}[]", encode(base)),
                    None => encode(name),
                };
                format!("{name}={}", encode(value))
            })
            .collect();
        Some(encoded.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_everything_but_unreserved() {
        assert_eq!(encode("VIII ZR 271/17"), "VIII%20ZR%20271%2F17");
        assert_eq!(encode("§ 535 BGB"), "%C2%A7%20535%20BGB");
        assert_eq!(encode("Art. 6 DSGVO"), "Art.%206%20DSGVO");
        assert_eq!(
            encode("ECLI:DE:BGH:2001:151101U1STR185.01.0"),
            "ECLI%3ADE%3ABGH%3A2001%3A151101U1STR185.01.0"
        );
        assert_eq!(
            encode("!*'();:@&=+$,/?#[]%"),
            "%21%2A%27%28%29%3B%3A%40%26%3D%2B%24%2C%2F%3F%23%5B%5D%25"
        );
        assert_eq!(encode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn refuses_segments_a_url_parser_would_collapse() {
        for bad in ["", ".", ".."] {
            let error = path_segment("id", bad).unwrap_err();
            assert_eq!(error.code(), Some("client_validation"));
        }
        assert_eq!(path_segment("id", "...").unwrap(), "...");
    }

    #[test]
    fn lists_are_sent_as_repeated_bracket_names() {
        let mut query = Query::new();
        query.push("query", "a&b");
        query.push_list("cited_statutes", &["StGB", "StPO"]);
        query.push_opt("bridge", &Some(false));
        query.push_opt::<u32>("limit", &None);
        assert_eq!(
            query.to_query_string().as_deref(),
            Some("query=a%26b&cited_statutes[]=StGB&cited_statutes[]=StPO&bridge=false")
        );
        assert_eq!(Query::new().to_query_string(), None);
    }
}
