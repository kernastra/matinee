//! Calendar screen.
//!
//! Paints [`CalendarModel`] as a month of seven columns in Atelier's
//! [`VirtualGrid`], beside the releases of the selected day. The grid gives
//! the month its keyboard behaviour (arrows, Home, End, Enter to select), and
//! it builds only the cells that are on screen. Covers load only for the
//! selected day's releases, never for the whole month.
//!
//! Calendar is a root destination. It stays alive while Details or the Player
//! covers it, so the month, the selected day, the filter, and the answers are
//! where the person left them. Calendar never opens Details: Radarr and
//! Sonarr identities are not Jellyfin identities, and no mapping is invented.
//!
//! The screen is the only holder of requests in flight. Each one is tied to a
//! ticket the model is waiting on. When the model stops waiting, the screen
//! aborts the request, and an answer that arrives late cannot apply.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use atelier_ui::gpui::{EventEmitter, Task};
use atelier_ui::prelude::*;
use chrono::{Days, Local, NaiveDate, Utc};
use matinee_integrations::{IntegrationProvider, ReleaseKind};
use tokio::task::JoinHandle;

use super::event::{CalendarEvent, MediaFilter};
use super::grid::{GRID_DAYS, Window as MonthWindow, month_start};
use super::load::{self, CalendarService};
use super::model::{
    CalendarModel, Overview, PROVIDERS, Request, Response, SourceFailure, SourceStatus, Ticket,
};
use crate::app_bar::{AppBar, BarAction, app_bar};
use crate::artwork::{Artwork, ArtworkLoad, ArtworkLoader, release_key};
use crate::nav::RootDestination;
use crate::runtime::ServiceRuntime;
use crate::tiles::art_frame;

/// What the shell does for Calendar.
pub(crate) enum CalendarScreenEvent {
    /// Go to another root destination from the app bar.
    Navigate(RootDestination),
    SignOut,
}

/// Gaps between day cells. The weekday row uses the same gap to line up.
const CELL_GAP: f32 = Space::S1.value();
/// A day cell's padding, and the gap between its lines.
const CELL_PADDING: f32 = Space::S1.value();
/// Lines below the day number that every cell has room for, however short
/// the window: two chips, or one and "+N more". At the 960 × 620 minimum
/// this is what lets all six weeks show without scrolling; taller windows
/// give each row more lines.
const MIN_CELL_LINES: usize = 2;
/// Weeks in the grid.
const GRID_ROWS: usize = (GRID_DAYS / 7) as usize;
/// Release panel width on wide windows. Narrow windows keep it too: the
/// standard minimum is 960 points, and the grid is still seven columns there.
const PANEL_WIDTH: f32 = 336.0;
/// Cover size in the release panel.
const COVER_WIDTH: f32 = 40.0;
const COVER_HEIGHT: f32 = 60.0;
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// A request the screen started and has not yet answered or abandoned.
struct InFlight {
    ticket: Ticket,
    task: JoinHandle<()>,
    /// Waits for the answer. Dropping it abandons the wait.
    wait: Option<Task<()>>,
}

/// Which way the month moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Previous,
    Next,
    Today,
}

pub(crate) struct CalendarScreen {
    runtime: Arc<ServiceRuntime>,
    /// `None` for review scenes, which never open a socket.
    service: Option<Arc<CalendarService>>,
    loader: ArtworkLoader,
    /// The signed-in name, for the app bar.
    name: String,
    pub(crate) model: CalendarModel<Local>,
    loads: Vec<InFlight>,
    /// Covers by address, for the selected day's releases only.
    pub(super) art: HashMap<String, Artwork>,
    art_tasks: HashMap<String, JoinHandle<()>>,
    /// Review scenes: one bundled cover stands in for every release.
    fixture_cover: Option<DecodedImage>,
    grid: VirtualGridState,
    /// The release panel's scroll position.
    panel_scroll: ScrollControl,
    /// One focus handle per release row, so the panel can scroll to the row
    /// the keyboard reaches.
    row_focus: Vec<FocusHandle>,
    /// The release row the panel last scrolled to.
    revealed_row: Option<usize>,
    focus: FocusHandle,
    settle_focus: bool,
    signing_out: bool,
}

impl EventEmitter<CalendarScreenEvent> for CalendarScreen {}

impl CalendarScreen {
    /// Calendar for a signed-in session. Nothing loads until the first render
    /// asks for it through [`Self::show`].
    pub(crate) fn open(
        runtime: Arc<ServiceRuntime>,
        service: Arc<CalendarService>,
        loader: ArtworkLoader,
        name: String,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_model(
            runtime,
            Some(service),
            loader,
            name,
            CalendarModel::new(Local, Utc::now()),
            cx,
        )
    }

