//! Home screen.
//!
//! Paints [`HomeModel`] and runs its requests on the service runtime, all
//! shelves at once. Artwork comes from the shared [`ArtworkLoader`]: this
//! screen decides which images it shows, starts the missing ones, and
//! cancels the ones it no longer shows.
//!
//! Home stays alive under Details and the Player. Its page offset, each
//! row's offset, and the card that was opened are kept here, so coming back
//! lands where the person left. Choosing a card emits [`HomeEvent::Open`];
//! the hero's Play or Resume emits [`HomeEvent::Play`]. Home never plans or
//! reports playback.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use atelier_ui::gpui::{EventEmitter, KeyDownEvent, linear_color_stop, linear_gradient};
use atelier_ui::prelude::*;
use matinee_core::{HomeShelf, ImageRole, ItemId, ItemKind, LibraryKind, LibrarySort, MediaItem};
use matinee_jellyfin::{ArtworkRequest, ArtworkUrls, Session};
use tokio::task::JoinHandle;

use super::load;
use super::model::{
    Applied, FocusTarget, HeroPick, HeroState, HomeFailure, HomeModel, PageState, Request,
    ShelfState, card_detail, card_title, remaining_label,
};
use crate::app_bar::{AppBar, BarAction, app_bar};
use crate::artwork::{
    Artwork, ArtworkLoad, ArtworkLoader, BACKDROP_WIDTH, Client, THUMB_WIDTH, TILE_POSTER_WIDTH,
};
use crate::details::{PlayAction, meta_line, progress_fraction, summary};
use crate::nav::RootDestination;
use crate::runtime::ServiceRuntime;
use crate::tiles::{art_frame, backdrop_request, progress_line, stable_index};

/// What the shell does for Home.
pub(crate) enum HomeEvent {
    /// Open Details for this item.
    Open(ItemId),
    /// Open the Player for this item, from the hero.
    Play(ItemId),
    /// Go to another root destination from the app bar.
    Navigate(RootDestination),
    /// "View all" on a row: Library for that kind, in the row's order.
    Browse(LibraryKind, LibrarySort),
    SignOut,
    /// Jellyfin no longer accepts the session.
    SessionExpired,
}

/// Shown under the empty Continue Watching heading.
pub(crate) const NOTHING_IN_PROGRESS: &str =
    "Nothing in progress. Anything you start will wait for you here.";

/// Shown when the library has no movies or series at all (shipping copy).
pub(crate) const EMPTY_LIBRARY: &str =
    "Your Jellyfin library is connected, but it does not contain any movies or series yet.";

pub(crate) struct HomeScreen {
    runtime: Arc<ServiceRuntime>,
    /// `None` for review scenes, which never open a socket.
    client: Option<Client>,
    /// Builds artwork addresses. No token is ever put on them.
    session: Session,
    loader: ArtworkLoader,
    pub(super) model: HomeModel,
    tasks: Vec<JoinHandle<()>>,
    pub(super) art: HashMap<String, Artwork>,
    art_tasks: HashMap<String, JoinHandle<()>>,
    focus: FocusHandle,
    hero_primary: FocusHandle,
    hero_secondary: FocusHandle,
    /// Whether a hero button had focus at the last paint.
    hero_focused: bool,
    pub(super) page: ScrollControl,
    rails: HashMap<HomeShelf, RailState>,
    cards: HashMap<(HomeShelf, ItemId), FocusHandle>,
    /// Put focus back once Home has something to focus.
    pub(super) restore_focus: bool,
    signing_out: bool,
}

impl EventEmitter<HomeEvent> for HomeScreen {}

