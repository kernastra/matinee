//! Library screen.
//!
//! Paints [`LibraryModel`] in Atelier's [`VirtualGrid`] and runs its
//! requests on the service runtime. The grid builds only the rows near the
//! viewport; this screen reads the same window ([`VirtualGridState::frame`])
//! to decide which posters to load and when to ask for the next page.
//! Artwork comes from the shared [`ArtworkLoader`]: posters that leave the
//! window lose their slot and their fetch, and stay in the shared cache.
//!
//! Library is a root destination: it stays alive while Details, the Player,
//! or Home is showing, so the query, pages, scroll offset, and focused card
//! are where the person left them. A card emits [`LibraryEvent::Open`];
//! Library never starts playback.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use atelier_ui::gpui::EventEmitter;
use atelier_ui::prelude::*;
use matinee_core::{ItemId, LibraryKind, LibrarySort, WatchFilter};
use matinee_jellyfin::{ArtworkRequest, Session};
use tokio::task::JoinHandle;

use super::load;
use super::model::{
    Applied, CatalogState, EmptyReason, Fetch, Footer, LibraryFailure, LibraryModel, Request,
    Response, count_label,
};
use crate::app_bar::{AppBar, BarAction, app_bar};
use crate::artwork::{Artwork, ArtworkLoad, ArtworkLoader, Client};
use crate::media_grid::{
    CardData, LIBRARY_IDS, Layout, artwork_window, card, poster_request, quiet, skeleton,
};
use crate::nav::RootDestination;
use crate::runtime::ServiceRuntime;

/// What the shell does for Library.
pub(crate) enum LibraryEvent {
    /// Open Details for this title.
    Open(ItemId),
    /// Go to another root destination from the app bar.
    Navigate(RootDestination),
    SignOut,
    /// Jellyfin no longer accepts the session.
    SessionExpired,
}

/// The four quiet menus in the header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuKind {
    View,
    Genre,
    Show,
    Sort,
}

/// Height of the line under the last row: loading, or Try again.
const FOOTER_HEIGHT: f32 = Space::S16.value();
/// The app bar and header, for the grid's size before it is measured.
const CHROME_ESTIMATE: f32 = 180.0;
/// Long menus (genres) scroll past this.
const MENU_HEIGHT: f32 = 360.0;

/// Review-scene artwork for an address.
pub(super) type FixtureArt = Box<dyn Fn(&str) -> Artwork>;
/// Review-scene answers for a request.
pub(super) type FixtureAnswers = Rc<dyn Fn(&Request) -> Option<Response>>;

pub(crate) struct LibraryScreen {
    runtime: Arc<ServiceRuntime>,
    /// `None` for review scenes, which never open a socket.
    client: Option<Client>,
    /// Builds artwork addresses. No token is ever put on them.
    session: Session,
    loader: ArtworkLoader,
    pub(crate) model: LibraryModel,
    /// The page request in flight for each kind. A new one replaces it.
    page_tasks: HashMap<LibraryKind, JoinHandle<()>>,
    tasks: Vec<JoinHandle<()>>,
    pub(super) art: HashMap<String, Artwork>,
    art_tasks: HashMap<String, JoinHandle<()>>,
    /// The addresses wanted at the last sync, in grid order.
    wanted: Vec<String>,
    grids: HashMap<LibraryKind, VirtualGridState>,
    focus: FocusHandle,
    pub(super) menu: Option<MenuKind>,
    /// Put focus on the grid once there is something to focus.
    pub(super) restore_focus: bool,
    signing_out: bool,
    opened_at: Instant,
    reported_first_grid: bool,
    /// Review scenes: artwork from fixtures instead of the network.
    pub(super) fixture_art: Option<FixtureArt>,
    /// Review scenes: answers from fixtures instead of the network. `None`
    /// leaves a request unanswered (the loading scene).
    pub(super) fixture_answers: Option<FixtureAnswers>,
}

impl EventEmitter<LibraryEvent> for LibraryScreen {}