    pub(super) fn with_model(
        runtime: Arc<ServiceRuntime>,
        service: Option<Arc<CalendarService>>,
        loader: ArtworkLoader,
        name: String,
        model: CalendarModel<Local>,
        cx: &mut Context<Self>,
    ) -> Self {
        // Covers this screen decoded and the cache did not keep leave the
        // atlas with the screen.
        cx.on_release(|screen: &mut Self, cx| {
            for art in screen.art.values() {
                if let Artwork::Ready(image) = art
                    && !screen.loader.is_cached(image)
                {
                    image.release(cx);
                }
            }
        })
        .detach();
        Self {
            runtime,
            service,
            loader,
            name,
            model,
            loads: Vec::new(),
            art: HashMap::new(),
            art_tasks: HashMap::new(),
            fixture_cover: None,
            grid: VirtualGridState::new(cx),
            panel_scroll: ScrollControl::new(),
            row_focus: Vec::new(),
            revealed_row: None,
            focus: cx.focus_handle(),
            settle_focus: true,
            signing_out: false,
        }
    }

    /// The root is shown. Reads each connection again, retries a failed
    /// month once its retry interval has passed, and asks for the shown month
    /// if it is missing or out of date.
    pub(crate) fn show(&mut self, cx: &mut Context<Self>) {
        self.model.show(Utc::now());
        self.settle_on_selected_day();
        self.drive(cx);
    }

    /// Put focus on the selected day when the screen next draws.
    pub(super) fn settle_on_selected_day(&mut self) {
        self.settle_focus = true;
        self.focus_selected_day();
    }

    /// The grid's focused cell.
    #[cfg(test)]
    pub(crate) fn focused_cell(&self) -> Option<usize> {
        self.grid.focused()
    }

    /// The grid's state, for the tests: its focused cell and its handle.
    #[cfg(test)]
    pub(super) fn grid_state(&self) -> VirtualGridState {
        self.grid.clone()
    }

    /// The release panel's scroll position, for the tests.
    #[cfg(test)]
    pub(super) fn panel_scroll(&self) -> &ScrollControl {
        &self.panel_scroll
    }

    pub(super) fn set_fixture_cover(&mut self, cover: Option<DecodedImage>) {
        self.fixture_cover = cover;
    }

    pub(crate) fn set_signing_out(&mut self, signing_out: bool, cx: &mut Context<Self>) {
        self.signing_out = signing_out;
        cx.notify();
    }

    pub(super) fn focus_selected_day(&self) {
        let index = self
            .model
            .window()
            .index_of(self.model.selected())
            .unwrap_or(0);
        self.grid.focus_index(Some(index));
    }

    /// Bring the requests in line with the model. Stops the work the model no
    /// longer waits on, starts what it now needs, and keeps covers for the
    /// selected day only. Call after every action and every answer, never on
    /// render.
    pub(super) fn drive(&mut self, cx: &mut Context<Self>) {
        self.model.observe_now(Utc::now());
        self.sync_artwork(cx);
        let Some(service) = self.service.clone() else {
            cx.notify();
            return;
        };
        let requests = self.model.plan();
        let model = &self.model;
        self.loads.retain_mut(|load| {
            if model.waiting_on(load.ticket) {
                return true;
            }
            load.task.abort();
            if let Some(wait) = load.wait.take() {
                drop(wait);
            }
            false
        });
        for request in requests {
            self.start(&service, request, cx);
        }
        cx.notify();
    }

    fn start(&mut self, service: &Arc<CalendarService>, request: Request, cx: &mut Context<Self>) {
        let ticket = request.ticket();
        let (task, answer) = load::spawn(&self.runtime, service, request);
        let wait = cx.spawn(async move |this, cx| {
            match answer.await {
                Ok(response) => this.update(cx, |this, cx| this.answer(response, cx)),
                // The task ended without replying. A wait the screen dropped
                // never gets here, so this is a request that still counts.
                Err(_) => this.update(cx, |this, cx| this.abandoned(ticket, cx)),
            }
            .ok();
        });
        self.loads.push(InFlight {
            ticket,
            task,
            wait: Some(wait),
        });
    }

    fn answer(&mut self, response: Response, cx: &mut Context<Self>) {
        let ticket = response.ticket();
        // This answer is the one the wait was for. Let it finish on its own,
        // rather than dropping the task that is delivering it.
        if let Some(load) = self.loads.iter_mut().find(|load| load.ticket == ticket)
            && let Some(wait) = load.wait.take()
        {
            wait.detach();
        }
        self.model.apply(response);
        self.drive(cx);
    }

    /// A request's task ended without an answer. The model stops waiting, so
    /// the source is not left loading with Refresh disabled.
    fn abandoned(&mut self, ticket: Ticket, cx: &mut Context<Self>) {
        if let Some(load) = self.loads.iter_mut().find(|load| load.ticket == ticket)
            && let Some(wait) = load.wait.take()
        {
            wait.detach();
        }
        self.model.abandon(ticket, Utc::now());
        self.drive(cx);
    }

    pub(super) fn step(&mut self, step: Step, cx: &mut Context<Self>) {
        match step {
            Step::Previous => self.model.previous_month(),
            Step::Next => self.model.next_month(),
            Step::Today => self.model.go_to_today(),
        }
        self.focus_selected_day();
        self.drive(cx);
    }

