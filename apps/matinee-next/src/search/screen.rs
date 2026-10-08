//! Search screen.
//!
//! Paints [`SearchModel`] in Atelier's [`VirtualGrid`] under a search field
//! that holds focus when the screen opens. Typing waits out the debounce
//! before a search is sent; Enter sends at once. The grid reads its own
//! window ([`VirtualGridState::frame`]) to decide which posters to load and
//! when to ask for the next page, exactly as Library does.
//!
//! Search is a root destination. It stays alive while Details or the Player
//! covers it, so the text, the results, the scroll offset, and the focused
//! title are where the person left them. A title emits [`SearchEvent::Open`];
//! Search never starts playback.
//!
//! Titles held from the previous query while a new one loads are dimmed and
//! inert: they do not open, take hover, or move the logical focus.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use atelier_ui::gpui::{EventEmitter, KeyDownEvent, Task};
use atelier_ui::prelude::*;
use matinee_core::{ItemId, SearchQuery};
use matinee_jellyfin::{ArtworkRequest, Session};
use tokio::task::JoinHandle;

use super::load;
use super::model::{
    Applied, DEBOUNCE_MILLIS, Debounce, Footer, IdleReason, Request, Response, SearchFailure,
    SearchModel, SearchState,
};
use crate::app_bar::{AppBar, BarAction, app_bar};
use crate::artwork::{Artwork, ArtworkLoad, ArtworkLoader, Client};
use crate::library::model::count_label;
use crate::media_grid::{
    CardData, Layout, SEARCH_IDS, artwork_window, card, poster_request, quiet, skeleton,
};
use crate::nav::RootDestination;
use crate::runtime::ServiceRuntime;

/// What the shell does for Search.
pub(crate) enum SearchEvent {
    /// Open Details for this title.
    Open(ItemId),
    /// Go to another root destination from the app bar, or Home on Escape.
    Navigate(RootDestination),
    SignOut,
    /// Jellyfin no longer accepts the session.
    SessionExpired,
}

/// Height of the line under the last row: loading, or Try again.
const FOOTER_HEIGHT: f32 = Space::S16.value();
/// The field stops growing here, however wide the window is.
const FIELD_MAX_WIDTH: f32 = 560.0;
/// Old titles, held while a new query loads, are drawn this faint.
const STALE_OPACITY: f32 = 0.5;

/// Review-scene artwork for an address.
pub(super) type FixtureArt = Box<dyn Fn(&str) -> Artwork>;
/// Review-scene answers for a request. `None` leaves it unanswered.
pub(super) type FixtureAnswers = Rc<dyn Fn(&Request) -> Option<Response>>;

/// Which control takes focus when the screen next shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Settle {
    /// The field: a fresh open, or a root chosen from the bar.
    Field,
    /// The results, where the person left the grid: back from Details.
    Results,
}

pub(crate) struct SearchScreen {
    runtime: Arc<ServiceRuntime>,
    /// `None` for review scenes, which never open a socket.
    client: Option<Client>,
    /// Builds artwork addresses. No token is ever put on them.
    session: Session,
    loader: ArtworkLoader,
    pub(crate) model: SearchModel,
    /// The request for the page in flight. A new one replaces it.
    page_task: Option<JoinHandle<()>>,
    tasks: Vec<JoinHandle<()>>,
    /// The debounce wait. Replacing or dropping it cancels the wait.
    debounce_task: Option<Task<()>>,
    pub(super) art: HashMap<String, Artwork>,
    art_tasks: HashMap<String, JoinHandle<()>>,
    /// The addresses wanted at the last sync, in grid order.
    wanted: Vec<String>,
    grid: VirtualGridState,
    focus: FocusHandle,
    field: FocusHandle,
    settle: Option<Settle>,
    signing_out: bool,
    /// Review scenes: artwork from fixtures instead of the network.
    pub(super) fixture_art: Option<FixtureArt>,
    /// Review scenes: answers from fixtures instead of the network.
    pub(super) fixture_answers: Option<FixtureAnswers>,
}

impl EventEmitter<SearchEvent> for SearchScreen {}