impl LibraryScreen {
    pub(crate) fn open(
        runtime: Arc<ServiceRuntime>,
        client: Client,
        loader: ArtworkLoader,
        kind: LibraryKind,
        cx: &mut Context<Self>,
    ) -> Self {
        let (model, requests) = LibraryModel::open(kind);
        let session = client.session().clone();
        let mut screen = Self::with_model(runtime, Some(client), session, loader, model, cx);
        screen.run(requests, cx);
        screen
    }

    pub(super) fn with_model(
        runtime: Arc<ServiceRuntime>,
        client: Option<Client>,
        session: Session,
        loader: ArtworkLoader,
        model: LibraryModel,
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
        let grids = [LibraryKind::Movies, LibraryKind::Series]
            .into_iter()
            .map(|kind| (kind, VirtualGridState::new(cx)))
            .collect();
        Self {
            runtime,
            client,
            session,
            loader,
            model,
            page_tasks: HashMap::new(),
            tasks: Vec::new(),
            art: HashMap::new(),
            art_tasks: HashMap::new(),
            wanted: Vec::new(),
            grids,
            focus: cx.focus_handle(),
            menu: None,
            restore_focus: true,
            signing_out: false,
            opened_at: Instant::now(),
            reported_first_grid: false,
            fixture_art: None,
            fixture_answers: None,
        }
    }

    pub(crate) fn kind(&self) -> LibraryKind {
        self.model.kind()
    }

    /// The grid state for a kind, for tests that check it survives pages.
    pub(crate) fn grid(&self, kind: LibraryKind) -> VirtualGridState {
        self.grids[&kind].clone()
    }

    pub(crate) fn set_signing_out(&mut self, signing_out: bool, cx: &mut Context<Self>) {
        self.signing_out = signing_out;
        cx.notify();
    }

    /// Show `kind`, as it was left. `sort` (from Home's "View all") applies
    /// that order first; the same order keeps the loaded pages.
    pub(crate) fn show(
        &mut self,
        kind: LibraryKind,
        sort: Option<LibrarySort>,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        let mut requests = self.model.show(kind);
        if let Some(sort) = sort {
            let changed = self.model.set_sort(sort);
            if !changed.is_empty() {
                self.grids[&kind].reset();
            }
            requests.extend(changed);
        }
        self.run(requests, cx);
        self.restore_focus = true;
        cx.notify();
    }

    /// Library is visible again after Details (and perhaps the Player)
    /// closed, or after another root. `playback` is set when the Player was
    /// open meanwhile: the opened title is asked for again so its progress
    /// is current. Nothing else reloads; scroll and focus are untouched.
    pub(crate) fn resume(&mut self, playback: bool, cx: &mut Context<Self>) {
        if playback && let Some(request) = self.model.reconcile() {
            self.run(vec![request], cx);
        }
        self.restore_focus = true;
        cx.notify();
    }