    /// Select the grid's `index`th day. A day of another month shows that
    /// month, and the grid's focus moves with the day to its new cell.
    pub(super) fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < GRID_DAYS as usize {
            self.model.select(self.model.window().day(index as u64));
            self.focus_selected_day();
            self.drive(cx);
        }
    }

    /// A movement key the grid could not follow inside its 42 days. Left and
    /// Right go on to the day before or after, across the week. Past the
    /// grid's first or last day, and Up or Down past its first or last week,
    /// the day reached is selected and its month is shown, focused on it.
    pub(super) fn edge(&mut self, step: GridStep, cx: &mut Context<Self>) {
        let Some(index) = self.grid.focused() else {
            return;
        };
        let window = self.model.window();
        let from = window.day(index as u64);
        let to = match step {
            GridStep::Left => from.pred_opt(),
            GridStep::Right => from.succ_opt(),
            GridStep::Up => from.checked_sub_days(Days::new(7)),
            GridStep::Down => from.checked_add_days(Days::new(7)),
            _ => None,
        };
        let Some(to) = to else {
            return;
        };
        match window.index_of(to) {
            Some(target) => {
                self.grid.focus_index(Some(target));
                cx.notify();
            }
            None => {
                self.model.select(to);
                self.focus_selected_day();
                self.drive(cx);
            }
        }
    }

    pub(crate) fn set_filter(&mut self, filter: MediaFilter, cx: &mut Context<Self>) {
        self.model.set_filter(filter);
        self.drive(cx);
    }

    fn focus_event(&mut self, id: &str, cx: &mut Context<Self>) {
        self.model.focus_event(id);
        cx.notify();
    }

    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.model.refresh();
        self.drive(cx);
    }

    fn bar_action(&mut self, action: BarAction, _: &mut Window, cx: &mut Context<Self>) {
        match action {
            BarAction::Go(RootDestination::Calendar) => {}
            BarAction::Go(destination) => cx.emit(CalendarScreenEvent::Navigate(destination)),
            BarAction::Refresh => self.refresh(cx),
            BarAction::SignOut => cx.emit(CalendarScreenEvent::SignOut),
        }
    }

    /// Covers for the selected day's releases, as the filter shows them.
    /// Others are let go: their loads stop and their images leave the atlas
    /// unless the shared cache keeps them. Runs from [`Self::drive`], so
    /// drawing never starts a download.
    pub(super) fn sync_artwork(&mut self, cx: &mut Context<Self>) {
        let mut wanted: Vec<(IntegrationProvider, String)> = Vec::new();
        for event in self.model.day_events(self.model.selected()) {
            if let Some(url) = &event.image_url
                && !wanted.iter().any(|(_, known)| known == url)
            {
                wanted.push((event.source, url.clone()));
            }
        }
        let keys: HashSet<&str> = wanted.iter().map(|(_, url)| url.as_str()).collect();
        let loader = self.loader.clone();
        self.art.retain(|url, art| {
            let keep = keys.contains(url.as_str());
            if !keep
                && let Artwork::Ready(image) = art
                && !loader.is_cached(image)
            {
                image.release(cx);
            }
            keep
        });
        self.art_tasks.retain(|url, task| {
            let keep = keys.contains(url.as_str());
            if !keep {
                task.abort();
            }
            keep
        });
        let Some(service) = self.service.clone() else {
            if let Some(cover) = self.fixture_cover.clone() {
                for (_, url) in wanted {
                    self.art
                        .entry(url)
                        .or_insert_with(|| Artwork::Ready(cover.clone()));
                }
            }
            return;
        };
        for (provider, url) in wanted {
            if self.art.contains_key(&url) {
                continue;
            }
            match self.loader.load_release(&service, provider, &url) {
                ArtworkLoad::Cached(image) => {
                    self.art.insert(url, Artwork::Ready(image));
                }
                ArtworkLoad::Pending { task, result } => {
                    let key = url.clone();
                    self.art.insert(url.clone(), Artwork::Loading);
                    self.art_tasks.insert(url.clone(), task);
                    cx.spawn(async move |this, cx| {
                        let Ok(outcome) = result.await else {
                            return;
                        };
                        this.update(cx, |this, cx| {
                            this.art_tasks.remove(&key);
                            // The release left the panel while its cover loaded.
                            if this.art.get(&key) != Some(&Artwork::Loading) {
                                return;
                            }
                            if let Artwork::Ready(image) = &outcome {
                                this.loader.remember(&release_key(&key), image, cx);
                            }
                            this.art.insert(key, outcome);
                            cx.notify();
                        })
                        .ok();
                    })
                    .detach();
                }
            }
        }
    }
}

impl Drop for CalendarScreen {
    fn drop(&mut self) {
        // Abandon everything in flight. The receivers close with the tasks.
        for load in &self.loads {
            load.task.abort();
        }
        for task in self.art_tasks.values() {
            task.abort();
        }
    }
}

/// One day cell, as the grid paints it.
#[derive(Clone, Debug)]
struct DayCell {
    day: NaiveDate,
    in_month: bool,
    today: bool,
    selected: bool,
    chips: Vec<Chip>,
    more: usize,
}

