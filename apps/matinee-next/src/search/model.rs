//! Search state. No GPUI, no HTTP.
//!
//! Two texts are kept apart. `input` is what the person is typing right
//! now. `effective` is the newest query that has been asked of Jellyfin, and
//! `shown` is the query whose titles are held. While typing waits out the
//! debounce, only `input` moves, so the results on screen still belong to
//! the query they are labelled with. When the effective query changes, the
//! old titles stay on screen marked as stale until the new first page
//! arrives, and then they are replaced.
//!
//! Every request carries a [`Ticket`]. Only the ticket in flight applies, so
//! an answer for an earlier query or an earlier page is ignored. Typing sets
//! a [`Debounce`] token. A timer whose token is no longer current does
//! nothing. The timer lives in the screen; this model only counts.

use std::collections::HashSet;

use matinee_core::{ItemId, LibraryPage, LibraryPageRequest, MediaItem, SearchQuery};
use matinee_jellyfin::JellyfinError;

/// Titles per request. Search answers come back on each query and each
/// scroll to the end. Sixty titles is six rows at the widest grid, which is
/// more than a window of cards, and keeps the first answer quick.
pub(crate) const PAGE_SIZE: usize = 60;

/// Ask for the next page once the last built card is this close to the end
/// of what is loaded. About four rows at the widest.
pub(crate) const PREFETCH_ITEMS: usize = 40;

/// How long typing must rest before a search is sent. Long enough that a
/// word typed at normal speed sends one search, short enough to feel live.
pub(crate) const DEBOUNCE_MILLIS: u64 = 300;

/// Identifies one request. Only the ticket in flight applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Ticket(u64);

/// Identifies one debounce wait. Only the latest one may send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Debounce(u64);

/// Why a search could not be completed. Copy is fixed; no server text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchFailure {
    SignedOut,
    Unreachable,
    Unreadable,
}

impl SearchFailure {
    pub(crate) fn from_error(error: &JellyfinError) -> Self {
        match error {
            error if crate::session::session_ended(error) => Self::SignedOut,
            JellyfinError::Unreachable { .. } | JellyfinError::Cancelled => Self::Unreachable,
            _ => Self::Unreadable,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::SignedOut => "Your Jellyfin session has ended",
            Self::Unreachable => "Jellyfin isn't answering",
            Self::Unreadable => "Search couldn't be completed",
        }
    }

    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::SignedOut => "Sign in again to keep searching.",
            Self::Unreachable => {
                "Check that the server is running and reachable from this computer, then try again."
            }
            Self::Unreadable => "Jellyfin sent a response Matinee could not read. Try again.",
        }
    }
}

/// Work the screen runs on the service runtime.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Request {
    Page {
        ticket: Ticket,
        query: SearchQuery,
        page: LibraryPageRequest,
    },
    /// One title again, after playback may have changed its progress.
    Item { ticket: Ticket, id: ItemId },
}

/// An answer, already mapped out of the Jellyfin client.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Response {
    Page(Result<LibraryPage, SearchFailure>),
    Item(Result<Box<MediaItem>, SearchFailure>),
}

/// What applying an answer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Applied {
    /// A late or unknown ticket. Nothing changed.
    Ignored,
    /// The first page of a query replaced the titles: the grid starts over.
    Replaced,
    /// A later page was added at the end.
    Appended,
    /// Something else changed: a failure, or one title's progress.
    Updated,
    /// Jellyfin no longer accepts the session. The application decides what
    /// that means; Search only reports it.
    SessionExpired,
}

/// What the results area shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchState {
    /// Nothing has been searched. `Empty` is a blank field; `TooShort` is
    /// one letter, which is not sent.
    Idle(IdleReason),
    /// The first page of the effective query has not arrived.
    Loading,
    /// Titles for the effective query.
    Ready,
    /// The effective query matched nothing.
    NoResults,
    /// The first page failed. Offers Try again.
    Failed(SearchFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IdleReason {
    Empty,
    TooShort,
}

/// What sits under the last row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Footer {
    /// More remain and none is being asked for yet.
    Idle,
    Loading,
    /// The next page failed; what is loaded stays. Offers Try again.
    Failed(SearchFailure),
    /// The server has nothing more.
    End,
}

#[derive(Debug, Default)]
pub(crate) struct SearchModel {
    /// What the person has typed, as typed (not trimmed).
    input: String,
    /// The latest debounce wait. Anything else is stale.
    debounce: u64,
    /// The newest query asked of Jellyfin. `None` when the input is too
    /// short or blank.
    effective: Option<SearchQuery>,
    /// The query whose titles are held. Equal to `effective` once its first
    /// page has arrived.
    shown: Option<SearchQuery>,
    items: Vec<MediaItem>,
    ids: HashSet<ItemId>,
    total: Option<usize>,
    next_start: usize,
    has_more: bool,
    /// The request in flight, and where a page starts.
    pending: Option<(Ticket, usize)>,
    /// The first page of `effective` failed.
    failure: Option<SearchFailure>,
    /// A later page failed; what is loaded stays.
    more_failed: Option<SearchFailure>,
    focused: Option<ItemId>,
    opened: Option<ItemId>,
    reconcile: Option<Ticket>,
    issued: u64,
    pages_requested: usize,
}

