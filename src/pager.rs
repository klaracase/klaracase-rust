//! Paging through a search or a citation graph.
//!
//! This crate has no stream dependency, so paging is a plain loop around [`Pager::next_page`]:
//!
//! ```no_run
//! # async fn run(client: klaracase::Klaracase) -> Result<(), klaracase::Error> {
//! let mut pager = client.decisions().search("Untreue").court("BGH").pages(3);
//! while let Some(page) = pager.next_page().await? {
//!     for hit in &page.results {
//!         println!("{:?}", hit.aktenzeichen);
//!     }
//! }
//! # Ok(()) }
//! ```

use std::fmt;
use std::marker::PhantomData;

use serde::de::DeserializeOwned;

use crate::client::{Klaracase, Request};
use crate::error::Error;
use crate::response::Response;

/// How a page ended the walk, as judged by the page itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageEnd {
    /// The page holds nothing. It is not handed to the caller, and the walk stops.
    Empty,
    /// The page holds data and the server says nothing follows it.
    Last,
    /// The page holds data and more may follow.
    More,
}

/// Builds the request for one page number.
pub(crate) type PageRequest = Box<dyn Fn(u32) -> Result<Request, Error> + Send + Sync>;

/// Walks the pages of a search or a citation graph, one request per page.
///
/// Get one from `decisions().search(..).pages(max_pages)` or
/// `decisions().citations(..).pages(max_pages)`.
///
/// `max_pages` is required, and it is clamped to what the server serves: 10 pages for a search and
/// 20 for a citation graph. A walk also stops at the first empty page, which is not returned.
///
/// **Cost.** Paging the same query within a credit period costs no extra credits. A changed query,
/// filter or mode is a new charge. Check `credits` on each response. The same holds for the
/// citation graph. `max_pages` is a guard against a runaway loop. Every page is still one request
/// against the rate limit of 60 requests per minute per token, and one that comes back empty is a
/// request too.
pub struct Pager<T> {
    client: Klaracase,
    request: PageRequest,
    classify: fn(&T) -> PageEnd,
    next: u32,
    last: u32,
    finished: bool,
    fetched: u32,
    _page: PhantomData<fn() -> T>,
}

impl<T> fmt::Debug for Pager<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pager")
            .field("next_page", &self.next)
            .field("last_page", &self.last)
            .field("finished", &self.finished)
            .field("fetched", &self.fetched)
            .finish_non_exhaustive()
    }
}

impl<T: DeserializeOwned> Pager<T> {
    /// A pager that fetches pages `first..=first + max_pages - 1`, never past `cap`.
    pub(crate) fn new(
        client: Klaracase,
        request: PageRequest,
        classify: fn(&T) -> PageEnd,
        first: u32,
        max_pages: u32,
        cap: u32,
    ) -> Self {
        let first = first.max(1);
        let last = first.saturating_add(max_pages).saturating_sub(1).min(cap);
        Self {
            client,
            request,
            classify,
            next: first,
            last,
            finished: max_pages == 0 || first > cap,
            fetched: 0,
            _page: PhantomData,
        }
    }

    /// Fetches the next page, or returns `None` when the walk is over.
    ///
    /// The walk is over after `max_pages` pages, at the cap the server serves, at the first empty
    /// page (which is not returned) and after a page that says nothing follows. Each call that
    /// returns a page, or an empty one, sent exactly one request.
    ///
    /// After an error the pager keeps its place: calling again retries the same page.
    ///
    /// # Errors
    ///
    /// Any [`Error`] the page request returns.
    pub async fn next_page(&mut self) -> Result<Option<Response<T>>, Error> {
        if self.finished || self.next > self.last {
            return Ok(None);
        }
        let request = (self.request)(self.next)?;
        let page = self.client.execute::<T>(request).await?;
        match (self.classify)(&page.body) {
            PageEnd::Empty => {
                self.finished = true;
                Ok(None)
            }
            end => {
                self.fetched += 1;
                self.next += 1;
                self.finished = end == PageEnd::Last || self.next > self.last;
                Ok(Some(page))
            }
        }
    }

    /// How many pages this pager has handed out.
    #[must_use]
    pub fn pages_fetched(&self) -> u32 {
        self.fetched
    }
}