/// A release on a day cell. Several episodes of one series read as one chip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Chip {
    title: String,
    movie: bool,
    count: usize,
}

/// A cell's chips for `events`, with room for `lines` lines below the day
/// number. Everything fits, or one line is kept for "+N more".
pub(super) fn chips_for(events: &[&CalendarEvent], lines: usize) -> (Vec<Chip>, usize) {
    let mut chips: Vec<Chip> = Vec::new();
    let mut series_index: HashMap<i64, usize> = HashMap::new();
    for event in events {
        match event.series_id.filter(|_| !event.is_movie()) {
            Some(series) => match series_index.get(&series) {
                Some(&index) => chips[index].count += 1,
                None => {
                    series_index.insert(series, chips.len());
                    chips.push(Chip {
                        title: event.title.clone(),
                        movie: false,
                        count: 1,
                    });
                }
            },
            None => chips.push(Chip {
                title: event.title.clone(),
                movie: event.is_movie(),
                count: 1,
            }),
        }
    }
    if chips.len() <= lines {
        return (chips, 0);
    }
    let shown = lines.saturating_sub(1);
    let more = chips.len() - shown;
    chips.truncate(shown);
    (chips, more)
}

fn kind_label(event: &CalendarEvent) -> &'static str {
    match event.kind {
        ReleaseKind::Theatrical => "Theatrical",
        ReleaseKind::Digital => "Digital",
        ReleaseKind::Physical => "Physical",
        ReleaseKind::Episode => "Episode",
    }
}

fn source_name(provider: IntegrationProvider) -> &'static str {
    match provider {
        IntegrationProvider::Radarr => "Radarr",
        IntegrationProvider::Sonarr => "Sonarr",
    }
}

/// What a source's line says. Failure copy carries no server text and no key.
pub(super) fn source_line(provider: IntegrationProvider, status: SourceStatus) -> String {
    let name = source_name(provider);
    match status {
        SourceStatus::Checking => format!("Checking {name}…"),
        SourceStatus::Unlinked | SourceStatus::Failed(SourceFailure::Unlinked) => {
            format!("{name} isn't connected")
        }
        SourceStatus::Loading => format!("Loading {name}…"),
        SourceStatus::Ready => format!("{name} connected"),
        SourceStatus::Failed(SourceFailure::Unauthorized) => {
            format!("{name} rejected the saved key")
        }
        SourceStatus::Failed(SourceFailure::Unavailable) => {
            format!("{name} couldn't be reached")
        }
        SourceStatus::Failed(SourceFailure::Malformed) => {
            format!("{name} sent a calendar Matinee could not read")
        }
    }
}

/// "3 scheduled releases", as the shipping calendar words it.
fn release_count_label(count: usize) -> String {
    let noun = if count == 1 { "release" } else { "releases" };
    format!("{count} scheduled {noun}")
}

/// What the panel says for a day with no releases. Nothing when the
/// connection guidance is shown instead. A day is called empty only when
/// every connected source answered its month; a source that failed is named,
/// because its releases may be missing rather than absent.
pub(super) fn empty_day_line<Z: chrono::TimeZone>(model: &CalendarModel<Z>) -> Option<String> {
    if model.overview() == Overview::NothingConnected {
        return None;
    }
    if model.settled() {
        return Some("Nothing is scheduled for this day.".to_string());
    }
    let waiting = PROVIDERS.iter().any(|provider| {
        matches!(
            model.status(*provider),
            SourceStatus::Loading | SourceStatus::Checking
        )
    });
    if waiting {
        return Some("Waiting for every connected source to answer this month.".to_string());
    }
    let failed: Vec<&str> = PROVIDERS
        .iter()
        .filter(|provider| matches!(model.status(**provider), SourceStatus::Failed(_)))
        .map(|provider| source_name(*provider))
        .collect();
    Some(format!(
        "Releases from {} could not be loaded for this month.",
        failed.join(" and ")
    ))
}

fn clock_label(at: chrono::DateTime<Utc>) -> String {
    at.with_timezone(&Local).format("%-I:%M %p").to_string()
}

/// The date the person reads: "Tuesday, August 18".
fn day_title(day: NaiveDate) -> String {
    day.format("%A, %B %-d").to_string()
}

/// A short date for milestones and rows: "Aug 18".
fn short_day(day: NaiveDate) -> String {
    day.format("%b %-d").to_string()
}

/// The heights a day cell is laid out from: the theme's type, so a change of
/// scale keeps lines whole.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CellMetrics {
    /// Padding above and below, plus the day number's line.
    chrome: f32,
    /// One chip or "+N more" line, with the gap above it.
    line: f32,
}

impl CellMetrics {
    pub(super) fn of(theme: &Theme) -> Self {
        let number = theme.typography.style(TextRole::Metadata).line_height;
        let caption = theme.typography.style(TextRole::Caption).line_height;
        Self {
            chrome: 2.0 * CELL_PADDING + number,
            // The chip's colour bar is 12 points; the text line is taller.
            line: CELL_PADDING + caption.max(12.0),
        }
    }