impl SearchModel {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn input(&self) -> &str {
        &self.input
    }

    pub(crate) fn effective(&self) -> Option<&SearchQuery> {
        self.effective.as_ref()
    }

    pub(crate) fn shown(&self) -> Option<&SearchQuery> {
        self.shown.as_ref()
    }

    pub(crate) fn items(&self) -> &[MediaItem] {
        &self.items
    }

    pub(crate) fn total(&self) -> Option<usize> {
        self.total
    }

    pub(crate) fn state(&self) -> SearchState {
        let Some(effective) = &self.effective else {
            // Long enough, but the debounce has not ended: the search is on
            // its way, so it reads as loading rather than as a blank field.
            if SearchQuery::parse(&self.input).is_some() {
                return SearchState::Loading;
            }
            let reason = if self.input.trim().is_empty() {
                IdleReason::Empty
            } else {
                IdleReason::TooShort
            };
            return SearchState::Idle(reason);
        };
        if let Some(failure) = self.failure {
            return SearchState::Failed(failure);
        }
        if self.shown.as_ref() != Some(effective) {
            return SearchState::Loading;
        }
        if self.items.is_empty() {
            SearchState::NoResults
        } else {
            SearchState::Ready
        }
    }

    /// The old titles are on screen while the new query loads. They are
    /// dimmed, so they are never taken for the new query's results.
    pub(crate) fn is_stale(&self) -> bool {
        self.effective.is_some()
            && self.shown != self.effective
            && self.failure.is_none()
            && !self.items.is_empty()
    }

