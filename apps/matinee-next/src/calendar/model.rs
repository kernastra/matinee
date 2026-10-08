//! Calendar state. No GPUI, no HTTP, and no clock of its own: the caller
//! passes `now` and the zone is a type parameter, so every rule is testable
//! for any offset and instant.
//!
//! Each source (Radarr, Sonarr) has its own link state, its own request in
//! flight, its own failure, and its own events. A slow source never holds up
//! another, and a response that is no longer asked for is ignored by its
//! ticket.
//!
//! The screen does three things with this model: it calls an action
//! (`next_month`, `select`, `refresh`, and so on), calls [`CalendarModel::plan`]
//! to learn which requests are needed now, and calls [`CalendarModel::apply`]
//! with each answer.
//!
//! Events are kept per day. An answer for a month replaces every event on
//! that month's grid days, so a release a server dropped disappears on the
//! next refresh. Events on days no retained month covers are dropped.

use chrono::{DateTime, NaiveDate, TimeDelta, TimeZone, Utc};
use matinee_integrations::{IntegrationError, IntegrationProvider, UpcomingRelease};

use super::event::{CalendarEvent, MediaFilter};
use super::grid::{Window, month_start, same_day_in_month, shift_month};

/// Months kept per source. The shown month plus three others: moving back
/// and forth between neighbours asks nothing again.
pub(crate) const RETAINED_WINDOWS: usize = 4;

/// How long an answer counts as current. The shipping calendar's cache
/// lives five minutes too. Past that, the month is asked for again when it
/// is shown; its events stay on screen until the new answer replaces them.
pub(crate) const FRESH_FOR: TimeDelta = TimeDelta::minutes(5);

/// Both sources, in the order the bar and the status line show them.
pub(crate) const PROVIDERS: [IntegrationProvider; 2] =
    [IntegrationProvider::Radarr, IntegrationProvider::Sonarr];

fn slot(provider: IntegrationProvider) -> usize {
    match provider {
        IntegrationProvider::Radarr => 0,
        IntegrationProvider::Sonarr => 1,
    }
}

/// Identifies one request. Only the ticket a source is waiting on applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Ticket(u64);

#[cfg(test)]
impl Ticket {
    pub(crate) fn for_test(value: u64) -> Self {
        Self(value)
    }
}

/// Why a source could not answer. Carries no server text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceFailure {
    /// No saved connection for this source.
    Unlinked,
    /// The server refused the saved key.
    Unauthorized,
    /// The server could not be reached or failed.
    Unavailable,
    /// The server answered with something that is not a calendar.
    Malformed,
}

impl SourceFailure {
    pub(crate) fn from_error(error: &IntegrationError) -> Self {
        match error {
            IntegrationError::NotConfigured { .. } => Self::Unlinked,
            IntegrationError::AuthenticationRejected { .. } => Self::Unauthorized,
            IntegrationError::MalformedResponse { .. } => Self::Malformed,
            _ => Self::Unavailable,
        }
    }
}

/// Work the screen runs on the service runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    /// Whether a saved connection exists for this source.
    Link {
        ticket: Ticket,
        provider: IntegrationProvider,
    },
    /// Releases for every day of `window`, asked for in one request.
    Releases {
        ticket: Ticket,
        provider: IntegrationProvider,
        window: Window,
    },
}

impl Request {
    pub(crate) fn ticket(&self) -> Ticket {
        match self {
            Self::Link { ticket, .. } | Self::Releases { ticket, .. } => *ticket,
        }
    }
}

/// An answer, ready for the model. `at` is when it arrived.
#[derive(Debug)]
pub(crate) enum Response {
    Link {
        ticket: Ticket,
        provider: IntegrationProvider,
        result: Result<bool, SourceFailure>,
    },
    Releases {
        ticket: Ticket,
        provider: IntegrationProvider,
        at: DateTime<Utc>,
        result: Result<Vec<UpcomingRelease>, SourceFailure>,
    },
}

impl Response {
    pub(crate) fn ticket(&self) -> Ticket {
        match self {
            Self::Link { ticket, .. } | Self::Releases { ticket, .. } => *ticket,
        }
    }
}

/// What applying an answer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Applied {
    /// No request was waiting on this ticket. Nothing changed.
    Ignored,
    Updated,
}

/// What a source's connection is, as far as the calendar knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Link {
    /// Not read yet.
    Checking,
    Connected,
    Disconnected,
    /// The saved connection could not be read.
    Failed(SourceFailure),
}

impl Default for Link {
    fn default() -> Self {
        Self::Checking
    }
}

/// What one source shows for the displayed month.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceStatus {
    Checking,
    Unlinked,
    /// Connected and the month is being asked for.
    Loading,
    /// The month is answered. Events may still be none.
    Ready,
    Failed(SourceFailure),
}