impl HomeScreen {
    pub(crate) fn open(
        runtime: Arc<ServiceRuntime>,
        client: Client,
        loader: ArtworkLoader,
        cx: &mut Context<Self>,
    ) -> Self {
        let (model, requests) = HomeModel::open();
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
        model: HomeModel,
        cx: &mut Context<Self>,
    ) -> Self {
        // Images this screen decoded and the cache did not keep leave the
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
            tasks: Vec::new(),
            art: HashMap::new(),
            art_tasks: HashMap::new(),
            focus: cx.focus_handle(),
            hero_primary: cx.focus_handle().tab_index(0).tab_stop(true),
            hero_secondary: cx.focus_handle().tab_index(0).tab_stop(true),
            hero_focused: false,
            page: ScrollControl::new(),
            rails: HashMap::new(),
            cards: HashMap::new(),
            restore_focus: true,
            signing_out: false,
        }
    }

    /// The objects that hold Home's scroll positions, for tests that check
    /// they survive pages opening above Home.
    #[cfg(test)]
    pub(crate) fn scroll_state(&self) -> (ScrollControl, Vec<(HomeShelf, RailState)>) {
        let mut rails: Vec<(HomeShelf, RailState)> = self
            .rails
            .iter()
            .map(|(shelf, rail)| (*shelf, rail.clone()))
            .collect();
        rails.sort_by_key(|(shelf, _)| *shelf as usize);
        (self.page.clone(), rails)
    }

    pub(crate) fn set_signing_out(&mut self, signing_out: bool, cx: &mut Context<Self>) {
        self.signing_out = signing_out;
        cx.notify();
    }

    /// Home is visible again after Details (and perhaps the Player) closed.
    /// `refresh` is set when playback happened meanwhile, so progress on
    /// Continue Watching and Next Up is reloaded. The page and row offsets
    /// are untouched; focus returns to the card that was opened.
    pub(crate) fn resume(&mut self, refresh: bool, cx: &mut Context<Self>) {
        if refresh {
            self.refresh(cx);
        }
        self.restore_focus = true;
        cx.notify();
    }

    /// Reload every shelf. What is shown stays until the answers arrive.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        let requests = self.model.refresh();
        self.run(requests, cx);
        cx.notify();
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let requests = self.model.retry();
        self.run(requests, cx);
        self.restore_focus = true;
        cx.notify();
    }

    fn retry_shelf(&mut self, shelf: HomeShelf, cx: &mut Context<Self>) {
        let requests = self.model.retry_shelf(shelf);
        self.run(requests, cx);
        cx.notify();
    }

    fn open_item(&mut self, shelf: Option<HomeShelf>, item: ItemId, cx: &mut Context<Self>) {
        match shelf {
            Some(shelf) => self.model.note_opened(shelf, item.clone()),
            None => self.model.note_hero_used(),
        }
        cx.emit(HomeEvent::Open(item));
    }

    fn play(&mut self, item: ItemId, cx: &mut Context<Self>) {
        self.model.note_hero_used();
        cx.emit(HomeEvent::Play(item));
    }

    /// Start every request at once. Shelves do not wait for each other.
    fn run(&mut self, requests: Vec<Request>, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.tasks.retain(|task| !task.is_finished());
        for request in requests {
            let client = Arc::clone(&client);
            let (task, rx) = self
                .runtime
                .spawn(async move { load::run(client.as_ref(), request).await });
            self.tasks.push(task);
            cx.spawn(async move |this, cx| {
                let Ok(response) = rx.await else {
                    return;
                };
                this.update(cx, |this, cx| {
                    match this.model.apply(request, response) {
                        Applied::Ignored => return,
                        Applied::Updated => {}
                        Applied::SessionExpired => cx.emit(HomeEvent::SessionExpired),
                    }
                    this.sync_artwork(cx);
                    this.prune_cards();
                    if !this.model.is_loading()
                        && std::env::var_os("MATINEE_ARTWORK_STATS").is_some()
                    {
                        eprintln!("home artwork cache: {:?}", this.loader.stats());
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    /// Every image Home shows, hero first, then shelf by shelf, each address
    /// once (see [`artwork_plan`]).
    pub(super) fn wanted_artwork(&self) -> Vec<ArtworkRequest> {
        artwork_plan(&self.model, &self.session.artwork())
            .into_iter()
            .map(|planned| planned.request)
            .collect()
    }

    /// Start what is wanted and missing; cancel and forget what is not.
    fn sync_artwork(&mut self, cx: &mut Context<Self>) {
        let wanted = self.wanted_artwork();
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
                            // The slot left while the image was loading.
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

    fn art(&self, request: Option<&ArtworkRequest>) -> Artwork {
        match request {
            Some(request) => self.art.get(&request.url).cloned().unwrap_or_default(),
            None => Artwork::Missing,
        }
    }

    /// Forget focus handles for cards that left Home.
    fn prune_cards(&mut self) {
        let model = &self.model;
        self.cards
            .retain(|(shelf, id), _| model.items(*shelf).iter().any(|item| item.id() == id));
    }

    fn rail(&mut self, shelf: HomeShelf) -> RailState {
        self.rails.entry(shelf).or_default().clone()
    }

    fn card_focus(
        &mut self,
        shelf: HomeShelf,
        item: &ItemId,
        cx: &mut Context<Self>,
    ) -> FocusHandle {
        self.cards
            .entry((shelf, item.clone()))
            .or_insert_with(|| cx.focus_handle().tab_index(0).tab_stop(true))
            .clone()
    }

    /// Shelves that show cards now, in page order.
    fn card_shelves(&self) -> Vec<HomeShelf> {
        HomeShelf::ALL
            .into_iter()
            .filter(|shelf| !self.model.items(*shelf).is_empty())
            .collect()
    }

    /// Up and Down move between the hero and the rows. Left and Right are
    /// the rail's. Escape belongs to the shell.
    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let plain = !key.modifiers.shift
            && !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.platform;
        let down = match key.key.as_str() {
            "down" if plain => true,
            "up" if plain => false,
            _ => return,
        };
        let shelves = self.card_shelves();
        let on_hero =
            self.hero_primary.is_focused(window) || self.hero_secondary.is_focused(window);
        let current = shelves.iter().position(|shelf| {
            self.model.items(*shelf).iter().any(|item| {
                self.cards
                    .get(&(*shelf, item.id().clone()))
                    .is_some_and(|focus| focus.is_focused(window))
            })
        });
        let target = match (on_hero, current, down) {
            (true, _, true) => shelves.first().copied(),
            (false, Some(index), true) => shelves.get(index + 1).copied(),
            (false, Some(0), false) => {
                self.focus_hero(window, cx);
                cx.stop_propagation();
                return;
            }
            (false, Some(index), false) => shelves.get(index - 1).copied(),
            _ => None,
        };
        let Some(shelf) = target else {
            return;
        };
        let items = self.model.items(shelf);
        let column = self
            .rails
            .get(&shelf)
            .and_then(RailState::last_focused)
            .unwrap_or(0)
            .min(items.len().saturating_sub(1));
        let Some(id) = items.get(column).map(|item| item.id().clone()) else {
            return;
        };
        let focus = self.card_focus(shelf, &id, cx);
        note_keyboard_navigation(cx);
        window.focus(&focus);
        cx.stop_propagation();
    }

    fn focus_hero(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        note_keyboard_navigation(cx);
        window.focus(&self.hero_primary);
    }

    /// After a return or a first load, put focus on the opened card, or the
    /// hero's first button. Waits while the hero is still loading.
    fn settle_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.restore_focus {
            return;
        }
        let target = match self.model.return_focus() {
            FocusTarget::Card(shelf, id) => Some(self.card_focus(shelf, &id, cx)),
            FocusTarget::Hero => match (self.model.hero(), self.model.page()) {
                (HeroState::Loading, PageState::Content) => return,
                (HeroState::Ready(_), _) => Some(self.hero_primary.clone()),
                (_, PageState::Failed(_)) => Some(self.hero_primary.clone()),
                _ => None,
            },
        };
        self.restore_focus = false;
        let target = target.unwrap_or_else(|| self.focus.clone());
        window.on_next_frame(move |window, _| window.focus(&target));
    }
}

impl Drop for HomeScreen {
    fn drop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        for (_, task) in self.art_tasks.drain() {
            task.abort();
        }
    }
}

/// The shape an image is drawn in. Jellyfin scales to the requested width
/// and keeps the source's aspect, so these are the expected aspects, used
/// for planning memory, not a guarantee about any one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ArtShape {
    /// The hero backdrop, 16:9.
    Backdrop,
    /// A Continue Watching or Next Up card: a still or backdrop, 16:9.
    Landscape,
    /// A poster card, 2:3.
    Poster,
}

impl ArtShape {
    /// Height over width.
    #[cfg(test)]
    pub(super) fn aspect(self) -> f32 {
        match self {
            Self::Backdrop | Self::Landscape => 9.0 / 16.0,
            Self::Poster => 1.5,
        }
    }
}

/// One image Home will ask the loader for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PlannedArt {
    pub request: ArtworkRequest,
    pub shape: ArtShape,
    /// The `maxWidth` the request asks for.
    pub width: u32,
}

/// Every image Home shows, hero first, then shelf by shelf. An address
/// that appears more than once (a movie in Recently Added and Favorites)
/// is listed once, so one fetch serves every slot. The hero's 1920 px
/// backdrop and a card's 480 px backdrop of the same item are different
/// addresses and both stay: the hero needs the larger image.
pub(super) fn artwork_plan(model: &HomeModel, urls: &ArtworkUrls<'_>) -> Vec<PlannedArt> {
    let mut plan: Vec<PlannedArt> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push = |request: Option<ArtworkRequest>, shape: ArtShape, width: u32| {
        if let Some(request) = request
            && seen.insert(request.url.clone())
        {
            plan.push(PlannedArt {
                request,
                shape,
                width,
            });
        }
    };
    if let HeroState::Ready(pick) = model.hero() {
        push(
            backdrop_request(&pick.item, urls),
            ArtShape::Backdrop,
            BACKDROP_WIDTH,
        );
    }
    for shelf in HomeShelf::ALL {
        let (shape, width) = card_shape(shelf);
        for item in model.items(shelf) {
            push(card_request(shelf, item, urls), shape, width);
        }
    }
    plan
}

/// How a shelf's cards are drawn and the width they ask for.
fn card_shape(shelf: HomeShelf) -> (ArtShape, u32) {
    if landscape(shelf) {
        (ArtShape::Landscape, THUMB_WIDTH)
    } else {
        (ArtShape::Poster, TILE_POSTER_WIDTH)
    }
}

/// The picture on a card: a still or a backdrop for the landscape rows,
/// the poster elsewhere. Sizes follow the drawn size, not the hero's.
pub(super) fn card_request(
    shelf: HomeShelf,
    item: &MediaItem,
    urls: &ArtworkUrls<'_>,
) -> Option<ArtworkRequest> {
    let (shape, width) = card_shape(shelf);
    match shape {
        ArtShape::Landscape if item.kind == ItemKind::Episode => {
            urls.item_request(item, ImageRole::Primary, width)
        }
        ArtShape::Landscape => urls
            .item_request(item, ImageRole::Backdrop, width)
            .or_else(|| urls.item_request(item, ImageRole::Primary, width)),
        _ => urls.item_request(item, ImageRole::Primary, width),
    }
}

/// Continue Watching and Next Up are landscape, so what you are in the
/// middle of reads differently from what is new.
fn landscape(shelf: HomeShelf) -> bool {
    matches!(shelf, HomeShelf::ContinueWatching | HomeShelf::NextUp)
}

/// Measurements that follow the window size.
#[derive(Clone, Copy)]
pub(super) struct Layout {
    gutter: Space,
    hero_height: f32,
    poster: (f32, f32),
    landscape: (f32, f32),
    copy_width: f32,
    compact: bool,
}

impl Layout {
    pub(super) fn for_viewport(width: f32, height: f32) -> Self {
        let compact = width < 1100.0;
        let wide = width >= 1600.0;
        let poster = if compact {
            128.0
        } else if wide {
            176.0
        } else {
            148.0
        };
        let card = if compact {
            240.0
        } else if wide {
            336.0
        } else {
            288.0
        };
        Self {
            gutter: if compact { Space::S10 } else { Space::S16 },
            // A restrained hero: the first row always starts on screen.
            hero_height: (height * 0.62).clamp(360.0, 620.0),
            poster: (poster, poster * 1.5),
            landscape: (card, card * 9.0 / 16.0),
            copy_width: if compact {
                460.0
            } else if wide {
                640.0
            } else {
                560.0
            },
            compact,
        }
    }
}

impl Render for HomeScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.settle_focus(window, cx);
        let hero_focused =
            self.hero_primary.is_focused(window) || self.hero_secondary.is_focused(window);
        if hero_focused && !self.hero_focused && input_modality(cx) == InputModality::Keyboard {
            self.page.reveal_child(0);
        }
        self.hero_focused = hero_focused;

        let theme = cx.theme().clone();
        let viewport = window.viewport_size();
        let (width, height) = (f32::from(viewport.width), f32::from(viewport.height));
        let layout = Layout::for_viewport(width, height);

        let mut page = ScrollView::vertical("home-scroll")
            .control(self.page.clone())
            // A sideways swipe over a row moves the row, not the page.
            .restrict_to_axis(true)
            .size_full()
            .flex()
            .flex_col()
            .child(self.hero(&theme, layout, width, cx));
        match self.model.page() {
            PageState::Failed(failure) => {
                page = page.child(
                    div()
                        .flex_none()
                        .child(self.failure(&theme, layout, failure, cx)),
                );
            }
            PageState::Empty => {
                page = page.child(
                    div()
                        .flex_none()
                        .px(layout.gutter.px())
                        .pt(Space::S8.px())
                        .child(quiet(&theme, EMPTY_LIBRARY)),
                );
            }
            PageState::Content => {
                let mut index = 1;
                for shelf in HomeShelf::ALL {
                    if let Some(section) =
                        self.shelf_section(&theme, layout, width, shelf, index, cx)
                    {
                        page = page.child(section);
                        index += 1;
                    }
                }
            }
        }
        page = page.child(div().h(Space::S16.px()).flex_none());

        div()
            .id("home")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.colors.surface.canvas)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event, window, cx| this.on_key(event, window, cx)))
            .child(page)
    }
}

