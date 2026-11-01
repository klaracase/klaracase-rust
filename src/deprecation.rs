//! The deprecation hook.
//!
//! When the API deprecates an endpoint it announces it with `Deprecation`, `Sunset` and
//! `Link: <...>; rel="deprecation"` headers. The client passes the raw header values to a hook, at
//! most once per process.

use std::sync::atomic::{AtomicBool, Ordering};

use reqwest::header::HeaderMap;

/// The deprecation headers of one answer, as the server sent them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeprecationNotice {
    /// The raw `Deprecation` header, for example `@1767225600`.
    pub deprecation: Option<String>,
    /// The raw `Sunset` header, an HTTP date.
    pub sunset: Option<String>,
    /// The raw `Link` header, which usually points to the deprecation notes.
    pub link: Option<String>,
}

impl DeprecationNotice {
    pub(crate) fn from_headers(headers: &HeaderMap) -> Option<Self> {
        let read = |name: &str| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let notice = Self {
            deprecation: read("deprecation"),
            sunset: read("sunset"),
            link: read("link"),
        };
        (notice.deprecation.is_some() || notice.sunset.is_some()).then_some(notice)
    }
}

static WARNED: AtomicBool = AtomicBool::new(false);

/// Runs `hook` unless a notice was already delivered in this process.
pub(crate) fn deliver(notice: DeprecationNotice, hook: &(dyn Fn(DeprecationNotice) + Send + Sync)) {
    if WARNED.swap(true, Ordering::SeqCst) {
        return;
    }
    hook(notice);
}

/// The default hook: one warning through `log` (feature `log`) or on stderr.
pub(crate) fn default_hook(notice: DeprecationNotice) {
    let message = format!(
        "klaracase: the API announced a deprecation (Deprecation: {}, Sunset: {}, Link: {}). \
         See https://klaracase.de/docs/api",
        notice.deprecation.as_deref().unwrap_or("-"),
        notice.sunset.as_deref().unwrap_or("-"),
        notice.link.as_deref().unwrap_or("-"),
    );
    #[cfg(feature = "log")]
    log::warn!("{message}");
    #[cfg(not(feature = "log"))]
    eprintln!("{message}");
}

/// Forgets that a deprecation notice was delivered, so the next one runs the hook again.
///
/// The guard is process-wide on purpose. This function exists for tests.
#[doc(hidden)]
pub fn reset_deprecation_guard() {
    WARNED.store(false, Ordering::SeqCst);
}