    /// Reload from the first page with the same selections. The titles stay
    /// until the new first page arrives.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        let requests = self.model.refresh();
        self.run(requests, cx);
        cx.notify();
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let requests = self.model.retry();
        self.run(requests, cx);
        cx.notify();
    }

    /// A query change: the grid starts at the top with nothing focused.
    fn requery(&mut self, requests: Vec<Request>, cx: &mut Context<Self>) {
        self.menu = None;
        if requests.iter().any(|r| matches!(r, Request::Page { .. })) {
            self.grids[&self.kind()].reset();
            self.restore_focus = true;
        }
        self.run(requests, cx);
        cx.notify();
    }

    fn open_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(id) = self.model.items().get(index).map(|item| item.id().clone()) else {
            return;
        };
        self.model.note_opened(id.clone());
        cx.emit(LibraryEvent::Open(id));
    }

    fn bar_action(&mut self, action: BarAction, _: &mut Window, cx: &mut Context<Self>) {
        match action {
            BarAction::Go(RootDestination::Library(kind)) if kind == self.kind() => {}
            BarAction::Go(destination) => cx.emit(LibraryEvent::Navigate(destination)),
            BarAction::Refresh => self.refresh(cx),
            BarAction::SignOut => cx.emit(LibraryEvent::SignOut),
        }
    }

    /// Start each request. A page request replaces the kind's earlier one,
    /// which is aborted; its answer could not apply anyway.
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
                Request::Page { kind, .. } => {
                    if let Some(previous) = self.page_tasks.insert(*kind, task) {
                        previous.abort();
                    }
                }
                _ => self.tasks.push(task),
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

    fn answer(&mut self, request: &Request, response: Response, cx: &mut Context<Self>) {
        match self.model.apply(request, response) {
            Applied::Ignored => return,
            Applied::SessionExpired => {
                cx.emit(LibraryEvent::SessionExpired);
                return;
            }
            Applied::Replaced => {
                if let Request::Page { kind, .. } = request {
                    let grid = &self.grids[kind];
                    // A refresh keeps the focused title if it is still
                    // there; otherwise the grid starts at the top.
                    match (*kind == self.kind())
                        .then(|| self.model.focused_index())
                        .flatten()
                    {
                        Some(index) => grid.focus_index(Some(index)),
                        None => grid.reset(),
                    }
                }
                self.restore_focus = true;
            }
            Applied::Appended | Applied::Updated => {}
        }
        if stats_enabled() {
            eprintln!(
                "library: {:?} · art slots {} · in flight {} · cache {:?}",
                self.model.stats(),
                self.art.len(),
                self.art_tasks.len(),
                self.loader.stats()
            );
        }
        cx.notify();
    }

    /// Keep artwork for exactly the cards the grid builds, and ask for the
    /// next page when they near the end of what is loaded. Runs from
    /// render, but does work only when the window actually moved.
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

    /// After a return or a first load, put focus on the grid (its focused
    /// card is kept in the grid state).
    fn settle_focus(&mut self, window: &mut Window) {
        if !self.restore_focus || self.menu.is_some() {
            return;
        }
        let target = match self.model.state() {
            CatalogState::Ready => self.grids[&self.kind()].focus_handle().clone(),
            CatalogState::Loading => return,
            _ => self.focus.clone(),
        };
        self.restore_focus = false;
        window.on_next_frame(move |window, _| window.focus(&target));
    }
}

impl Drop for LibraryScreen {
    fn drop(&mut self) {
        for (_, task) in self.page_tasks.drain() {
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

fn stats_enabled() -> bool {
    std::env::var_os("MATINEE_LIBRARY_STATS").is_some()
}

impl Render for LibraryScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.settle_focus(window);
        let theme = cx.theme().clone();
        let size = window.viewport_size();
        let (width, height) = (f32::from(size.width), f32::from(size.height));
        let layout = Layout::for_width(width);
        let kind = self.kind();
        let state = self.model.state();

        let body = match state {
            CatalogState::Ready => self.grid_view(&theme, layout, (width, height), cx),
            CatalogState::Loading => {
                skeleton(&theme, LIBRARY_IDS, layout, width).into_any_element()
            }
            CatalogState::Empty(reason) => self.empty(&theme, layout, reason, cx),
            CatalogState::Failed(failure) => self.failure(layout, failure, cx),
        };
        if state != CatalogState::Ready {
            self.follow_window(0..0, cx);
        }

        div()
            .id("library")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.colors.surface.canvas)
            .track_focus(&self.focus)
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
                                    active: RootDestination::Library(kind),
                                    name: self.session.user().name().to_string(),
                                    refresh_disabled: self.model.is_loading(),
                                    signing_out: self.signing_out,
                                },
                                cx,
                                Self::bar_action,
                            )),
                    )
                    .child(self.header(&theme, layout, cx))
                    .child(div().flex_1().min_h(px(0.0)).w_full().child(body)),
            )
    }
}