impl HomeScreen {
    fn hero(
        &self,
        theme: &Theme,
        layout: Layout,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hero = self.model.hero();
        let backdrop = match &hero {
            HeroState::Ready(pick) => {
                Some(self.art(backdrop_request(&pick.item, &self.session.artwork()).as_ref()))
            }
            _ => None,
        };
        let body = match &hero {
            HeroState::Ready(pick) => self.hero_copy(theme, layout, pick, cx).into_any_element(),
            HeroState::Loading => hero_skeleton(theme).into_any_element(),
            HeroState::None => div().into_any_element(),
        };
        div()
            .id("home-hero")
            .relative()
            .flex_none()
            .w_full()
            .h(px(layout.hero_height))
            .overflow_hidden()
            .child(backdrop_layer(
                theme,
                backdrop.as_ref(),
                width,
                layout.hero_height,
            ))
            .child(
                v_stack(Space::S0)
                    .absolute()
                    .inset_0()
                    .px(layout.gutter.px())
                    .pt(Space::S5.px())
                    .pb(Space::S10.px())
                    .child(self.top_bar(theme, cx))
                    .child(div().flex_1())
                    .child(body),
            )
            .into_any_element()
    }

    /// The app bar: wordmark, Home, Movies, Series, name, Refresh, Sign out.
    fn top_bar(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        app_bar(
            theme,
            AppBar {
                active: RootDestination::Home,
                name: self.session.user().name().to_string(),
                refresh_disabled: self.model.is_loading(),
                signing_out: self.signing_out,
            },
            cx,
            |this, action, _, cx| match action {
                BarAction::Go(RootDestination::Home) => {}
                BarAction::Go(destination) => cx.emit(HomeEvent::Navigate(destination)),
                BarAction::Refresh => this.refresh(cx),
                BarAction::SignOut => cx.emit(HomeEvent::SignOut),
            },
        )
    }

