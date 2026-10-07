//! Library state. No GPUI, no HTTP.
//!
//! Library browses one kind of title at a time, Movies or Series. Each kind
//! keeps its own [`Catalog`]: the query (library, sort, filter), the pages
//! loaded so far, whether more remain, and which card was focused and
//! opened. Switching kinds keeps both, so Movies → Series → Movies lands
//! where it was.
//!
//! Every request carries a [`Ticket`]. A catalog applies only the ticket it
//! is waiting for, so a page from an earlier query, an earlier refresh, or a
//! kind that has since changed its query is ignored. A query change clears
//! the catalog at once; a refresh keeps the titles on screen until the new
//! first page replaces them.

use std::collections::HashSet;

use matinee_core::{
    ItemId, LibraryFilter, LibraryGenre, LibraryId, LibraryKind, LibraryPage, LibraryPageRequest,
    LibraryQuery, LibrarySort, LibraryView, MediaItem, WatchFilter,
};
use matinee_jellyfin::JellyfinError;

/// Titles per request. The first page fills the largest window (ten
/// columns, about four rows) more than twice over, and a 10,000-title
/// library is a hundred requests only if someone scrolls all of it.
pub(crate) const PAGE_SIZE: usize = 100;

/// Ask for the next page once the last built card is this close to the end
/// of what is loaded: about four rows at the widest, more when narrow.
pub(crate) const PREFETCH_ITEMS: usize = 40;

/// Identifies one request. Only the latest ticket for a slot applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Ticket(u64);

/// Why something could not be loaded. Copy is fixed; no server text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LibraryFailure {
    SignedOut,
    Unreachable,
    Unreadable,
}

impl LibraryFailure {
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
            Self::Unreadable => "This library couldn't be loaded",
        }
    }

    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::SignedOut => "Sign in again to keep browsing.",
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
        kind: LibraryKind,
        query: LibraryQuery,
        page: LibraryPageRequest,
    },
    Views {
        ticket: Ticket,
    },
    Genres {
        ticket: Ticket,
        kind: LibraryKind,
        view: Option<LibraryId>,
    },
    /// One title again, after playback may have changed its progress.
    Item {
        ticket: Ticket,
        kind: LibraryKind,
        id: ItemId,
    },
}

/// An answer, already mapped out of the Jellyfin client.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Response {
    Page(Result<LibraryPage, LibraryFailure>),
    Views(Result<Vec<LibraryView>, LibraryFailure>),
    Genres(Result<Vec<LibraryGenre>, LibraryFailure>),
    Item(Result<Box<MediaItem>, LibraryFailure>),
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
    /// Something else changed (a failure, the libraries, the genres, one
    /// title's progress).
    Updated,
    /// Jellyfin no longer accepts the session. The application decides what
    /// that means; Library only reports it.
    SessionExpired,
}

/// Metadata that feeds a menu: libraries or genres.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Fetch<T> {
    Loading,
    Ready(T),
    /// The menu is left out; browsing still works.
    Failed,
}

/// What the grid area shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CatalogState {
    /// The first page of this query has not arrived.
    Loading,
    Ready,
    Empty(EmptyReason),
    /// The first page failed. Offers Try again.
    Failed(LibraryFailure),
}

/// Why there is nothing to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmptyReason {
    /// The library has no titles of this kind.
    Library,
    /// Titles exist, but none match the filters.
    Filtered,
}

impl EmptyReason {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Library => "Nothing is in this library yet.",
            Self::Filtered => "Nothing matches these filters.",
        }
    }
}

/// What sits under the last row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Footer {
    /// More remain and none is being asked for yet.
    Idle,
    Loading,
    /// The next page failed; what is loaded stays. Offers Try again.
    Failed(LibraryFailure),
    /// The server has nothing more.
    End,
}