    /// The shortest row: the day number and [`MIN_CELL_LINES`] lines.
    pub(super) fn min_row(self) -> f32 {
        self.chrome + MIN_CELL_LINES as f32 * self.line
    }

    /// Row height for a grid area `area` points tall: the six weeks share it
    /// whole, so a tall window has no empty band under the grid, and the
    /// grid scrolls only when even the shortest rows do not fit.
    pub(super) fn row_height(self, area: Option<f32>) -> f32 {
        let share = area.map_or(0.0, |area| {
            ((area - GRID_ROWS as f32 * CELL_GAP) / GRID_ROWS as f32).floor()
        });
        share.max(self.min_row())
    }

    /// Lines a row of `height` has room for below the day number.
    pub(super) fn lines(self, height: f32) -> usize {
        ((height - self.chrome) / self.line).floor().max(1.0) as usize
    }
}

pub(super) fn grid_sizing(row_height: f32) -> GridSizing {
    GridSizing {
        min_cell_width: Space::S10.value(),
        max_cell_width: 400.0,
        aspect: 0.0,
        extra_height: row_height,
        column_gap: CELL_GAP,
        row_gap: CELL_GAP,
        inset_x: 0.0,
        inset_top: 0.0,
        inset_bottom: 0.0,
        max_columns: 7,
        overscan_rows: 1,
    }
}

impl Render for CalendarScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.settle_focus) {
            let grid = self.grid.focus_handle().clone();
            window.defer(cx, move |window, _| window.focus(&grid));
        }
        let theme = cx.theme().clone();
        let bar = AppBar {
            active: RootDestination::Calendar,
            name: self.name.clone(),
            refresh_disabled: self.model.busy(),
            signing_out: self.signing_out,
        };
        let selected_events: Vec<CalendarEvent> = self
            .model
            .day_events(self.model.selected())
            .into_iter()
            .cloned()
            .collect();
        while self.row_focus.len() < selected_events.len() {
            // A tab stop, as a `Pressable`'s own handle is.
            self.row_focus
                .push(cx.focus_handle().tab_index(0).tab_stop(true));
        }
        let (panel, first_row) = self.panel(&theme, &selected_events, cx);
        // Rows take focus from the keyboard only (a press does not focus
        // them), so a newly focused row is one Tab reached: bring it into view.
        let focused_row = self.row_focus[..selected_events.len()]
            .iter()
            .position(|handle| handle.is_focused(window));
        if focused_row != self.revealed_row {
            if let Some(row) = focused_row {
                self.panel_scroll.reveal_child(first_row + row);
            }
            self.revealed_row = focused_row;
        }

        // The bar, header, and status line keep their height; the month and
        // the panel share what is left and scroll inside it. Without
        // `flex_none`, a short window squeezed the header and the status line
        // under the grid.
        v_stack(Space::S3)
            .size_full()
            .overflow_hidden()
            .p(Space::S4.px())
            .bg(theme.colors.surface.canvas)
            .text_color(theme.colors.text.primary)
            .track_focus(&self.focus)
            .child(
                div()
                    .flex_none()
                    .child(app_bar(&theme, bar, cx, Self::bar_action)),
            )
            .child(
                div()
                    .flex_none()
                    .debug_selector(|| "calendar-header".into())
                    .child(self.header(&theme, cx)),
            )
            .child(
                div()
                    .flex_none()
                    .debug_selector(|| "calendar-status".into())
                    .child(self.status_strip(&theme, cx)),
            )
            .child(
                h_stack(Space::S4)
                    .flex_1()
                    .min_h(px(0.0))
                    .debug_selector(|| "calendar-body".into())
                    .child(self.month(&theme, cx))
                    .child(panel),
            )
    }
}

impl CalendarScreen {
    fn header(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        h_stack(Space::S4)
            .w_full()
            .items_end()
            .justify_between()
            .child(
                v_stack(Space::S1)
                    .child(
                        Text::new("Upcoming releases")
                            .role(TextRole::Metadata)
                            .color(theme.colors.control.accent),
                    )
                    .child(
                        Text::new(self.model.month().format("%B %Y").to_string())
                            .role(TextRole::Heading),
                    )
                    .child(
                        Text::new(release_count_label(self.model.month_release_count()))
                            .role(TextRole::Caption)
                            .tone(TextTone::Secondary),
                    )
                    .child(
                        Text::new(
                            "Monitored titles from your connected Radarr and Sonarr libraries.",
                        )
                        .role(TextRole::Caption)
                        .tone(TextTone::Secondary),
                    ),
            )
            .child(
                h_stack(Space::S2)
                    .items_center()
                    .child(
                        IconButton::new(
                            "calendar-previous",
                            IconName::ChevronLeft,
                            "Previous month",
                        )
                        .variant(ButtonVariant::Subtle)
                        .on_click(cx.listener(|this, _, _, cx| this.step(Step::Previous, cx))),
                    )
                    .child(
                        Button::new("calendar-today", "Today")
                            .variant(ButtonVariant::Secondary)
                            .size(ButtonSize::Small)
                            .on_click(cx.listener(|this, _, _, cx| this.step(Step::Today, cx))),
                    )
                    .child(
                        IconButton::new("calendar-next", IconName::ChevronRight, "Next month")
                            .variant(ButtonVariant::Subtle)
                            .on_click(cx.listener(|this, _, _, cx| this.step(Step::Next, cx))),
                    ),
            )
    }

