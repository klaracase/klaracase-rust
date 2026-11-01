//! The wrapper around every successful answer.

use std::ops::Deref;
use std::time::Duration;

use serde_json::{Map, Value};

/// A parsed answer plus facts about the HTTP exchange.
///
/// `Response<T>` dereferences to `T`, so `response.results` works on a search answer. Use
/// [`Response::into_body`] to move the body out and [`Response::meta`] for the HTTP facts.
///
/// Nothing in the body is renamed, reordered, cleaned or translated. The typed view keeps every
/// unknown key in the `extra` map of the struct that held it. [`Response::raw`] gives the answer as
/// parsed JSON, exactly as the server sent it.
#[derive(Debug, Clone)]
pub struct Response<T> {
    /// The typed body.
    pub body: T,
    /// Facts about the HTTP exchange.
    pub meta: ResponseMeta,
    raw: Value,
}

impl<T> Response<T> {
    pub(crate) fn new(body: T, meta: ResponseMeta, raw: Value) -> Self {
        Self { body, meta, raw }
    }

    /// The facts about the HTTP exchange.
    #[must_use]
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The answer as parsed JSON, without `meta`. This is the lossless view: it holds every key the
    /// server sent, at every depth, with `null` kept apart from an absent key.
    #[must_use]
    pub fn raw(&self) -> &Value {
        &self.raw
    }

    /// Moves the typed body out and drops the rest.
    #[must_use]
    pub fn into_body(self) -> T {
        self.body
    }

    /// Splits the response into its typed body, its metadata and the raw JSON.
    #[must_use]
    pub fn into_parts(self) -> (T, ResponseMeta, Value) {
        (self.body, self.meta, self.raw)
    }
}

impl<T> Deref for Response<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.body
    }
}

/// Facts about one call.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseMeta {
    /// The HTTP status of the answer: 200, or 202 for an upload.
    pub status: u16,
    /// The value of the `X-Klaracase-Api-Release` header, if present.
    pub api_release: Option<String>,
    /// Wall time of the final HTTP attempt, from sending the request to reading the whole body.
    /// Earlier attempts and the waits between attempts are not counted.
    pub request_duration: Duration,
}

/// Access to the unknown keys of a response struct.
///
/// Every response struct in this crate keeps keys it does not model in a flattened `extra` map. This
/// trait reads that map without naming the struct.
pub trait Extras {
    /// Keys the server sent that this crate does not model.
    fn extra(&self) -> &Map<String, Value>;
}