#[derive(Clone, Debug)]
struct Catalog {
    kind: LibraryKind,
    query: LibraryQuery,
    started: bool,
    items: Vec<MediaItem>,
    ids: HashSet<ItemId>,
    total: Option<usize>,
    next_start: usize,
    has_more: bool,
    state: CatalogState,
    /// The page being waited for, and where it starts.
    pending: Option<(Ticket, usize)>,
    more_failed: Option<LibraryFailure>,
    genres: Fetch<Vec<LibraryGenre>>,
    genres_ticket: Option<Ticket>,
    focused: Option<ItemId>,
    opened: Option<ItemId>,
    reconcile: Option<Ticket>,
    pages_requested: usize,
}

impl Catalog {
    fn new(kind: LibraryKind) -> Self {
        Self {
            kind,
            query: LibraryQuery::new(kind),
            started: false,
            items: Vec::new(),
            ids: HashSet::new(),
            total: None,
            next_start: 0,
            has_more: false,
            state: CatalogState::Loading,
            pending: None,
            more_failed: None,
            genres: Fetch::Loading,
            genres_ticket: None,
            focused: None,
            opened: None,
            reconcile: None,
            pages_requested: 0,
        }
    }

    /// Forget the titles of the previous query. The next answer for this
    /// catalog must be a new first page.
    fn clear(&mut self) {
        self.items.clear();
        self.ids.clear();
        self.total = None;
        self.next_start = 0;
        self.has_more = false;
        self.state = CatalogState::Loading;
        self.more_failed = None;
        self.focused = None;
        self.reconcile = None;
    }

    fn empty_reason(&self) -> EmptyReason {
        if self.query.filter.is_active() {
            EmptyReason::Filtered
        } else {
            EmptyReason::Library
        }
    }
}

/// Counters for reviewing how Library uses the network and memory. Not
/// shown in the UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryStats {
    pub items: usize,
    pub pages_requested: usize,
    pub total: Option<usize>,
}

pub(crate) struct LibraryModel {
    issued: u64,
    kind: LibraryKind,
    movies: Catalog,
    series: Catalog,
    views: Fetch<Vec<LibraryView>>,
    views_ticket: Option<Ticket>,
}

impl LibraryModel {
    pub(crate) fn new(kind: LibraryKind) -> Self {
        Self {
            issued: 0,
            kind,
            movies: Catalog::new(LibraryKind::Movies),
            series: Catalog::new(LibraryKind::Series),
            views: Fetch::Loading,
            views_ticket: None,
        }
    }

    /// A model showing `kind`, and the requests that start it.
    pub(crate) fn open(kind: LibraryKind) -> (Self, Vec<Request>) {
        let mut model = Self::new(kind);
        let requests = model.show(kind);
        (model, requests)
    }

    /// Show `kind`. The first time, its first page and genres are asked
    /// for (and the libraries, once for both kinds). Afterwards it is shown
    /// exactly as it was left.
    pub(crate) fn show(&mut self, kind: LibraryKind) -> Vec<Request> {
        self.kind = kind;
        let mut requests = Vec::new();
        if self.views_ticket.is_none() && matches!(self.views, Fetch::Loading) {
            requests.push(self.views_request());
        }
        if !self.catalog().started {
            self.catalog_mut().started = true;
            requests.push(self.first_page());
            requests.push(self.genres_request());
        }
        requests
    }

    pub(crate) fn kind(&self) -> LibraryKind {
        self.kind
    }

    pub(crate) fn query(&self) -> &LibraryQuery {
        &self.catalog().query
    }

    pub(crate) fn items(&self) -> &[MediaItem] {
        &self.catalog().items
    }

    /// The server's count for the whole query, once the first page says.
    pub(crate) fn total(&self) -> Option<usize> {
        self.catalog().total
    }

    pub(crate) fn state(&self) -> CatalogState {
        self.catalog().state
    }

    pub(crate) fn footer(&self) -> Footer {
        let catalog = self.catalog();
        if catalog.state != CatalogState::Ready {
            return Footer::Idle;
        }
        match (catalog.more_failed, catalog.pending, catalog.has_more) {
            (Some(failure), _, _) => Footer::Failed(failure),
            (None, Some(_), _) => Footer::Loading,
            (None, None, true) => Footer::Idle,
            (None, None, false) => Footer::End,
        }
    }