    fn hero_copy(
        &self,
        theme: &Theme,
        layout: Layout,
        pick: &HeroPick,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let item = &pick.item;
        let title = pick.title().to_string();
        let title_role = if layout.compact || title.chars().count() > 24 {
            TextRole::Title
        } else {
            TextRole::Display
        };
        let episode = (item.kind == ItemKind::Episode)
            .then(|| card_detail(item))
            .flatten();
        let meta = meta_line(item);
        let overview = item
            .metadata
            .overview
            .as_deref()
            .map(|text| summary(text, if layout.compact { 160 } else { 240 }))
            .filter(|text| !text.is_empty());
        let play = pick.play();
        let progress = play
            .as_ref()
            .and_then(|action| action.resume)
            .and(progress_fraction(item));
        let target = item.id().clone();
        let details_label = if item.kind == ItemKind::Series {
            "View series"
        } else {
            "Details"
        };

        v_stack(Space::S3)
            .max_w(px(layout.copy_width))
            .child(
                Text::new(pick.eyebrow())
                    .role(TextRole::Caption)
                    .color(theme.colors.control.accent),
            )
            .child(Text::new(title).role(title_role))
            .when_some(episode, |stack, line| {
                stack.child(
                    Text::new(line)
                        .role(TextRole::Subheading)
                        .tone(TextTone::Secondary),
                )
            })
            .when(!meta.is_empty(), |stack| {
                stack.child(
                    Text::new(meta)
                        .role(TextRole::Metadata)
                        .tone(TextTone::Secondary),
                )
            })
            .when_some(overview, |stack, text| {
                stack.child(
                    Text::new(text)
                        .role(TextRole::Body)
                        .tone(TextTone::Secondary),
                )
            })
            .child(
                h_stack(Space::S3)
                    .pt(Space::S2.px())
                    .items_center()
                    .flex_wrap()
                    .when_some(play, |row, action| {
                        row.child(self.hero_play_button(action, cx))
                    })
                    .child({
                        let primary = pick.play().is_none();
                        let focus = if primary {
                            self.hero_primary.clone()
                        } else {
                            self.hero_secondary.clone()
                        };
                        Button::new("home-hero-details", details_label)
                            .variant(if primary {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            })
                            .size(ButtonSize::Large)
                            .icon(IconName::Info)
                            .focus_handle(focus)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_item(None, target.clone(), cx);
                            }))
                    })
                    .when_some(progress, |row, fraction| {
                        row.child(progress_line(theme, fraction, 140.0))
                    }),
            )
    }

    fn hero_play_button(&self, action: PlayAction, cx: &mut Context<Self>) -> impl IntoElement {
        let target = action.target.clone();
        Button::new("home-hero-play", action.label())
            .variant(ButtonVariant::Primary)
            .size(ButtonSize::Large)
            .icon(IconName::Play)
            .focus_handle(self.hero_primary.clone())
            .on_click(cx.listener(move |this, _, _, cx| this.play(target.clone(), cx)))
    }

    fn failure(
        &self,
        theme: &Theme,
        layout: Layout,
        failure: HomeFailure,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_stack(Space::S3)
            .px(layout.gutter.px())
            .pt(Space::S6.px())
            .max_w(px(620.0))
            .child(
                Text::new("Home")
                    .role(TextRole::Caption)
                    .color(theme.colors.control.accent),
            )
            .child(Text::new(failure.title()).role(TextRole::Title))
            .child(
                Text::new(failure.message())
                    .role(TextRole::Body)
                    .tone(TextTone::Secondary),
            )
            .child(
                h_stack(Space::S3).pt(Space::S3.px()).child(
                    Button::new("home-retry", "Try again")
                        .variant(ButtonVariant::Primary)
                        .focus_handle(self.hero_primary.clone())
                        .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                ),
            )
    }

    /// One shelf, or nothing when an answered shelf is empty (Continue
    /// Watching says so instead). `page_index` is its place on the page.
    #[allow(clippy::too_many_arguments)]
    fn shelf_section(
        &mut self,
        theme: &Theme,
        layout: Layout,
        width: f32,
        shelf: HomeShelf,
        page_index: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let body = match self.model.shelf(shelf) {
            ShelfState::Loading => skeleton_row(theme, layout, shelf).into_any_element(),
            ShelfState::Failed(_) => h_stack(Space::S3)
                .items_center()
                .child(quiet(
                    theme,
                    &format!("{} isn't available right now.", shelf.title()),
                ))
                .child(
                    Button::new(("home-shelf-retry", shelf as usize), "Try again")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| this.retry_shelf(shelf, cx))),
                )
                .into_any_element(),
            ShelfState::Ready(_) if self.model.items(shelf).is_empty() => {
                if shelf != HomeShelf::ContinueWatching {
                    return None;
                }
                quiet(theme, NOTHING_IN_PROGRESS).into_any_element()
            }
            ShelfState::Ready(_) => self.rail_for(theme, layout, shelf, page_index, cx),
        };
        let rail = self.rails.get(&shelf).cloned();
        // Decided from the layout, so the arrows are there on the first
        // frame, before the rail has been measured.
        let (card_width, _) = if landscape(shelf) {
            layout.landscape
        } else {
            layout.poster
        };
        let count = self.model.items(shelf).len() as f32;
        let content = count * (card_width + Space::S2.value() + Space::S4.value())
            + 2.0 * layout.gutter.value();
        let pager = matches!(self.model.shelf(shelf), ShelfState::Ready(_)) && content > width;
        Some(
            v_stack(Space::S3)
                .flex_none()
                .w_full()
                .pt(Space::S8.px())
                .child(
                    h_stack(Space::S2)
                        .px(layout.gutter.px())
                        .items_center()
                        .child(Text::new(shelf.title()).role(TextRole::Heading))
                        .child(div().flex_1())
                        .when_some(
                            browse_target(shelf).filter(|_| {
                                matches!(self.model.shelf(shelf), ShelfState::Ready(_))
                                    && !self.model.items(shelf).is_empty()
                            }),
                            |row, kind| {
                                row.child(
                                    Button::new(("home-view-all", shelf as usize), "View all")
                                        .variant(ButtonVariant::Subtle)
                                        .size(ButtonSize::Small)
                                        .on_click(cx.listener(move |_, _, _, cx| {
                                            cx.emit(HomeEvent::Browse(
                                                kind,
                                                LibrarySort::DateCreated,
                                            ));
                                        })),
                                )
                            },
                        )
                        .when_some(rail.filter(|_| pager), |row, rail| {
                            row.child(pager_buttons(shelf, rail, cx))
                        }),
                )
                .child(div().w_full().child(body))
                .into_any_element(),
        )
    }

    fn rail_for(
        &mut self,
        theme: &Theme,
        layout: Layout,
        shelf: HomeShelf,
        page_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.rail(shelf);
        let page = self.page.clone();
        let items: Vec<MediaItem> = self.model.items(shelf).into_iter().cloned().collect();
        let mut rail = Rail::new(("home-rail", shelf as usize), &state)
            .w_full()
            .px(layout.gutter.px())
            .py(Space::S1.px())
            .on_focus(move |_, _, cx| {
                if input_modality(cx) == InputModality::Keyboard {
                    page.reveal_child(page_index);
                }
            });
        for item in &items {
            let focus = self.card_focus(shelf, item.id(), cx);
            let card = if landscape(shelf) {
                self.landscape_card(theme, layout, shelf, item, focus.clone(), cx)
            } else {
                self.poster_card(theme, layout, shelf, item, focus.clone(), cx)
            };
            rail = rail.item(focus, card);
        }
        rail.into_any_element()
    }

    fn landscape_card(
        &self,
        theme: &Theme,
        layout: Layout,
        shelf: HomeShelf,
        item: &MediaItem,
        focus: FocusHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let urls = self.session.artwork();
        let art = self.art(card_request(shelf, item, &urls).as_ref());
        let (w, h) = layout.landscape;
        let action = PlayAction::for_item(item);
        let progress = action.resume.and(progress_fraction(item));
        let title = card_title(item).to_string();
        let detail = match (shelf, item.kind == ItemKind::Episode) {
            (HomeShelf::ContinueWatching, false) => remaining_label(item),
            _ => card_detail(item),
        };
        let label = match action.resume {
            Some(_) => format!("{title}, in progress"),
            None => title.clone(),
        };
        let target = item.id().clone();
        let key = stable_index(item.id());
        Pressable::new((shelf_id(shelf, "card"), key), label)
            .focus_handle(focus)
            .w(px(w + Space::S2.value()))
            .p(Space::S1.px())
            .on_press(cx.listener(move |this, _, _, cx| {
                this.open_item(Some(shelf), target.clone(), cx);
            }))
            .child(
                v_stack(Space::S2)
                    .child(
                        div()
                            .relative()
                            .child(art_frame(
                                theme,
                                (shelf_id(shelf, "art"), key),
                                &art,
                                w,
                                h,
                                Radius::Medium,
                                &title,
                            ))
                            .when_some(progress, |frame, fraction| {
                                frame.child(
                                    div()
                                        .absolute()
                                        .left(Space::S3.px())
                                        .right(Space::S3.px())
                                        .bottom(Space::S3.px())
                                        .child(progress_line(theme, fraction, w - 24.0)),
                                )
                            }),
                    )
                    .child(Text::new(title).role(TextRole::Label).truncate())
                    .when_some(detail, |stack, detail| {
                        stack.child(
                            Text::new(detail)
                                .role(TextRole::Caption)
                                .tone(TextTone::Muted)
                                .truncate(),
                        )
                    }),
            )
            .into_any_element()
    }

    fn poster_card(
        &self,
        theme: &Theme,
        layout: Layout,
        shelf: HomeShelf,
        item: &MediaItem,
        focus: FocusHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let urls = self.session.artwork();
        let art = self.art(card_request(shelf, item, &urls).as_ref());
        let (w, h) = layout.poster;
        let progress = PlayAction::for_item(item)
            .resume
            .and(progress_fraction(item));
        let watched = item.user.is_played() && progress.is_none();
        let year = card_detail(item);
        let target = item.id().clone();
        let key = stable_index(item.id());
        Pressable::new((shelf_id(shelf, "card"), key), item.name().to_string())
            .focus_handle(focus)
            .w(px(w + Space::S2.value()))
            .p(Space::S1.px())
            .on_press(cx.listener(move |this, _, _, cx| {
                this.open_item(Some(shelf), target.clone(), cx);
            }))
            .child(
                v_stack(Space::S2)
                    .child(
                        div()
                            .relative()
                            .child(art_frame(
                                theme,
                                (shelf_id(shelf, "art"), key),
                                &art,
                                w,
                                h,
                                Radius::Medium,
                                item.name(),
                            ))
                            .when_some(progress, |frame, fraction| {
                                frame.child(
                                    div()
                                        .absolute()
                                        .left(Space::S2.px())
                                        .right(Space::S2.px())
                                        .bottom(Space::S2.px())
                                        .child(progress_line(theme, fraction, w - 16.0)),
                                )
                            }),
                    )
                    .child(
                        Text::new(item.name().to_string())
                            .role(TextRole::Label)
                            .truncate(),
                    )
                    .child(
                        h_stack(Space::S1)
                            .items_center()
                            .when_some(year, |row, year| {
                                row.child(
                                    Text::new(year)
                                        .role(TextRole::Caption)
                                        .tone(TextTone::Muted),
                                )
                            })
                            .when(watched, |row| {
                                row.child(
                                    Icon::new(IconName::Check)
                                        .size(IconSize::Small)
                                        .color(theme.colors.text.muted),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }
}

/// The Library a row's "View all" opens. Recently Added rows are the
/// newest titles of one kind, so they open that kind sorted by date added.
/// Continue Watching, Next Up, and Favorites have no Library equivalent.
pub(crate) fn browse_target(shelf: HomeShelf) -> Option<LibraryKind> {
    match shelf {
        HomeShelf::RecentMovies => Some(LibraryKind::Movies),
        HomeShelf::RecentSeries => Some(LibraryKind::Series),
        _ => None,
    }
}

fn shelf_id(shelf: HomeShelf, part: &'static str) -> SharedString {
    format!("home-{part}-{}", shelf as usize).into()
}

fn pager_buttons(
    shelf: HomeShelf,
    rail: RailState,
    cx: &mut Context<HomeScreen>,
) -> impl IntoElement {
    let back = rail.clone();
    let title = shelf.title();
    let measured = f32::from(rail.scroll().max_offset().width) > 0.0;
    h_stack(Space::S1)
        .child(
            IconButton::new(
                ("home-page-back", shelf as usize),
                IconName::ChevronLeft,
                format!("Scroll {title} back"),
            )
            .variant(ButtonVariant::Subtle)
            .disabled(!rail.can_page_back())
            .on_click(cx.listener(move |_, _, _, cx| {
                back.page(false);
                cx.notify();
            })),
        )
        .child(
            IconButton::new(
                ("home-page-forward", shelf as usize),
                IconName::ChevronRight,
                format!("Scroll {title} forward"),
            )
            .variant(ButtonVariant::Subtle)
            // Before the first measurement the rail is at its start.
            .disabled(measured && !rail.can_page_forward())
            .on_click(cx.listener(move |_, _, _, cx| {
                rail.page(true);
                cx.notify();
            })),
        )
}

fn quiet(theme: &Theme, text: &str) -> impl IntoElement {
    Text::new(text.to_string())
        .role(TextRole::Caption)
        .color(theme.colors.text.muted)
}

/// Calm placeholders in the shape of the cards that will arrive. No shimmer.
fn skeleton_row(theme: &Theme, layout: Layout, shelf: HomeShelf) -> impl IntoElement {
    let (w, h) = if landscape(shelf) {
        layout.landscape
    } else {
        layout.poster
    };
    let count = if landscape(shelf) { 4 } else { 7 };
    h_stack(Space::S4)
        .px(layout.gutter.px())
        .py(Space::S1.px())
        .overflow_hidden()
        .children((0..count).map(|index| {
            v_stack(Space::S2)
                .id((shelf_id(shelf, "skeleton"), index))
                .flex_none()
                .p(Space::S1.px())
                .child(
                    div()
                        .w(px(w))
                        .h(px(h))
                        .rounded(px(theme.radius.get(Radius::Medium)))
                        .bg(theme.colors.surface.panel),
                )
                .child(
                    div()
                        .w(px(w * 0.6))
                        .h(px(10.0))
                        .rounded(px(theme.radius.get(Radius::Small)))
                        .bg(theme.colors.surface.elevated),
                )
        }))
}

fn hero_skeleton(theme: &Theme) -> impl IntoElement {
    let block = |width: f32, height: f32| {
        div()
            .w(px(width))
            .h(px(height))
            .rounded(px(theme.radius.get(Radius::Small)))
            .bg(theme.colors.surface.elevated)
    };
    v_stack(Space::S3)
        .child(block(120.0, 12.0))
        .child(block(380.0, 40.0))
        .child(block(200.0, 12.0))
        .child(
            v_stack(Space::S2)
                .pt(Space::S1.px())
                .child(block(480.0, 12.0))
                .child(block(420.0, 12.0)),
        )
        .child(div().pt(Space::S2.px()).child(block(168.0, 44.0)))
}

fn backdrop_layer(
    theme: &Theme,
    art: Option<&Artwork>,
    width: f32,
    height: f32,
) -> impl IntoElement {
    let canvas = theme.colors.surface.canvas;
    div()
        .absolute()
        .top_0()
        .left_0()
        .w(px(width))
        .h(px(height))
        .when_some(art.and_then(Artwork::image), |layer, image| {
            layer.child(
                Image::decoded("home-backdrop", image.clone())
                    .frame(width, height)
                    .fit(ImageFit::Fill)
                    .radius(Radius::None)
                    .label("Backdrop"),
            )
        })
        // The copy sits bottom left; the rows continue below on the canvas.
        .child(div().absolute().inset_0().bg(linear_gradient(
            90.0,
            linear_color_stop(canvas.with_alpha(0.92), 0.0),
            linear_color_stop(canvas.with_alpha(0.15), 1.0),
        )))
        .child(div().absolute().inset_0().bg(linear_gradient(
            180.0,
            linear_color_stop(canvas.with_alpha(0.35), 0.0),
            linear_color_stop(canvas, 1.0),
        )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artwork::ARTWORK_CACHE_BYTES;
    use crate::home::model::SHELF_LIMIT;
    use crate::home::model::tests::{item, with_backdrop};
    use crate::test_support::FIXTURE_TOKEN;

    #[test]
    fn home_artwork_is_tokenless_and_sized_for_its_slot() {
        let session = crate::model::review_session();
        let urls = session.artwork();
        let movie = with_backdrop(item("movie-1", ItemKind::Movie));
        let mut episode = with_backdrop(item("ep-1", ItemKind::Episode));
        episode.hierarchy.series_id = ItemId::parse("series-1").ok();

        let poster = card_request(HomeShelf::RecentMovies, &movie, &urls).unwrap();
        assert!(poster.url.contains("/Images/Primary"));
        assert!(
            poster
                .url
                .contains(&format!("maxWidth={TILE_POSTER_WIDTH}"))
        );
        let wide = card_request(HomeShelf::ContinueWatching, &movie, &urls).unwrap();
        assert!(wide.url.contains("/Images/Backdrop"));
        assert!(wide.url.contains(&format!("maxWidth={THUMB_WIDTH}")));
        let still = card_request(HomeShelf::NextUp, &episode, &urls).unwrap();
        assert!(still.url.contains("/Items/ep-1/Images/Primary"));
        let hero = backdrop_request(&movie, &urls).unwrap();
        assert!(hero.url.contains(&format!("maxWidth={BACKDROP_WIDTH}")));
        for request in [poster, wide, still, hero] {
            assert!(!request.url.contains("api_key"), "{}", request.url);
            assert!(!request.url.contains(FIXTURE_TOKEN), "{}", request.url);
        }
        assert!(
            card_request(
                HomeShelf::RecentMovies,
                &item("bare", ItemKind::Movie),
                &urls
            )
            .is_none()
        );
    }

    /// A Home with every shelf full of distinct titles that all have art:
    /// the most images Home can ask for at once.
    fn full_home() -> HomeModel {
        let (mut model, requests) = HomeModel::open();
        for request in requests {
            let shelf = request.shelf;
            let items = (0..SHELF_LIMIT)
                .map(|index| {
                    let value = format!("{}-{index}", shelf as usize);
                    let kind = if landscape(shelf) {
                        ItemKind::Episode
                    } else {
                        ItemKind::Movie
                    };
                    let mut item = with_backdrop(item(&value, kind));
                    item.hierarchy.series_id = ItemId::parse(format!("series-{value}")).ok();
                    item
                })
                .collect();
            model.apply(request, Ok(items));
        }
        model
    }

    fn decoded_bytes(planned: &PlannedArt) -> usize {
        let width = planned.width as f32;
        (width * (width * planned.shape.aspect()).round()) as usize * 4
    }

    #[test]
    fn the_artwork_plan_is_derived_from_the_production_requests() {
        let session = crate::model::review_session();
        let urls = session.artwork();
        let plan = artwork_plan(&full_home(), &urls);
        // Every planned width is the width its address actually asks for.
        for planned in &plan {
            assert!(
                planned
                    .request
                    .url
                    .contains(&format!("maxWidth={}", planned.width)),
                "{}",
                planned.request.url
            );
        }
        let count = |shape| plan.iter().filter(|p| p.shape == shape).count();
        let landscape_shelves = HomeShelf::ALL.iter().filter(|s| landscape(**s)).count();
        assert_eq!(count(ArtShape::Backdrop), 1);
        assert_eq!(count(ArtShape::Landscape), landscape_shelves * SHELF_LIMIT);
        assert_eq!(
            count(ArtShape::Poster),
            (HomeShelf::ALL.len() - landscape_shelves) * SHELF_LIMIT
        );
    }

    #[test]
    fn a_full_home_is_a_planning_estimate_well_inside_the_cache() {
        // A representative worst case, computed from the requests Home
        // makes (widths from the production constants, the expected 16:9
        // and 2:3 aspects). Real files keep their own aspect, so this is a
        // planning number, not a ceiling. The ceiling is the cache budget.
        let session = crate::model::review_session();
        let urls = session.artwork();
        let home: usize = artwork_plan(&full_home(), &urls)
            .iter()
            .map(decoded_bytes)
            .sum();
        let mib = home as f32 / (1024.0 * 1024.0);
        assert!(home < ARTWORK_CACHE_BYTES * 55 / 100, "{mib:.1} MiB");
        // Documented as "about 46.5 MiB" in docs/architecture/home.md.
        assert!((44.0..50.0).contains(&mib), "{mib:.1} MiB");
    }

    #[test]
    fn the_same_artwork_in_several_shelves_is_one_request() {
        let session = crate::model::review_session();
        let urls = session.artwork();
        let movie = with_backdrop(item("northwind", ItemKind::Movie));
        let (mut model, requests) = HomeModel::open();
        for request in requests {
            let items = match request.shelf {
                // The hero, a landscape card, and two poster shelves.
                HomeShelf::ContinueWatching | HomeShelf::RecentMovies | HomeShelf::Favorites => {
                    vec![movie.clone()]
                }
                _ => Vec::new(),
            };
            model.apply(request, Ok(items));
        }
        let plan = artwork_plan(&model, &urls);
        let addresses: HashSet<&str> = plan.iter().map(|p| p.request.url.as_str()).collect();
        assert_eq!(addresses.len(), plan.len(), "no address twice");
        // Hero backdrop (1920), card backdrop (480), one poster (360):
        // shared only where the size genuinely matches.
        assert_eq!(plan.len(), 3);
        let shapes: Vec<ArtShape> = plan.iter().map(|p| p.shape).collect();
        assert_eq!(
            shapes,
            vec![ArtShape::Backdrop, ArtShape::Landscape, ArtShape::Poster]
        );
        assert_ne!(
            plan[0].request, plan[1].request,
            "hero keeps its larger image"
        );
    }
}