    fn status_strip(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let failed = PROVIDERS
            .iter()
            .any(|provider| matches!(self.model.status(*provider), SourceStatus::Failed(_)));
        let weak = cx.entity().downgrade();
        let selected = MediaFilter::ALL
            .iter()
            .position(|filter| *filter == self.model.filter())
            .unwrap_or(0);
        let segments = MediaFilter::ALL
            .iter()
            .map(|filter| Segment::new(filter.label()))
            .collect();
        let updated = match self.model.last_updated() {
            Some(at) => format!("Updated {}", clock_label(at)),
            None => "Waiting for the first update".to_string(),
        };
        let mut line = h_stack(Space::S4).w_full().items_center().flex_wrap();
        for provider in PROVIDERS {
            let status = self.model.status(provider);
            let failure = matches!(status, SourceStatus::Failed(_));
            line = line.child(
                Text::new(source_line(provider, status))
                    .role(TextRole::Caption)
                    .tone(if failure {
                        TextTone::Primary
                    } else {
                        TextTone::Secondary
                    })
                    .color(if failure {
                        theme.colors.text.danger
                    } else {
                        theme.colors.text.secondary
                    }),
            );
        }
        if failed {
            line = line.child(
                Button::new("calendar-try-again", "Try again")
                    .variant(ButtonVariant::Subtle)
                    .size(ButtonSize::Small)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            );
        }
        line.child(div().flex_1())
            .child(
                Text::new(updated)
                    .role(TextRole::Caption)
                    .tone(TextTone::Muted),
            )
            .child(
                SegmentedControl::new("calendar-filter", segments, selected).on_change(
                    move |index, _, cx| {
                        if let Some(filter) = MediaFilter::ALL.get(index).copied() {
                            weak.update(cx, |this, cx| this.set_filter(filter, cx)).ok();
                        }
                    },
                ),
            )
    }