/// What the calendar as a whole has to say about its connections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Overview {
    /// Neither source is connected yet, and at least one is still being read.
    Checking,
    /// Neither source is connected.
    NothingConnected,
    /// At least one source is connected.
    Connected,
}

#[derive(Debug, Default)]
struct Source {
    link: Link,
    /// The link check in flight.
    link_ticket: Option<Ticket>,
    /// The month request in flight, and the month it is for.
    fetch: Option<Fetch>,
    /// The last failed month, cleared on refresh and on a new visit.
    failure: Option<(Window, SourceFailure)>,
    /// Answered months, least recently used first.
    loaded: Vec<Loaded>,
    /// Events on the days of the loaded months.
    events: Vec<CalendarEvent>,
}

#[derive(Clone, Copy, Debug)]
struct Fetch {
    ticket: Ticket,
    window: Window,
}

#[derive(Clone, Copy, Debug)]
struct Loaded {
    window: Window,
    fetched_at: DateTime<Utc>,
}

impl Source {
    fn fresh(&self, window: Window, now: DateTime<Utc>) -> bool {
        self.loaded
            .iter()
            .any(|loaded| loaded.window == window && now - loaded.fetched_at < FRESH_FOR)
    }

    fn touch(&mut self, window: Window) {
        if let Some(index) = self
            .loaded
            .iter()
            .position(|loaded| loaded.window == window)
        {
            let entry = self.loaded.remove(index);
            self.loaded.push(entry);
        }
    }
}

pub(crate) struct CalendarModel<Z: TimeZone> {
    zone: Z,
    /// The current instant, as last observed.
    clock: DateTime<Utc>,
    today: NaiveDate,
    /// The first day of the shown month.
    month: NaiveDate,
    selected: NaiveDate,
    /// The event the person chose, by id. Absent means the first of the day.
    focused: Option<String>,
    filter: MediaFilter,
    sources: [Source; 2],
    last_updated: Option<DateTime<Utc>>,
    next_ticket: u64,
    /// Read every link again at the next plan.
    recheck_links: bool,
}

impl<Z: TimeZone> CalendarModel<Z> {
    /// A calendar for the month of `now`, with today selected.
    pub(crate) fn new(zone: Z, now: DateTime<Utc>) -> Self {
        let today = now.with_timezone(&zone).date_naive();
        Self {
            zone,
            clock: now,
            today,
            month: month_start(today),
            selected: today,
            focused: None,
            filter: MediaFilter::All,
            sources: Default::default(),
            last_updated: None,
            next_ticket: 0,
            recheck_links: true,
        }
    }

    /// The instant is now `now`. Moves the notion of today when the day has
    /// changed, but neither the month nor the selection.
    pub(crate) fn observe_now(&mut self, now: DateTime<Utc>) {
        self.clock = now;
        self.today = now.with_timezone(&self.zone).date_naive();
    }

    /// The calendar is shown again. Reads every connection, and clears
    /// earlier failures so the shown month is asked for again.
    pub(crate) fn show(&mut self) {
        self.recheck_links = true;
        for source in &mut self.sources {
            source.failure = None;
        }
    }

    /// Everything in flight is stale. Each source's connection and the shown
    /// month are asked for again by the next plan. Events stay on screen until
    /// the answers replace them.
    pub(crate) fn refresh(&mut self) {
        self.recheck_links = true;
        for source in &mut self.sources {
            source.link_ticket = None;
            source.fetch = None;
            source.failure = None;
            source.loaded.clear();
        }
    }

    pub(crate) fn today(&self) -> NaiveDate {
        self.today
    }

    pub(crate) fn month(&self) -> NaiveDate {
        self.month
    }

    pub(crate) fn selected(&self) -> NaiveDate {
        self.selected
    }

    pub(crate) fn filter(&self) -> MediaFilter {
        self.filter
    }

    pub(crate) fn window(&self) -> Window {
        Window::for_month(self.month)
    }

    /// When the newest answer arrived.
    pub(crate) fn last_updated(&self) -> Option<DateTime<Utc>> {
        self.last_updated
    }

    pub(crate) fn link(&self, provider: IntegrationProvider) -> Link {
        self.sources[slot(provider)].link
    }

    pub(crate) fn overview(&self) -> Overview {
        let links: Vec<Link> = PROVIDERS.iter().map(|p| self.link(*p)).collect();
        if links.contains(&Link::Connected) {
            Overview::Connected
        } else if links.contains(&Link::Checking) {
            Overview::Checking
        } else {
            Overview::NothingConnected
        }
    }