    /// Whether any request for the current kind is in flight.
    pub(crate) fn is_loading(&self) -> bool {
        let catalog = self.catalog();
        catalog.pending.is_some() || catalog.genres_ticket.is_some() || self.views_ticket.is_some()
    }

    /// Libraries that can hold the current kind, in server order. Shown as
    /// a menu only when there is more than one.
    pub(crate) fn views(&self) -> Vec<&LibraryView> {
        match &self.views {
            Fetch::Ready(views) => views
                .iter()
                .filter(|view| view.content.holds(self.kind))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The selected library's name, or `None` for every library.
    pub(crate) fn view_name(&self) -> Option<&str> {
        let selected = self.query().view.as_ref()?;
        match &self.views {
            Fetch::Ready(views) => views
                .iter()
                .find(|view| &view.id == selected)
                .map(|view| view.name.as_str()),
            _ => None,
        }
    }

    pub(crate) fn genres(&self) -> &Fetch<Vec<LibraryGenre>> {
        &self.catalog().genres
    }

    /// The selected genre's name.
    pub(crate) fn genre_name(&self) -> Option<&str> {
        let selected = self.query().filter.genre.as_ref()?;
        match self.genres() {
            Fetch::Ready(genres) => genres
                .iter()
                .find(|genre| &genre.id == selected)
                .map(|genre| genre.name.as_str()),
            _ => None,
        }
    }

    pub(crate) fn stats(&self) -> LibraryStats {
        let catalog = self.catalog();
        LibraryStats {
            items: self.movies.items.len() + self.series.items.len(),
            pages_requested: self.movies.pages_requested + self.series.pages_requested,
            total: catalog.total,
        }
    }

    pub(crate) fn set_sort(&mut self, sort: LibrarySort) -> Vec<Request> {
        if self.query().sort == sort {
            return Vec::new();
        }
        self.catalog_mut().query.sort = sort;
        self.requery()
    }

    pub(crate) fn set_watch(&mut self, watch: WatchFilter) -> Vec<Request> {
        if self.query().filter.watch == watch {
            return Vec::new();
        }
        self.catalog_mut().query.filter.watch = watch;
        self.requery()
    }

    pub(crate) fn set_genre(&mut self, genre: Option<ItemId>) -> Vec<Request> {
        if self.query().filter.genre == genre {
            return Vec::new();
        }
        self.catalog_mut().query.filter.genre = genre;
        self.requery()
    }

    /// Choose one library, or every library. The genre filter is cleared:
    /// the genre list is the new library's.
    pub(crate) fn set_view(&mut self, view: Option<LibraryId>) -> Vec<Request> {
        if self.query().view == view {
            return Vec::new();
        }
        let catalog = self.catalog_mut();
        catalog.query.view = view;
        catalog.query.filter.genre = None;
        catalog.genres = Fetch::Loading;
        let mut requests = self.requery();
        requests.push(self.genres_request());
        requests
    }

    pub(crate) fn clear_filters(&mut self) -> Vec<Request> {
        if !self.query().filter.is_active() {
            return Vec::new();
        }
        self.catalog_mut().query.filter = LibraryFilter::default();
        self.requery()
    }

    /// Ask again from the first page with the same selections. The titles
    /// on screen stay until it arrives, then replace every page. The menus'
    /// libraries and genres are asked for again too.
    pub(crate) fn refresh(&mut self) -> Vec<Request> {
        let requests = vec![
            self.first_page(),
            self.genres_request(),
            self.views_request(),
        ];
        let catalog = self.catalog_mut();
        catalog.more_failed = None;
        catalog.reconcile = None;
        if catalog.state != CatalogState::Ready {
            catalog.state = CatalogState::Loading;
        }
        requests
    }

    /// Try again after a failure: the first page, or the page that failed.
    pub(crate) fn retry(&mut self) -> Vec<Request> {
        let catalog = self.catalog();
        if matches!(catalog.state, CatalogState::Failed(_)) {
            self.catalog_mut().state = CatalogState::Loading;
            return vec![self.first_page()];
        }
        if catalog.more_failed.is_some() {
            self.catalog_mut().more_failed = None;
            return self.next_page().into_iter().collect();
        }
        Vec::new()
    }

    /// The last built card is `last_built`: ask for the next page when it is
    /// near the end of what is loaded. Nothing while a page is in flight,
    /// after the end, or after a failure (that waits for Try again).
    pub(crate) fn want_more(&mut self, last_built: usize) -> Option<Request> {
        let catalog = self.catalog();
        if catalog.state != CatalogState::Ready
            || !catalog.has_more
            || catalog.pending.is_some()
            || catalog.more_failed.is_some()
            || last_built + PREFETCH_ITEMS < catalog.items.len()
        {
            return None;
        }
        self.next_page()
    }

    pub(crate) fn note_focused(&mut self, id: ItemId) {
        self.catalog_mut().focused = Some(id);
    }

    pub(crate) fn focused(&self) -> Option<&ItemId> {
        self.catalog().focused.as_ref()
    }

    /// The focused title's place in the loaded list, if it is still there.
    pub(crate) fn focused_index(&self) -> Option<usize> {
        let id = self.focused()?;
        self.items().iter().position(|item| item.id() == id)
    }

    pub(crate) fn note_opened(&mut self, id: ItemId) {
        let catalog = self.catalog_mut();
        catalog.focused = Some(id.clone());
        catalog.opened = Some(id);
    }

    /// After playback: ask for the opened title again, so its progress and
    /// watched state are current. One request, not a reload.
    pub(crate) fn reconcile(&mut self) -> Option<Request> {
        let id = self.catalog().opened.clone()?;
        if !self.catalog().ids.contains(&id) {
            return None;
        }
        let ticket = self.ticket();
        self.catalog_mut().reconcile = Some(ticket);
        Some(Request::Item {
            ticket,
            kind: self.kind,
            id,
        })
    }

    pub(crate) fn apply(&mut self, request: &Request, response: Response) -> Applied {
        let failure = match &response {
            Response::Page(Err(failure))
            | Response::Views(Err(failure))
            | Response::Genres(Err(failure))
            | Response::Item(Err(failure)) => Some(*failure),
            _ => None,
        };
        let current = match request {
            Request::Page { ticket, kind, .. } => {
                self.catalog_for(*kind).pending.map(|(t, _)| t) == Some(*ticket)
            }
            Request::Views { ticket } => self.views_ticket == Some(*ticket),
            Request::Genres { ticket, kind, .. } => {
                self.catalog_for(*kind).genres_ticket == Some(*ticket)
            }
            Request::Item { ticket, kind, .. } => {
                self.catalog_for(*kind).reconcile == Some(*ticket)
            }
        };
        if !current {
            return Applied::Ignored;
        }
        if failure == Some(LibraryFailure::SignedOut) {
            // Not a library failure: the session is gone. What is shown
            // stays, and the shell ends the session.
            return Applied::SessionExpired;
        }
        match (request, response) {
            (Request::Page { kind, page, .. }, Response::Page(result)) => {
                self.apply_page(*kind, *page, result)
            }
            (Request::Views { .. }, Response::Views(result)) => {
                self.views_ticket = None;
                self.views = match result {
                    Ok(views) => Fetch::Ready(views),
                    Err(_) if matches!(self.views, Fetch::Ready(_)) => return Applied::Updated,
                    Err(_) => Fetch::Failed,
                };
                Applied::Updated
            }
            (Request::Genres { kind, .. }, Response::Genres(result)) => {
                let catalog = self.catalog_for_mut(*kind);
                catalog.genres_ticket = None;
                catalog.genres = match result {
                    Ok(genres) => Fetch::Ready(genres),
                    Err(_) if matches!(catalog.genres, Fetch::Ready(_)) => {
                        return Applied::Updated;
                    }
                    Err(_) => Fetch::Failed,
                };
                Applied::Updated
            }
            (Request::Item { kind, .. }, Response::Item(result)) => {
                let catalog = self.catalog_for_mut(*kind);
                catalog.reconcile = None;
                if let Ok(item) = result
                    && let Some(slot) = catalog.items.iter_mut().find(|old| old.id() == item.id())
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
        kind: LibraryKind,
        page: LibraryPageRequest,
        result: Result<LibraryPage, LibraryFailure>,
    ) -> Applied {
        let catalog = self.catalog_for_mut(kind);
        catalog.pending = None;
        let first = page.start == 0;
        match result {
            // A failed refresh keeps the titles on screen.
            Err(_) if first && catalog.state == CatalogState::Ready => Applied::Updated,
            Err(failure) if first => {
                catalog.state = CatalogState::Failed(failure);
                Applied::Updated
            }
            Err(failure) => {
                catalog.more_failed = Some(failure);
                Applied::Updated
            }
            Ok(loaded) => {
                let has_more = loaded.has_more(page.limit);
                let received = loaded.items.len();
                if first {
                    catalog.items.clear();
                    catalog.ids.clear();
                    catalog.more_failed = None;
                }
                for item in loaded.items {
                    // Titles can shift between pages while the library
                    // changes. A title already shown is not shown twice.
                    if catalog.ids.insert(item.id().clone()) {
                        catalog.items.push(item);
                    }
                }
                catalog.total = loaded.total.or(catalog.total);
                catalog.next_start = page.start + received;
                catalog.has_more = has_more;
                catalog.state = if catalog.items.is_empty() {
                    CatalogState::Empty(catalog.empty_reason())
                } else {
                    CatalogState::Ready
                };
                if first {
                    if let Some(focused) = &catalog.focused
                        && !catalog.ids.contains(focused)
                    {
                        catalog.focused = None;
                    }
                    Applied::Replaced
                } else {
                    Applied::Appended
                }
            }
        }
    }

    /// A new query: new tickets, nothing of the old result kept.
    fn requery(&mut self) -> Vec<Request> {
        self.catalog_mut().clear();
        vec![self.first_page()]
    }

    fn first_page(&mut self) -> Request {
        self.page_request(0)
    }

    fn next_page(&mut self) -> Option<Request> {
        let start = self.catalog().next_start;
        Some(self.page_request(start))
    }

    fn page_request(&mut self, start: usize) -> Request {
        let ticket = self.ticket();
        let kind = self.kind;
        let catalog = self.catalog_mut();
        catalog.pending = Some((ticket, start));
        catalog.pages_requested += 1;
        Request::Page {
            ticket,
            kind,
            query: catalog.query.clone(),
            page: LibraryPageRequest {
                start,
                limit: PAGE_SIZE,
            },
        }
    }

    fn genres_request(&mut self) -> Request {
        let ticket = self.ticket();
        let kind = self.kind;
        let catalog = self.catalog_mut();
        catalog.genres_ticket = Some(ticket);
        Request::Genres {
            ticket,
            kind,
            view: catalog.query.view.clone(),
        }
    }

    fn views_request(&mut self) -> Request {
        let ticket = self.ticket();
        self.views_ticket = Some(ticket);
        Request::Views { ticket }
    }

    fn ticket(&mut self) -> Ticket {
        self.issued += 1;
        Ticket(self.issued)
    }

    fn catalog(&self) -> &Catalog {
        self.catalog_for(self.kind)
    }

    fn catalog_mut(&mut self) -> &mut Catalog {
        self.catalog_for_mut(self.kind)
    }

    fn catalog_for(&self, kind: LibraryKind) -> &Catalog {
        match kind {
            LibraryKind::Movies => &self.movies,
            LibraryKind::Series => &self.series,
        }
    }

    fn catalog_for_mut(&mut self, kind: LibraryKind) -> &mut Catalog {
        let catalog = match kind {
            LibraryKind::Movies => &mut self.movies,
            LibraryKind::Series => &mut self.series,
        };
        debug_assert_eq!(catalog.kind, kind);
        catalog
    }
}

/// `1,284 titles`.
pub(crate) fn count_label(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    match count {
        1 => "1 title".into(),
        _ => format!("{grouped} titles"),
    }
}