    /// Whether a request is in flight, for the Refresh control.
    pub(crate) fn is_loading(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn footer(&self) -> Footer {
        if let Some(failure) = self.more_failed {
            return Footer::Failed(failure);
        }
        match self.pending {
            Some((_, start)) if start > 0 => Footer::Loading,
            _ if self.shown.is_some() && self.shown == self.effective && !self.has_more => {
                Footer::End
            }
            _ => Footer::Idle,
        }
    }

    /// Requests issued for pages, for tests and the scale checks.
    #[cfg(test)]
    pub(crate) fn pages_requested(&self) -> usize {
        self.pages_requested
    }

    /// The person typed. Returns the debounce wait to start, if a search
    /// should follow. Nothing is sent yet, and typing again replaces it.
    pub(crate) fn type_text(&mut self, text: &str) -> Option<Debounce> {
        self.input = text.to_string();
        self.debounce += 1;
        match SearchQuery::parse(text) {
            // Too short or blank: nothing is sent, and the titles of the old
            // query are no longer what the field asks for.
            None => {
                self.clear_query();
                None
            }
            // The query already asked for (held or in flight): nothing new to
            // send, and its request must not be cancelled.
            Some(query) if self.effective.as_ref() == Some(&query) => None,
            Some(_) => Some(Debounce(self.debounce)),
        }
    }

    /// A debounce wait ended. Sends the input if that wait is still the
    /// latest one.
    pub(crate) fn debounced(&mut self, debounce: Debounce) -> Vec<Request> {
        if debounce.0 != self.debounce {
            return Vec::new();
        }
        self.request_input()
    }

    /// Enter: send the input now, without waiting. Cancels any debounce
    /// wait. A query already held sends nothing.
    pub(crate) fn submit(&mut self) -> Vec<Request> {
        self.debounce += 1;
        self.request_input()
    }

    /// Refresh: the first page of the query on screen again. Nothing is sent
    /// without a query.
    pub(crate) fn refresh(&mut self) -> Vec<Request> {
        if self.effective.is_none() {
            return Vec::new();
        }
        vec![self.first_page()]
    }

    /// Try again after a failure: the first page, or the page that failed.
    pub(crate) fn retry(&mut self) -> Vec<Request> {
        if self.failure.is_some() {
            self.failure = None;
            return vec![self.first_page()];
        }
        if self.more_failed.is_some() {
            self.more_failed = None;
            return self.next_page().into_iter().collect();
        }
        Vec::new()
    }

    /// The last built card is `last_built`: ask for the next page when it is
    /// near the end of what is loaded. Nothing while a page is in flight,
    /// after the end, while the titles are stale, or after a failure.
    pub(crate) fn want_more(&mut self, last_built: usize) -> Option<Request> {
        if self.state() != SearchState::Ready
            || self.shown != self.effective
            || !self.has_more
            || self.pending.is_some()
            || self.more_failed.is_some()
            || last_built + PREFETCH_ITEMS < self.items.len()
        {
            return None;
        }
        self.next_page()
    }

    pub(crate) fn note_focused(&mut self, id: ItemId) {
        self.focused = Some(id);
    }

    pub(crate) fn focused_index(&self) -> Option<usize> {
        let id = self.focused.as_ref()?;
        self.items.iter().position(|item| item.id() == id)
    }

    pub(crate) fn note_opened(&mut self, id: ItemId) {
        self.focused = Some(id.clone());
        self.opened = Some(id);
    }

    /// After playback: ask for the opened title again, so its progress is
    /// current. One request, not a search.
    pub(crate) fn reconcile(&mut self) -> Option<Request> {
        let id = self.opened.clone()?;
        if !self.ids.contains(&id) {
            return None;
        }
        let ticket = self.ticket();
        self.reconcile = Some(ticket);
        Some(Request::Item { ticket, id })
    }

    pub(crate) fn apply(&mut self, request: &Request, response: Response) -> Applied {
        let current = match request {
            Request::Page { ticket, .. } => self.pending.map(|(t, _)| t) == Some(*ticket),
            Request::Item { ticket, .. } => self.reconcile == Some(*ticket),
        };
        if !current {
            return Applied::Ignored;
        }
        let failure = match &response {
            Response::Page(Err(failure)) | Response::Item(Err(failure)) => Some(*failure),
            _ => None,
        };
        if failure == Some(SearchFailure::SignedOut) {
            // Not a search failure: the session is gone. What is shown stays,
            // and the shell ends the session.
            self.pending = None;
            self.reconcile = None;
            return Applied::SessionExpired;
        }
        match (request, response) {
            (Request::Page { query, page, .. }, Response::Page(result)) => {
                self.apply_page(query, *page, result)
            }
            (Request::Item { .. }, Response::Item(result)) => {
                self.reconcile = None;
                if let Ok(item) = result
                    && let Some(slot) = self.items.iter_mut().find(|old| old.id() == item.id())
                {
                    *slot = *item;
                }
                Applied::Updated
            }
            _ => Applied::Ignored,
        }
    }

    fn apply_page(
        &mut self,
        query: &SearchQuery,
        page: LibraryPageRequest,
        result: Result<LibraryPage, SearchFailure>,
    ) -> Applied {
        self.pending = None;
        let first = page.start == 0;
        match result {
            Err(failure) if first => {
                // The new query failed. Its titles are not shown, and the
                // old ones must not stay under a failure for the new query.
                self.failure = Some(failure);
                self.shown = None;
                self.items.clear();
                self.ids.clear();
                self.total = None;
                self.next_start = 0;
                self.has_more = false;
                self.focused = None;
                Applied::Updated
            }
            Err(failure) => {
                self.more_failed = Some(failure);
                Applied::Updated
            }
            Ok(loaded) => {
                let has_more = loaded.has_more(page.limit);
                let received = loaded.items.len();
                if first {
                    self.items.clear();
                    self.ids.clear();
                    self.more_failed = None;
                    self.shown = Some(query.clone());
                }
                for item in loaded.items {
                    // Titles can shift between pages while the library
                    // changes. A title already shown is not shown twice.
                    if self.ids.insert(item.id().clone()) {
                        self.items.push(item);
                    }
                }
                self.total = loaded.total.or(self.total);
                self.next_start = page.start + received;
                self.has_more = has_more;
                if first {
                    if let Some(focused) = &self.focused
                        && !self.ids.contains(focused)
                    {
                        self.focused = None;
                    }
                    Applied::Replaced
                } else {
                    Applied::Appended
                }
            }
        }
    }

    /// Send the input as the effective query, unless it is already held or
    /// already on its way.
    fn request_input(&mut self) -> Vec<Request> {
        let Some(query) = SearchQuery::parse(&self.input) else {
            self.clear_query();
            return Vec::new();
        };
        if self.effective.as_ref() == Some(&query) {
            // The same query. Only a first page that failed is asked again.
            return if self.failure.is_some() && self.pending.is_none() {
                vec![self.first_page()]
            } else {
                Vec::new()
            };
        }
        if self.shown.as_ref() == Some(&query) {
            self.resume_effective(query);
            return Vec::new();
        }
        self.effective = Some(query);
        self.failure = None;
        self.more_failed = None;
        vec![self.first_page()]
    }

    /// The effective query is back on the titles already held: they show
    /// again at once, and any request for another query is abandoned.
    fn resume_effective(&mut self, query: SearchQuery) {
        self.effective = Some(query);
        self.pending = None;
        self.failure = None;
        self.more_failed = None;
    }

    /// No query: nothing held, nothing pending, no failure.
    fn clear_query(&mut self) {
        self.effective = None;
        self.shown = None;
        self.items.clear();
        self.ids.clear();
        self.total = None;
        self.next_start = 0;
        self.has_more = false;
        self.pending = None;
        self.failure = None;
        self.more_failed = None;
        self.focused = None;
        self.reconcile = None;
    }

    fn first_page(&mut self) -> Request {
        self.page_request(0)
    }

    fn next_page(&mut self) -> Option<Request> {
        let start = self.next_start;
        Some(self.page_request(start))
    }

    fn page_request(&mut self, start: usize) -> Request {
        let ticket = self.ticket();
        self.pending = Some((ticket, start));
        self.pages_requested += 1;
        Request::Page {
            ticket,
            query: self
                .effective
                .clone()
                .expect("a page is only asked for an effective query"),
            page: LibraryPageRequest {
                start,
                limit: PAGE_SIZE,
            },
        }
    }

    fn ticket(&mut self) -> Ticket {
        self.issued += 1;
        Ticket(self.issued)
    }
}