    /// What `provider` shows for the displayed month.
    pub(crate) fn status(&self, provider: IntegrationProvider) -> SourceStatus {
        let source = &self.sources[slot(provider)];
        match source.link {
            Link::Checking => SourceStatus::Checking,
            Link::Disconnected => SourceStatus::Unlinked,
            Link::Failed(failure) => SourceStatus::Failed(failure),
            Link::Connected => {
                let window = self.window();
                if source.fresh(window, self.clock) {
                    SourceStatus::Ready
                } else if let Some((failed, failure)) = source.failure
                    && failed == window
                {
                    SourceStatus::Failed(failure)
                } else {
                    SourceStatus::Loading
                }
            }
        }
    }

    /// Whether an answer for `ticket` would still apply. The screen stops the
    /// work of every ticket that is no longer waited on.
    pub(crate) fn waiting_on(&self, ticket: Ticket) -> bool {
        self.sources.iter().any(|source| {
            source.link_ticket == Some(ticket)
                || source.fetch.is_some_and(|fetch| fetch.ticket == ticket)
        })
    }

    /// Whether any request is in flight.
    pub(crate) fn busy(&self) -> bool {
        self.sources
            .iter()
            .any(|source| source.link_ticket.is_some() || source.fetch.is_some())
    }

    /// Whether every connected source has answered the shown month. Only then
    /// is an empty day known to be empty.
    pub(crate) fn settled(&self) -> bool {
        PROVIDERS.iter().all(|provider| {
            !matches!(
                self.status(*provider),
                SourceStatus::Loading | SourceStatus::Checking
            ) && !matches!(self.status(*provider), SourceStatus::Failed(_))
        })
    }

    pub(crate) fn next_month(&mut self) {
        self.set_month(shift_month(self.month, 1));
    }

    pub(crate) fn previous_month(&mut self) {
        self.set_month(shift_month(self.month, -1));
    }

    /// Today's month, with today selected.
    pub(crate) fn go_to_today(&mut self) {
        self.month = month_start(self.today);
        self.select(self.today);
    }

    /// Select a day. Choosing one outside the shown month shows its month.
    pub(crate) fn select(&mut self, day: NaiveDate) {
        if self.selected != day {
            self.focused = None;
        }
        self.selected = day;
        self.month = month_start(day);
    }

    pub(crate) fn set_filter(&mut self, filter: MediaFilter) {
        self.filter = filter;
        if let Some(id) = self.focused.clone()
            && !self
                .day_events(self.selected)
                .iter()
                .any(|event| event.id == id)
        {
            self.focused = None;
        }
    }

    /// Choose one of the selected day's events. Unknown ids are ignored.
    pub(crate) fn focus_event(&mut self, id: &str) {
        if self
            .day_events(self.selected)
            .iter()
            .any(|event| event.id == id)
        {
            self.focused = Some(id.to_string());
        }
    }

    /// The event whose details the panel shows: the chosen one if it is on the
    /// selected day, otherwise the first.
    pub(crate) fn focused_event(&self) -> Option<&CalendarEvent> {
        let events = self.day_events(self.selected);
        self.focused
            .as_deref()
            .and_then(|id| events.iter().copied().find(|event| event.id == id))
            .or_else(|| events.first().copied())
    }

    /// The selected day's events, in display order, for the sources that are
    /// shown and the filter that is set.
    pub(crate) fn day_events(&self, day: NaiveDate) -> Vec<&CalendarEvent> {
        let mut events: Vec<&CalendarEvent> = self.events_where(|event| event.day == day);
        events.sort_by(|left, right| left.day_order(right));
        events
    }

    /// Every event on a grid day of the shown month, for the grid's chips.
    pub(crate) fn grid_events(&self) -> Vec<&CalendarEvent> {
        let window = self.window();
        self.events_where(|event| window.contains(event.day))
    }

    /// How many releases fall in the shown calendar month.
    pub(crate) fn month_release_count(&self) -> usize {
        let month = self.month;
        self.events_where(|event| month_start(event.day) == month)
            .len()
    }

    fn events_where(&self, keep: impl Fn(&CalendarEvent) -> bool) -> Vec<&CalendarEvent> {
        self.sources
            .iter()
            .flat_map(|source| source.events.iter())
            .filter(|event| self.filter.admits(event.source) && keep(event))
            .collect()
    }

    fn set_month(&mut self, month: NaiveDate) {
        self.month = month;
        self.selected = same_day_in_month(self.selected, month);
        self.focused = None;
    }

    fn ticket(&mut self) -> Ticket {
        self.next_ticket += 1;
        Ticket(self.next_ticket)
    }