    /// The month: a weekday row and the six weeks, in a virtualized grid.
    fn month(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        // The grid reports the area it was given; the first frame uses the
        // shortest rows and the grid draws again once it is measured.
        let metrics = CellMetrics::of(theme);
        let row_height = metrics.row_height(self.grid.measured().map(|(_, height)| height));
        let lines = metrics.lines(row_height);
        let window: MonthWindow = self.model.window();
        let month = self.model.month();
        let today = self.model.today();
        let selected = self.model.selected();
        let mut by_day: BTreeMap<NaiveDate, Vec<&CalendarEvent>> = BTreeMap::new();
        for event in self.model.grid_events() {
            by_day.entry(event.day).or_default().push(event);
        }
        let cells: Vec<DayCell> = (0..GRID_DAYS)
            .map(|index| {
                let day = window.day(index);
                let (chips, more) =
                    chips_for(by_day.get(&day).map(Vec::as_slice).unwrap_or(&[]), lines);
                DayCell {
                    day,
                    in_month: month_start(day) == month,
                    today: day == today,
                    selected: day == selected,
                    chips,
                    more,
                }
            })
            .collect();
        let cells = Rc::new(cells);
        let grid_theme = theme.clone();
        let weak = cx.entity().downgrade();
        let grid = VirtualGrid::new(
            "calendar-grid",
            &self.grid,
            cells.len(),
            move |cell, _, _| day_cell(&grid_theme, &cells[cell.index], cell.focused),
        )
        .sizing(grid_sizing(row_height))
        .on_activate({
            let weak = weak.clone();
            move |index, _, cx| {
                weak.update(cx, |this, cx| this.select_index(index, cx))
                    .ok();
            }
        })
        .on_edge(move |step, _, cx| {
            weak.update(cx, |this, cx| this.edge(step, cx)).ok();
        });

        let mut weekdays = h_stack(Space::S1).w_full();
        for day in WEEKDAYS {
            weekdays = weekdays.child(
                div()
                    .flex_1()
                    .child(Text::new(day).role(TextRole::Caption).tone(TextTone::Muted)),
            );
        }
        // `h_full`: a row's stretched child has no definite height while its
        // own children are sized, so without it the grid area took its whole
        // content height and overflowed the body on short windows.
        v_stack(Space::S2)
            .flex_1()
            .h_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .child(weekdays)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .debug_selector(|| "calendar-grid".into())
                    .child(grid),
            )
            .into_any_element()
    }

    /// The selected day's releases, and the one the person chose. Each part
    /// is a direct, full-width block child of the panel's scroll view, so a
    /// row can be scrolled into view by its position; the second value is the
    /// first row's. (As children of a flex scroll view, rows shrank to their
    /// content when the panel did not overflow.)
    fn panel(
        &self,
        theme: &Theme,
        events: &[CalendarEvent],
        cx: &mut Context<Self>,
    ) -> (AnyElement, usize) {
        let day = self.model.selected();
        let focused = self.model.focused_event().map(|event| event.id.clone());
        let mut parts: Vec<AnyElement> = vec![
            Text::new(day_title(day))
                .role(TextRole::Subheading)
                .into_any_element(),
        ];
        match self.model.overview() {
            Overview::Checking => {
                parts.push(note(theme, "Checking Radarr and Sonarr…").into_any_element());
            }
            Overview::NothingConnected => parts.push(Self::disconnected(cx)),
            Overview::Connected => {}
        }
        let first_row = parts.len();
        if events.is_empty() {
            if let Some(line) = empty_day_line(&self.model) {
                parts.push(note(theme, &line).into_any_element());
            }
        } else {
            for (index, event) in events.iter().enumerate() {
                parts.push(self.event_row(
                    theme,
                    index,
                    event,
                    focused.as_deref() == Some(event.id.as_str()),
                    cx,
                ));
            }
            if let Some(event) = events
                .iter()
                .find(|event| focused.as_deref() == Some(event.id.as_str()))
                .or_else(|| events.first())
            {
                parts.push(Self::detail(event).into_any_element());
            }
        }

        let panel = div()
            .debug_selector(|| "calendar-panel".into())
            .w(px(PANEL_WIDTH))
            .flex_none()
            .h_full()
            .min_h(px(0.0))
            .child(
                ScrollView::vertical("calendar-panel-scroll")
                    .control(self.panel_scroll.clone())
                    .size_full()
                    .children(
                        parts
                            .into_iter()
                            .map(|part| div().w_full().pb(Space::S3.px()).child(part)),
                    ),
            )
            .into_any_element();
        (panel, first_row)
    }

    fn disconnected(cx: &mut Context<Self>) -> AnyElement {
        Surface::new(SurfaceLevel::Elevated)
            .radius(Radius::Medium)
            .padding(Space::S4)
            .bordered(true)
            .child(
                v_stack(Space::S2)
                    .child(
                        Text::new("Connect Radarr or Sonarr")
                            .role(TextRole::Label),
                    )
                    .child(
                        Text::new(
                            "Upcoming movies and episodes appear here once Radarr or Sonarr is connected: use Test & save in the Settings of the current Matinee app. Native Settings is not available yet, and the connections it saves are read here.",
                        )
                        .role(TextRole::Caption)
                        .tone(TextTone::Secondary),
                    )
                    .child(
                        Button::new("calendar-check-again", "Check again")
                            .variant(ButtonVariant::Secondary)
                            .size(ButtonSize::Small)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            )
            .into_any_element()
    }

    fn event_row(
        &self,
        theme: &Theme,
        index: usize,
        event: &CalendarEvent,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = event.id.clone();
        let art = event
            .image_url
            .as_ref()
            .and_then(|url| self.art.get(url))
            .cloned()
            .unwrap_or_default();
        let source = source_name(event.source);
        let when = match event.instant {
            Some(at) => format!("{} · {}", source, clock_label(at)),
            None => format!("{} · {}", source, kind_label(event)),
        };
        let subtitle = event
            .subtitle
            .clone()
            .unwrap_or_else(|| kind_label(event).to_string());
        let weak = cx.entity().downgrade();
        let row = Pressable::new(("calendar-release", index), event.title.clone());
        let row = match self.row_focus.get(index) {
            Some(handle) => row.focus_handle(handle.clone()),
            None => row,
        };
        row.w_full()
            .radius(Radius::Medium)
            .on_press(move |_, _, cx| {
                weak.update(cx, |this, cx| this.focus_event(&id, cx)).ok();
            })
            .child(
                h_stack(Space::S3)
                    .debug_selector(move || format!("calendar-row-{index}"))
                    .w_full()
                    .items_start()
                    .p(Space::S2.px())
                    .when(focused, |row| row.bg(theme.colors.surface.elevated))
                    .child(art_frame(
                        theme,
                        ("calendar-cover", index),
                        &art,
                        COVER_WIDTH,
                        COVER_HEIGHT,
                        Radius::Small,
                        &event.title,
                    ))
                    .child(
                        v_stack(Space::S1)
                            .flex_1()
                            .min_w(px(0.0))
                            .child(Text::new(event.title.clone()).role(TextRole::Label))
                            .child(
                                Text::new(subtitle)
                                    .role(TextRole::Caption)
                                    .tone(TextTone::Secondary),
                            )
                            .child(
                                Text::new(when)
                                    .role(TextRole::Metadata)
                                    .color(kind_color(event)),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// The chosen release in full: what it is, when, what the source knows,
    /// and where a movie's other release days fall.
    fn detail(event: &CalendarEvent) -> impl IntoElement {
        let mut column = v_stack(Space::S2)
            .w_full()
            .child(
                Text::new(if event.is_movie() {
                    "Radarr movie"
                } else {
                    "Sonarr episode"
                })
                .role(TextRole::Metadata)
                .color(kind_color(event)),
            )
            .child(Text::new(event.title.clone()).role(TextRole::Subheading));
        if let Some(subtitle) = &event.subtitle {
            column = column.child(
                Text::new(subtitle.clone())
                    .role(TextRole::Label)
                    .tone(TextTone::Secondary),
            );
        }
        column = column.child(
            Text::new(format!("{} · {}", kind_label(event), short_day(event.day)))
                .role(TextRole::Caption)
                .tone(TextTone::Secondary),
        );
        if event.milestones.len() > 1 {
            let mut milestones = v_stack(Space::S1).w_full().child(
                Text::new("Release dates")
                    .role(TextRole::Metadata)
                    .tone(TextTone::Muted),
            );
            for milestone in &event.milestones {
                let current = milestone.kind == event.kind;
                milestones = milestones.child(
                    Text::new(format!(
                        "{} · {}",
                        kind_label_of(milestone.kind),
                        short_day(milestone.day)
                    ))
                    .role(TextRole::Caption)
                    .tone(if current {
                        TextTone::Primary
                    } else {
                        TextTone::Secondary
                    }),
                );
            }
            column = column.child(milestones);
        }
        column =
            column.child(
                Text::new(event.overview.clone().unwrap_or_else(|| {
                    "No overview is available from the connected service.".into()
                }))
                .role(TextRole::Body)
                .tone(TextTone::Secondary),
            );
        if !event.genres.is_empty() {
            column = column.child(
                Text::new(event.genres.join(" · "))
                    .role(TextRole::Caption)
                    .tone(TextTone::Muted),
            );
        }
        column.child(
            Text::new(if event.downloaded {
                "In your library"
            } else {
                "Not in your library yet"
            })
            .role(TextRole::Caption)
            .tone(TextTone::Muted),
        )
    }
}

/// A plain note in the panel.
fn note(theme: &Theme, text: &str) -> impl IntoElement {
    Text::new(text.to_string())
        .role(TextRole::Caption)
        .tone(TextTone::Secondary)
        .color(theme.colors.text.secondary)
}

fn kind_color(event: &CalendarEvent) -> Color {
    if event.is_movie() {
        matinee_ui::palette::MARQUEE_AMBER
    } else {
        matinee_ui::palette::FADED_TEAL
    }
}

fn kind_label_of(kind: ReleaseKind) -> &'static str {
    match kind {
        ReleaseKind::Theatrical => "Theatrical",
        ReleaseKind::Digital => "Digital",
        ReleaseKind::Physical => "Physical",
        ReleaseKind::Episode => "Episode",
    }
}

/// A day number's colour: the accent for today, muted for a day the grid
/// borrows from a neighbouring month, primary otherwise.
pub(super) fn day_number_color(theme: &Theme, in_month: bool, today: bool) -> Color {
    if today {
        theme.colors.control.accent
    } else if in_month {
        theme.colors.text.primary
    } else {
        theme.colors.text.muted
    }
}

/// One day cell: the number, up to three release chips, and a count of the rest.
fn day_cell(theme: &Theme, cell: &DayCell, focused: bool) -> AnyElement {
    let mut column = v_stack(Space::S1).size_full().p(Space::S1.px()).child(
        Text::new(cell.day.format("%-d").to_string())
            .role(TextRole::Metadata)
            .color(day_number_color(theme, cell.in_month, cell.today)),
    );
    for chip in &cell.chips {
        column = column.child(
            h_stack(Space::S1)
                .w_full()
                .items_center()
                .overflow_hidden()
                .child(div().w(px(3.0)).h(px(12.0)).flex_none().bg(if chip.movie {
                    matinee_ui::palette::MARQUEE_AMBER
                } else {
                    matinee_ui::palette::FADED_TEAL
                }))
                // One line per chip: a wrapped title ran into the chip below
                // it in a narrow cell. The episode count stays visible.
                .child(
                    div().min_w_0().flex_1().overflow_hidden().child(
                        Text::new(chip.title.clone())
                            .role(TextRole::Caption)
                            .tone(TextTone::Primary)
                            .truncate(),
                    ),
                )
                .when(chip.count > 1, |row| {
                    row.child(
                        div().flex_none().child(
                            Text::new(format!("×{}", chip.count))
                                .role(TextRole::Caption)
                                .tone(TextTone::Secondary),
                        ),
                    )
                }),
        );
    }
    if cell.more > 0 {
        column = column.child(
            Text::new(format!("+{} more", cell.more))
                .role(TextRole::Caption)
                .tone(TextTone::Muted),
        );
    }
    div()
        .size_full()
        .rounded(px(theme.radius.get(Radius::Small)))
        .bg(if cell.selected {
            theme.colors.surface.elevated
        } else if cell.in_month {
            theme.colors.surface.panel
        } else {
            theme.colors.surface.canvas
        })
        .border(px(1.0))
        .border_color(if focused {
            theme.colors.focus.ring
        } else if cell.selected {
            theme.colors.control.accent
        } else {
            theme.colors.border.subtle
        })
        .overflow_hidden()
        .child(column)
        .into_any_element()
}