impl LibraryScreen {
    fn header(&self, theme: &Theme, layout: Layout, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self.kind();
        let eyebrow = self.model.view_name().unwrap_or("Your library").to_string();
        let count = match (self.model.state(), self.model.total()) {
            (CatalogState::Ready, Some(total)) => Some(count_label(total)),
            (CatalogState::Ready, None) => Some(count_label(self.model.items().len())),
            (CatalogState::Empty(_), _) => Some(count_label(0)),
            _ => None,
        };
        h_stack(Space::S4)
            .flex_none()
            .w_full()
            .items_end()
            .px(layout.gutter.px())
            .pt(Space::S8.px())
            .pb(Space::S5.px())
            .child(
                v_stack(Space::S1)
                    .child(
                        Text::new(eyebrow)
                            .role(TextRole::Caption)
                            .color(theme.colors.control.accent),
                    )
                    .child(Text::new(kind.title()).role(TextRole::Title))
                    .child(
                        Text::new(count.unwrap_or_else(|| "\u{a0}".into()))
                            .role(TextRole::Caption)
                            .tone(TextTone::Muted),
                    ),
            )
            .child(div().flex_1())
            .child(self.toolbar(cx))
    }

    /// Library, Genre, Show, and Sort. Each is one quiet button whose label
    /// says the current choice; a choice that narrows the titles is drawn
    /// stronger so it is never forgotten. Library appears only with more
    /// than one library; Genre only once genres have loaded.
    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = h_stack(Space::S2).items_center().flex_wrap();
        let query = self.model.query().clone();
        let views = self.model.views();
        if views.len() > 1 {
            let mut entries = vec![MenuEntry::Item(self.choice(
                "All libraries",
                query.view.is_none(),
                cx,
                |this, cx| {
                    let requests = this.model.set_view(None);
                    this.requery(requests, cx);
                },
            ))];
            for view in views {
                let id = view.id.clone();
                entries.push(MenuEntry::Item(self.choice(
                    &view.name,
                    query.view.as_ref() == Some(&view.id),
                    cx,
                    move |this, cx| {
                        let requests = this.model.set_view(Some(id.clone()));
                        this.requery(requests, cx);
                    },
                )));
            }
            let label = self
                .model
                .view_name()
                .unwrap_or("All libraries")
                .to_string();
            row = row.child(self.menu_button(
                MenuKind::View,
                label,
                query.view.is_some(),
                entries,
                cx,
            ));
        }
        if let Fetch::Ready(genres) = self.model.genres()
            && !genres.is_empty()
        {
            let mut entries = vec![MenuEntry::Item(self.choice(
                "All genres",
                query.filter.genre.is_none(),
                cx,
                |this, cx| {
                    let requests = this.model.set_genre(None);
                    this.requery(requests, cx);
                },
            ))];
            for genre in genres {
                let id = genre.id.clone();
                entries.push(MenuEntry::Item(self.choice(
                    &genre.name,
                    query.filter.genre.as_ref() == Some(&genre.id),
                    cx,
                    move |this, cx| {
                        let requests = this.model.set_genre(Some(id.clone()));
                        this.requery(requests, cx);
                    },
                )));
            }
            let label = self.model.genre_name().unwrap_or("All genres").to_string();
            row = row.child(self.menu_button(
                MenuKind::Genre,
                label,
                query.filter.genre.is_some(),
                entries,
                cx,
            ));
        }
        let shows = WatchFilter::ALL
            .into_iter()
            .map(|watch| {
                MenuEntry::Item(self.choice(
                    watch.label(),
                    query.filter.watch == watch,
                    cx,
                    move |this, cx| {
                        let requests = this.model.set_watch(watch);
                        this.requery(requests, cx);
                    },
                ))
            })
            .collect();
        row = row.child(self.menu_button(
            MenuKind::Show,
            query.filter.watch.label().to_string(),
            query.filter.watch != WatchFilter::All,
            shows,
            cx,
        ));
        let sorts = LibrarySort::ALL
            .into_iter()
            .map(|sort| {
                MenuEntry::Item(self.choice(
                    sort.label(),
                    query.sort == sort,
                    cx,
                    move |this, cx| {
                        let requests = this.model.set_sort(sort);
                        this.requery(requests, cx);
                    },
                ))
            })
            .collect();
        row.child(self.menu_button(
            MenuKind::Sort,
            format!("Sort: {}", query.sort.label()),
            false,
            sorts,
            cx,
        ))
    }

    fn choice(
        &self,
        label: &str,
        checked: bool,
        cx: &mut Context<Self>,
        pick: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> MenuItem {
        let entity = cx.entity().downgrade();
        let pick = Rc::new(pick);
        MenuItem::new(label.to_string())
            .checked(checked)
            .on_activate(move |_, cx| {
                let pick = Rc::clone(&pick);
                entity.update(cx, |this, cx| pick(this, cx)).ok();
            })
    }

    fn menu_button(
        &self,
        kind: MenuKind,
        label: String,
        narrowing: bool,
        entries: Vec<MenuEntry>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = self.menu == Some(kind);
        Popover::new(("library-menu", kind as usize))
            .open(open)
            .align(Alignment::End)
            .menu_max_height(MENU_HEIGHT)
            .on_dismiss({
                let entity = cx.entity().downgrade();
                move |_, cx| {
                    entity
                        .update(cx, |this, cx| {
                            if this.menu == Some(kind) {
                                this.menu = None;
                                cx.notify();
                            }
                        })
                        .ok();
                }
            })
            .trigger(
                Button::new(("library-menu-button", kind as usize), label)
                    .variant(if narrowing {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Subtle
                    })
                    .size(ButtonSize::Small)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.menu = if this.menu == Some(kind) {
                            None
                        } else {
                            Some(kind)
                        };
                        cx.notify();
                    })),
            )
            .menu(entries)
    }

    fn grid_view(
        &mut self,
        theme: &Theme,
        layout: Layout,
        window: (f32, f32),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kind = self.kind();
        let grid = self.grids[&kind].clone();
        let sizing = layout.sizing();
        let count = self.model.items().len();
        let fallback = (window.0, (window.1 - CHROME_ESTIMATE).max(0.0));
        let frame = grid.frame(sizing, count, FOOTER_HEIGHT, fallback);
        if !self.reported_first_grid && stats_enabled() {
            self.reported_first_grid = true;
            eprintln!(
                "library: first grid after {:?} ({} cards built)",
                self.opened_at.elapsed(),
                frame.materialized.len()
            );
        }
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
        let items: Rc<Vec<ItemId>> = Rc::new(
            self.model
                .items()
                .iter()
                .map(|item| item.id().clone())
                .collect(),
        );

        VirtualGrid::new(
            ("library-grid", kind as usize),
            &grid,
            count,
            move |cell, _, _| match cards.borrow_mut().remove(&cell.index) {
                Some(data) => card(
                    &card_theme,
                    LIBRARY_IDS,
                    &data,
                    cell_width,
                    art_height,
                    cell.focused,
                ),
                None => div().into_any_element(),
            },
        )
        .sizing(sizing)
        .fallback_viewport(fallback)
        .footer(FOOTER_HEIGHT, footer)
        .on_activate(move |index, _, cx| {
            open.update(cx, |this, cx| this.open_item(index, cx)).ok();
        })
        .on_focus(move |index, _, cx| {
            if let Some(id) = items.get(index).cloned() {
                focused
                    .update(cx, |this, _| this.model.note_focused(id))
                    .ok();
            }
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
                .child(quiet(theme, "Loading more titles…"))
                .into_any_element(),
            Footer::Failed(failure) => row
                .child(quiet(
                    theme,
                    match failure {
                        LibraryFailure::Unreachable => {
                            "More titles couldn't be loaded. Jellyfin isn't answering."
                        }
                        _ => "More titles couldn't be loaded.",
                    },
                ))
                .child(
                    Button::new("library-more-retry", "Try again")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                )
                .into_any_element(),
            Footer::Idle | Footer::End => div().into_any_element(),
        }
    }

    fn empty(
        &self,
        theme: &Theme,
        layout: Layout,
        reason: EmptyReason,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_stack(Space::S3)
            .px(layout.gutter.px())
            .pt(Space::S8.px())
            .child(
                Text::new(reason.message())
                    .role(TextRole::Body)
                    .tone(TextTone::Secondary),
            )
            .when(reason == EmptyReason::Filtered, |stack| {
                stack.child(
                    div().child(
                        Button::new("library-clear-filters", "Clear filters")
                            .variant(ButtonVariant::Secondary)
                            .size(ButtonSize::Small)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let requests = this.model.clear_filters();
                                this.requery(requests, cx);
                            })),
                    ),
                )
            })
            .when(reason == EmptyReason::Library, |stack| {
                stack.child(quiet(
                    theme,
                    "Titles appear here once Jellyfin has scanned them.",
                ))
            })
            .into_any_element()
    }

    fn failure(
        &self,
        layout: Layout,
        failure: LibraryFailure,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                    Button::new("library-retry", "Try again")
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artwork::{ARTWORK_CACHE_BYTES, TILE_POSTER_WIDTH};
    use crate::media_grid::poster_request;
    use matinee_core::MediaItem;

    /// Columns, card width, and the most cards (and so poster slots) the
    /// grid can build at each review size, from the production sizing.
    fn plan(width: f32, height: f32) -> (usize, f32, usize) {
        let layout = GridLayout::new(Layout::for_width(width).sizing(), width, 10_000, 0.0);
        let viewport = height - CHROME_ESTIMATE;
        let mut most = 0;
        let mut scroll = 0.0;
        while scroll < 20.0 * layout.row_pitch() {
            most = most.max(layout.materialized(scroll, viewport).len());
            scroll += 7.0;
        }
        (layout.columns, layout.cell_width, most)
    }

    #[test]
    fn columns_follow_the_window_and_cards_stay_readable() {
        let sizes = [
            (960.0, 620.0),
            (1200.0, 760.0),
            (1440.0, 900.0),
            (1920.0, 1080.0),
        ];
        let columns: Vec<usize> = sizes.iter().map(|(w, h)| plan(*w, *h).0).collect();
        assert_eq!(columns, vec![5, 6, 7, 10]);
        for (width, height) in sizes {
            let (_, card, _) = plan(width, height);
            assert!((148.0..=180.0).contains(&card), "{width}: {card}");
        }
    }

    #[test]
    fn poster_slots_are_bounded_by_the_window_and_fit_the_cache() {
        let (_, _, small) = plan(960.0, 620.0);
        let (_, _, large) = plan(1920.0, 1080.0);
        assert!(small <= 25, "{small}");
        assert!(large <= 60, "{large}");
        // A 360-pixel 2:3 poster decodes to 360 × 540 × 4 bytes.
        let bytes = large * 360 * 540 * 4;
        assert!(bytes < ARTWORK_CACHE_BYTES / 2, "{bytes}");
    }

    #[test]
    fn library_posters_share_home_s_address() {
        let session = crate::model::review_session();
        let urls = session.artwork();
        let item = super::super::preview::fixture(LibraryKind::Movies, 0);
        let request = poster_request(&item, &urls).unwrap();
        assert!(
            request
                .url
                .contains(&format!("maxWidth={TILE_POSTER_WIDTH}"))
        );
        assert!(request.url.contains("/Images/Primary"));
        assert!(!request.url.contains("api_key"));
        assert!(!request.url.contains(crate::test_support::FIXTURE_TOKEN));
        // A window past the loaded titles asks for nothing extra.
        let items: Vec<MediaItem> = (0..10)
            .map(|index| super::super::preview::fixture(LibraryKind::Movies, index))
            .collect();
        // Titles 5 to 9 are loaded; 8 has no poster.
        assert_eq!(artwork_window(&items, 5..40, &urls).len(), 4);
        assert!(artwork_window(&items, 20..40, &urls).is_empty());
        // Title 8 has no poster: no request, the placeholder shows.
        assert_eq!(artwork_window(&items, 8..9, &urls).len(), 0);
    }
}