    /// The requests needed now and not already in flight, answered, or
    /// failed for this month. Call it after every action and every answer,
    /// never on render.
    pub(crate) fn plan(&mut self) -> Vec<Request> {
        let window = self.window();
        let recheck = std::mem::take(&mut self.recheck_links);
        let mut requests = Vec::new();
        for provider in PROVIDERS {
            let index = slot(provider);
            let needs_link = recheck || self.sources[index].link == Link::Checking;
            if needs_link && self.sources[index].link_ticket.is_none() {
                let ticket = self.ticket();
                self.sources[index].link_ticket = Some(ticket);
                requests.push(Request::Link { ticket, provider });
            }
        }
        for provider in PROVIDERS {
            let index = slot(provider);
            if self.sources[index].link != Link::Connected {
                continue;
            }
            self.sources[index].touch(window);
            let source = &self.sources[index];
            let asked = source.fetch.is_some_and(|fetch| fetch.window == window);
            let failed = source.failure.is_some_and(|(failed, _)| failed == window);
            if asked || failed || source.fresh(window, self.clock) {
                continue;
            }
            let ticket = self.ticket();
            self.sources[index].fetch = Some(Fetch { ticket, window });
            requests.push(Request::Releases {
                ticket,
                provider,
                window,
            });
        }
        requests
    }

    /// Apply one answer. Only the ticket a source is waiting on applies.
    pub(crate) fn apply(&mut self, response: Response) -> Applied {
        match response {
            Response::Link {
                ticket,
                provider,
                result,
            } => self.apply_link(ticket, provider, result),
            Response::Releases {
                ticket,
                provider,
                at,
                result,
            } => self.apply_releases(ticket, provider, at, result),
        }
    }

    fn apply_link(
        &mut self,
        ticket: Ticket,
        provider: IntegrationProvider,
        result: Result<bool, SourceFailure>,
    ) -> Applied {
        let source = &mut self.sources[slot(provider)];
        if source.link_ticket != Some(ticket) {
            return Applied::Ignored;
        }
        source.link_ticket = None;
        match result {
            Ok(true) => source.link = Link::Connected,
            Ok(false) => {
                source.link = Link::Disconnected;
                disconnect(source);
            }
            Err(failure) => source.link = Link::Failed(failure),
        }
        Applied::Updated
    }

    fn apply_releases(
        &mut self,
        ticket: Ticket,
        provider: IntegrationProvider,
        at: DateTime<Utc>,
        result: Result<Vec<UpcomingRelease>, SourceFailure>,
    ) -> Applied {
        let index = slot(provider);
        let Some(fetch) = self.sources[index].fetch else {
            return Applied::Ignored;
        };
        if fetch.ticket != ticket {
            return Applied::Ignored;
        }
        self.sources[index].fetch = None;
        let window = fetch.window;
        match result {
            Ok(releases) => {
                let events = normalize(&releases, window, &self.zone);
                let source = &mut self.sources[index];
                source.events.retain(|event| !window.contains(event.day));
                source.events.extend(events);
                source.failure = None;
                source.loaded.retain(|loaded| loaded.window != window);
                source.loaded.push(Loaded {
                    window,
                    fetched_at: at,
                });
                self.evict(index);
                self.last_updated = Some(at);
            }
            Err(SourceFailure::Unlinked) => {
                let source = &mut self.sources[index];
                source.link = Link::Disconnected;
                disconnect(source);
            }
            Err(failure) => {
                self.sources[index].failure = Some((window, failure));
            }
        }
        Applied::Updated
    }

    /// Keep the most recent months, never the shown one, and drop events no
    /// kept month covers.
    fn evict(&mut self, index: usize) {
        let shown = self.window();
        let source = &mut self.sources[index];
        while source.loaded.len() > RETAINED_WINDOWS {
            let Some(oldest) = source
                .loaded
                .iter()
                .position(|loaded| loaded.window != shown)
            else {
                break;
            };
            source.loaded.remove(oldest);
        }
        let kept: Vec<Window> = source.loaded.iter().map(|loaded| loaded.window).collect();
        source
            .events
            .retain(|event| kept.iter().any(|window| window.contains(event.day)));
    }
}

/// Events for the grid days of `window` from one answer. One row per id
/// (the first wins), placed by the zone, and only on days the window shows.
fn normalize<Z: TimeZone>(
    releases: &[UpcomingRelease],
    window: Window,
    zone: &Z,
) -> Vec<CalendarEvent> {
    let mut seen = std::collections::HashSet::new();
    releases
        .iter()
        .filter_map(|release| CalendarEvent::from_release(release, zone))
        .filter(|event| window.contains(event.day))
        .filter(|event| seen.insert(event.id.clone()))
        .collect()
}

/// A source lost its connection: nothing it sent is shown any more.
fn disconnect(source: &mut Source) {
    source.link_ticket = None;
    source.fetch = None;
    source.failure = None;
    source.loaded.clear();
    source.events.clear();
}