impl SearchScreen {
    pub(crate) fn open(
        runtime: Arc<ServiceRuntime>,
        client: Client,
        loader: ArtworkLoader,
        cx: &mut Context<Self>,
    ) -> Self {
        let session = client.session().clone();
        Self::with_model(
            runtime,
            Some(client),
            session,
            loader,
            SearchModel::new(),
            cx,
        )
    }

    pub(super) fn with_model(
        runtime: Arc<ServiceRuntime>,
        client: Option<Client>,
        session: Session,
        loader: ArtworkLoader,
        model: SearchModel,
        cx: &mut Context<Self>,
    ) -> Self {
        // Posters this screen decoded and the cache did not keep leave the
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
            client,
            session,
            loader,
            model,
            page_task: None,
            tasks: Vec::new(),
            debounce_task: None,
            art: HashMap::new(),
            art_tasks: HashMap::new(),
            wanted: Vec::new(),
            grid: VirtualGridState::new(cx),
            focus: cx.focus_handle(),
            field: cx.focus_handle(),
            settle: Some(Settle::Field),
            signing_out: false,
            fixture_art: None,
            fixture_answers: None,
        }
    }

    pub(crate) fn set_signing_out(&mut self, signing_out: bool, cx: &mut Context<Self>) {
        self.signing_out = signing_out;
        cx.notify();
    }

    /// Search was chosen from the bar: the field takes focus. Playback that
    /// happened meanwhile reloads only the title that was opened.
    pub(crate) fn show(&mut self, playback: bool, cx: &mut Context<Self>) {
        self.reconcile_if(playback, cx);
        self.settle = Some(Settle::Field);
        cx.notify();
    }

    /// Search is visible again after Details (and perhaps the Player)
    /// closed. The results take focus, where the person left them.
    pub(crate) fn resume(&mut self, playback: bool, cx: &mut Context<Self>) {
        self.reconcile_if(playback, cx);
        self.settle = Some(Settle::Results);
        cx.notify();
    }

    fn reconcile_if(&mut self, playback: bool, cx: &mut Context<Self>) {
        if playback && let Some(request) = self.model.reconcile() {
            self.run(vec![request], cx);
        }
    }

    fn type_text(&mut self, value: &str, cx: &mut Context<Self>) {
        // Typing again replaces the wait: dropping the old task cancels it.
        self.debounce_task = None;
        if let Some(debounce) = self.model.type_text(value) {
            self.debounce_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(DEBOUNCE_MILLIS))
                    .await;
                this.update(cx, |this, cx| this.debounced(debounce, cx))
                    .ok();
            }));
        }
        self.drop_abandoned_page();
        cx.notify();
    }

    /// The model gave up on the page in flight: the field was cleared, or
    /// went back to the query already shown. Stop its HTTP call; its answer
    /// could not apply anyway.
    fn drop_abandoned_page(&mut self) {
        if !self.model.is_loading()
            && let Some(task) = self.page_task.take()
        {
            task.abort();
        }
    }

    fn debounced(&mut self, debounce: Debounce, cx: &mut Context<Self>) {
        let requests = self.model.debounced(debounce);
        self.drop_abandoned_page();
        self.run(requests, cx);
        cx.notify();
    }

    /// Enter: search the field now.
    fn submit(&mut self, cx: &mut Context<Self>) {
        self.debounce_task = None;
        let requests = self.model.submit();
        self.drop_abandoned_page();
        self.run(requests, cx);
        cx.notify();
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let requests = self.model.retry();
        self.run(requests, cx);
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let requests = self.model.refresh();
        self.run(requests, cx);
        cx.notify();
    }

    fn open_item(&mut self, index: usize, cx: &mut Context<Self>) {
        // Dimmed titles belong to the previous query. They do not open.
        if self.model.is_stale() {
            return;
        }
        let Some(id) = self.model.items().get(index).map(|item| item.id().clone()) else {
            return;
        };
        self.model.note_opened(id.clone());
        cx.emit(SearchEvent::Open(id));
    }

    /// The grid's logical focus moved. Looked up when it happens, so a
    /// render does not copy every loaded id.
    fn note_focused(&mut self, index: usize) {
        if self.model.is_stale() {
            return;
        }
        if let Some(id) = self.model.items().get(index).map(|item| item.id().clone()) {
            self.model.note_focused(id);
        }
    }

    fn bar_action(&mut self, action: BarAction, _: &mut Window, cx: &mut Context<Self>) {
        match action {
            BarAction::Go(RootDestination::Search) => {}
            BarAction::Go(destination) => cx.emit(SearchEvent::Navigate(destination)),
            BarAction::Refresh => self.refresh(cx),
            BarAction::SignOut => cx.emit(SearchEvent::SignOut),
        }
    }

    /// Keys the field cannot take itself: Enter searches now, and Down moves
    /// into the results (the title focused there before, or the first).
    /// The field binds neither, so its editing keys are untouched. Up from
    /// the grid's first row comes back through the grid's edge
    /// (`VirtualGrid::on_edge`); the grid's own keys stay its own.
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.field.is_focused(window) {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter" => {
                self.submit(cx);
                cx.stop_propagation();
            }
            "down" if self.model.state() == SearchState::Ready => {
                let index = self.grid.focused().unwrap_or(0);
                note_keyboard_navigation(cx);
                self.grid.focus_index(Some(index));
                window.focus(self.grid.focus_handle());
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    /// Start each request. A page request replaces the one in flight, which
    /// is aborted: its answer could not apply anyway.
    fn run(&mut self, requests: Vec<Request>, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            if let Some(answers) = self.fixture_answers.clone() {
                for request in requests {
                    let Some(response) = answers(&request) else {
                        continue;
                    };
                    cx.spawn(async move |this, cx| {
                        this.update(cx, |this, cx| this.answer(&request, response, cx))
                            .ok();
                    })
                    .detach();
                }
            }
            return;
        };
        self.tasks.retain(|task| !task.is_finished());
        for request in requests {
            let client = Arc::clone(&client);
            let sent = request.clone();
            let (task, rx) = self
                .runtime
                .spawn(async move { load::run(client.as_ref(), sent).await });
            match &request {
                Request::Page { .. } => {
                    if let Some(previous) = self.page_task.replace(task) {
                        previous.abort();
                    }
                }
                Request::Item { .. } => self.tasks.push(task),
            }
            cx.spawn(async move |this, cx| {
                let Ok(response) = rx.await else {
                    return;
                };
                this.update(cx, |this, cx| this.answer(&request, response, cx))
                    .ok();
            })
            .detach();
        }
    }

    pub(super) fn answer(&mut self, request: &Request, response: Response, cx: &mut Context<Self>) {
        match self.model.apply(request, response) {
            Applied::Ignored => return,
            Applied::SessionExpired => {
                cx.emit(SearchEvent::SessionExpired);
                return;
            }
            // A new query starts at the top of its results. A refresh of the
            // same query keeps the focused title if it is still there.
            Applied::Replaced => match self.model.focused_index() {
                Some(index) => self.grid.focus_index(Some(index)),
                None => self.grid.reset(),
            },
            Applied::Appended | Applied::Updated => {}
        }
        cx.notify();
    }

    /// Keep artwork for exactly the cards the grid builds, and ask for the
    /// next page when they near the end of what is loaded. Runs from render,
    /// but does work only when the window actually moved.
    fn follow_window(&mut self, built: Range<usize>, cx: &mut Context<Self>) {
        let urls = self.session.artwork();
        let wanted = artwork_window(self.model.items(), built.clone(), &urls);
        let keys: Vec<String> = wanted.iter().map(|request| request.url.clone()).collect();
        if keys != self.wanted {
            self.wanted = keys;
            self.sync_artwork(wanted, cx);
        }
        if !built.is_empty()
            && let Some(request) = self.model.want_more(built.end - 1)
        {
            self.run(vec![request], cx);
        }
    }

    /// Start what is wanted and missing; cancel and forget what is not.
    /// Forgotten posters stay in the shared cache.
    fn sync_artwork(&mut self, wanted: Vec<ArtworkRequest>, cx: &mut Context<Self>) {
        let keys: HashSet<&str> = wanted.iter().map(|request| request.url.as_str()).collect();
        let loader = self.loader.clone();
        self.art.retain(|key, art| {
            let keep = keys.contains(key.as_str());
            if !keep
                && let Artwork::Ready(image) = art
                && !loader.is_cached(image)
            {
                image.release(cx);
            }
            keep
        });
        self.art_tasks.retain(|key, task| {
            let keep = keys.contains(key.as_str());
            if !keep {
                task.abort();
            }
            keep
        });
        let Some(client) = self.client.clone() else {
            if let Some(fixture) = &self.fixture_art {
                for request in wanted {
                    let art = fixture(&request.url);
                    self.art.entry(request.url).or_insert(art);
                }
            }
            return;
        };
        for request in wanted {
            if self.art.contains_key(&request.url) {
                continue;
            }
            let key = request.url.clone();
            match self.loader.load(&client, request) {
                ArtworkLoad::Cached(image) => {
                    self.art.insert(key, Artwork::Ready(image));
                }
                ArtworkLoad::Pending { task, result } => {
                    self.art.insert(key.clone(), Artwork::Loading);
                    self.art_tasks.insert(key.clone(), task);
                    cx.spawn(async move |this, cx| {
                        let Ok(outcome) = result.await else {
                            return;
                        };
                        this.update(cx, |this, cx| {
                            this.art_tasks.remove(&key);
                            // The card left the window while it loaded.
                            if this.art.get(&key) != Some(&Artwork::Loading) {
                                return;
                            }
                            if let Artwork::Ready(image) = &outcome {
                                this.loader.remember(&key, image, cx);
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

    /// Review scenes: keyboard focus on the results, at `index`.
    pub(super) fn focus_results(&mut self, index: Option<usize>) {
        self.grid.focus_index(index);
        self.settle = Some(Settle::Results);
    }

    /// The grid state, for tests that check it survives pages and roots.
    #[cfg(test)]
    pub(crate) fn grid(&self) -> VirtualGridState {
        self.grid.clone()
    }

    /// The field's focus handle, for tests.
    #[cfg(test)]
    pub(crate) fn field_focus(&self) -> FocusHandle {
        self.field.clone()
    }

    /// Artwork slots held and fetches in flight, for tests.
    #[cfg(test)]
    pub(crate) fn art_counts(&self) -> (usize, usize) {
        (self.art.len(), self.art_tasks.len())
    }

    /// The addresses the screen currently wants, in grid order.
    #[cfg(test)]
    pub(crate) fn wanted(&self) -> &[String] {
        &self.wanted
    }

    /// Record every request this review-scene screen sends from now on.
    #[cfg(test)]
    pub(crate) fn record_requests(&mut self) -> Rc<RefCell<Vec<Request>>> {
        let log = Rc::new(RefCell::new(Vec::new()));
        if let Some(answers) = self.fixture_answers.take() {
            let sink = Rc::clone(&log);
            self.fixture_answers = Some(Rc::new(move |request: &Request| {
                sink.borrow_mut().push(request.clone());
                answers(request)
            }));
        }
        log
    }

    /// Whether a page request task is held (in flight or not yet reaped).
    #[cfg(test)]
    pub(crate) fn holds_page_task(&self) -> bool {
        self.page_task.is_some()
    }

    /// After a show or a return, put focus where it was left: the field for
    /// a fresh open, the results for a return from Details.
    ///
    /// Applied once this frame is drawn (`Window::defer`), so the target is
    /// in the frame's focus tree. Unlike a next-frame callback this does not
    /// wait for the platform to ask for a frame, so it is also what the
    /// headless tests see.
    fn settle_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(settle) = self.settle.take() else {
            return;
        };
        let target = match settle {
            Settle::Results if self.model.state() == SearchState::Ready => {
                self.grid.focus_handle().clone()
            }
            _ => self.field.clone(),
        };
        window.defer(cx, move |window, _| window.focus(&target));
    }
}

impl Drop for SearchScreen {
    fn drop(&mut self) {
        if let Some(task) = self.page_task.take() {
            task.abort();
        }
        for task in self.tasks.drain(..) {
            task.abort();
        }
        for (_, task) in self.art_tasks.drain() {
            task.abort();
        }
    }
}

impl Render for SearchScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.settle_focus(window, cx);
        let theme = cx.theme().clone();
        let width = f32::from(window.viewport_size().width);
        let layout = Layout::for_width(width);
        let state = self.model.state();
        // The previous query's titles, held while this one loads.
        let stale = self.model.is_stale();

        let body = match state {
            SearchState::Ready => self.results(&theme, layout, window, cx),
            SearchState::Loading if stale => self.results(&theme, layout, window, cx),
            SearchState::Loading => skeleton(&theme, SEARCH_IDS, layout, width).into_any_element(),
            SearchState::Idle(reason) => self.idle(&theme, layout, reason).into_any_element(),
            SearchState::NoResults => self.no_results(&theme, layout).into_any_element(),
            SearchState::Failed(failure) => self.failure(layout, failure, cx).into_any_element(),
        };
        if state != SearchState::Ready && !stale {
            // No grid this frame, so no poster is wanted. (The grid already
            // synced its own window above; doing both would drop and restart
            // every stale poster on each render.)
            self.follow_window(0..0, cx);
        }

        div()
            .id("search")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.colors.surface.canvas)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.key_down(event, window, cx);
            }))
            .child(
                v_stack(Space::S0)
                    .size_full()
                    .child(
                        div()
                            .flex_none()
                            .px(layout.gutter.px())
                            .pt(Space::S5.px())
                            .child(app_bar(
                                &theme,
                                AppBar {
                                    active: RootDestination::Search,
                                    name: self.session.user().name().to_string(),
                                    refresh_disabled: self.model.is_loading()
                                        || self.model.effective().is_none(),
                                    signing_out: self.signing_out,
                                },
                                cx,
                                Self::bar_action,
                            )),
                    )
                    .child(self.header(layout, cx))
                    .child(div().flex_1().min_h(px(0.0)).w_full().child(body)),
            )
    }
}

impl SearchScreen {
    fn header(&self, layout: Layout, cx: &mut Context<Self>) -> impl IntoElement {
        let caption = match self.model.state() {
            SearchState::Ready => {
                let total = self.model.total().unwrap_or(self.model.items().len());
                Some(count_label(total))
            }
            SearchState::Loading => Some("Searching…".to_string()),
            _ => None,
        };
        let field = {
            let screen = cx.entity().downgrade();
            let on_change = screen.clone();
            let dismiss = screen;
            SearchField::new("search-field", self.model.input().to_string())
                .placeholder("Search movies, series, and episodes")
                .focus_handle(self.field.clone())
                .on_change(move |value, _, cx| {
                    on_change
                        .update(cx, |this, cx| this.type_text(value.as_ref(), cx))
                        .ok();
                })
                .on_dismiss(move |_, cx| {
                    // Escape on an empty field leaves Search for Home.
                    dismiss
                        .update(cx, |_, cx| {
                            cx.emit(SearchEvent::Navigate(RootDestination::Home))
                        })
                        .ok();
                })
        };
        v_stack(Space::S4)
            .flex_none()
            .w_full()
            .px(layout.gutter.px())
            .pt(Space::S8.px())
            .pb(Space::S5.px())
            .child(Text::new("Search").role(TextRole::Title))
            .child(div().w_full().max_w(px(FIELD_MAX_WIDTH)).child(field))
            .child(
                Text::new(caption.unwrap_or_else(|| "\u{a0}".into()))
                    .role(TextRole::Caption)
                    .tone(TextTone::Muted),
            )
    }

    /// The results grid. While a new query loads, the old titles stay on
    /// screen, faint and inert, so they are never taken for the new query's
    /// or opened by mistake.
    fn results(
        &mut self,
        theme: &Theme,
        layout: Layout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let grid = self.grid.clone();
        let sizing = layout.sizing();
        let count = self.model.items().len();
        let width = f32::from(window.viewport_size().width);
        let height = f32::from(window.viewport_size().height);
        let fallback = (width, (height - CHROME_ESTIMATE).max(0.0));
        let frame = grid.frame(sizing, count, FOOTER_HEIGHT, fallback);
        self.follow_window(frame.materialized.clone(), cx);

        let urls = self.session.artwork();
        let cards: HashMap<usize, CardData> = frame
            .materialized
            .clone()
            .filter_map(|index| {
                let item = self.model.items().get(index)?;
                let art = match poster_request(item, &urls) {
                    Some(request) => self.art.get(&request.url).cloned().unwrap_or_default(),
                    None => Artwork::Missing,
                };
                Some((index, CardData::new(item, art)))
            })
            .collect();
        let cards = Rc::new(RefCell::new(cards));
        let card_theme = theme.clone();
        let (cell_width, art_height) = (frame.layout.cell_width, frame.layout.cell_width * 1.5);
        let footer = self.footer(theme, cx);
        let open = cx.entity().downgrade();
        let focused = cx.entity().downgrade();
        let field = self.field.clone();

        let stale = self.model.is_stale();
        let grid = VirtualGrid::new("search-grid", &grid, count, move |cell, _, _| {
            match cards.borrow_mut().remove(&cell.index) {
                Some(data) => card(
                    &card_theme,
                    SEARCH_IDS,
                    &data,
                    cell_width,
                    art_height,
                    cell.focused,
                ),
                None => div().into_any_element(),
            }
        })
        .sizing(sizing)
        .fallback_viewport(fallback)
        .footer(FOOTER_HEIGHT, footer)
        .on_activate(move |index, _, cx| {
            open.update(cx, |this, cx| this.open_item(index, cx)).ok();
        })
        .on_focus(move |index, _, cx| {
            focused.update(cx, |this, _| this.note_focused(index)).ok();
        })
        .on_edge(move |step, window, _| {
            // Up from the first row goes back to the field above.
            if step == GridStep::Up {
                window.focus(&field);
            }
        });
        div()
            .relative()
            .size_full()
            .child(grid)
            .when(stale, |area| {
                // A layer over the old titles takes the pointer, so they do
                // not hover or open. The wheel still scrolls them.
                area.opacity(STALE_OPACITY).child(
                    div()
                        .id("search-stale")
                        .absolute()
                        .inset_0()
                        .block_mouse_except_scroll(),
                )
            })
            .into_any_element()
    }

    fn footer(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let row = h_stack(Space::S3).size_full().justify_center();
        match self.model.footer() {
            Footer::Loading => row
                .child(
                    Icon::new(IconName::Spinner)
                        .size(IconSize::Small)
                        .color(theme.colors.text.muted),
                )
                .child(quiet(theme, "Loading more results…"))
                .into_any_element(),
            Footer::Failed(failure) => row
                .child(quiet(theme, more_failure_copy(failure)))
                .child(
                    Button::new("search-more-retry", "Try again")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                )
                .into_any_element(),
            Footer::Idle | Footer::End => div().into_any_element(),
        }
    }

    /// Before anything is searched, or while the text is too short.
    fn idle(&self, theme: &Theme, layout: Layout, reason: IdleReason) -> impl IntoElement {
        let (title, hint) = match reason {
            IdleReason::Empty => (
                "Search your library",
                "Movies, series, and episodes appear as you type.",
            ),
            IdleReason::TooShort => ("Keep typing", "Two letters or more to search."),
        };
        v_stack(Space::S3)
            .px(layout.gutter.px())
            .pt(Space::S8.px())
            .child(Text::new(title).role(TextRole::Subheading))
            .child(quiet(theme, hint))
    }

    fn no_results(&self, theme: &Theme, layout: Layout) -> impl IntoElement {
        let term = self
            .model
            .shown()
            .or(self.model.effective())
            .map(SearchQuery::term)
            .unwrap_or_default()
            .to_string();
        v_stack(Space::S3)
            .px(layout.gutter.px())
            .pt(Space::S8.px())
            .child(
                Text::new(format!("No results for \u{201c}{term}\u{201d}"))
                    .role(TextRole::Subheading),
            )
            .child(quiet(theme, "Check the spelling, or try fewer words."))
    }

    fn failure(
        &self,
        layout: Layout,
        failure: SearchFailure,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_stack(Space::S3)
            .px(layout.gutter.px())
            .pt(Space::S8.px())
            .max_w(px(620.0))
            .child(Text::new(failure.title()).role(TextRole::Subheading))
            .child(
                Text::new(failure.message())
                    .role(TextRole::Body)
                    .tone(TextTone::Secondary),
            )
            .child(
                div().pt(Space::S2.px()).child(
                    Button::new("search-retry", "Try again")
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                ),
            )
    }
}

/// What a failed next page says. Copy is fixed; the server's text never shows.
fn more_failure_copy(failure: SearchFailure) -> &'static str {
    match failure {
        SearchFailure::Unreachable => "More results couldn't be loaded. Jellyfin isn't answering.",
        _ => "More results couldn't be loaded.",
    }
}

/// Measured height of the app bar and header, for the grid before it is
/// measured. Library uses the same estimate.
const CHROME_ESTIMATE: f32 = 260.0;
