//! Native window: startup, Login, the signed-in roots (Home and Library),
//! and the pages above them (Details, the Player).
//!
//! The view asks the service runtime to do vault and HTTP work, then applies
//! the result to [`crate::model::AppModel`]. It does not poll HTTP futures.

use std::sync::Arc;

use atelier_app::{
    ChromeIntent, Platform, WindowSpec, on_fullscreen_escape, open_window, resolve_chrome,
    titlebar_leading, titlebar_spacer,
};
use atelier_ui::gpui::AnyView;
use atelier_ui::prelude::*;
use matinee_jellyfin::{ReqwestTransport, authenticate};
use matinee_secrets::{KeyringStore, MemoryStore};
use matinee_ui::palette::FADED_TEAL;
use tokio::task::JoinHandle;

use crate::artwork::{ArtworkLoader, Client};
use crate::calendar::{CalendarPreview, CalendarScreen, CalendarScreenEvent, CalendarService};
use crate::details::{DetailsEvent, DetailsScreen};
use crate::home::{HomeEvent, HomeScreen};
use crate::library::{LibraryEvent, LibraryScreen};
use crate::nav::{Navigation, Root, RootDestination, StaleRoots};
use crate::player::{KeyOutcome, LeavePlayer, PlayerScreen, PlayerSessionEnded};
use crate::runtime::ServiceRuntime;
use crate::search::{SearchEvent, SearchScreen};
use crate::session::{accept_authentication, forget_session, restore_session};
use crate::store::SharedStore;

use crate::model::{AppModel, LOGIN_COPY, Phase, ReviewScene};

#[derive(Clone)]
pub struct Services {
    runtime: Arc<ServiceRuntime>,
    store: SharedStore,
    /// Radarr and Sonarr, reading the same vault namespace as the shipping app.
    /// The Calendar's only route to them.
    calendar: Arc<CalendarService>,
    /// Orders vault writes that can overlap: removing an ended session and
    /// saving the next sign-in. Whoever takes it first finishes first.
    vault: Arc<tokio::sync::Mutex<()>>,
}

impl Services {
    pub fn production() -> std::io::Result<Self> {
        let store = SharedStore::new(KeyringStore::new());
        Ok(Self {
            runtime: Arc::new(ServiceRuntime::new()?),
            calendar: calendar_service(store.clone())?,
            store,
            vault: Arc::default(),
        })
    }

    pub fn memory() -> std::io::Result<Self> {
        let store = SharedStore::new(MemoryStore::new());
        Ok(Self {
            runtime: Arc::new(ServiceRuntime::new()?),
            calendar: calendar_service(store.clone())?,
            store,
            vault: Arc::default(),
        })
    }

    pub fn runtime(&self) -> Arc<ServiceRuntime> {
        Arc::clone(&self.runtime)
    }
}

fn calendar_service(store: SharedStore) -> std::io::Result<Arc<CalendarService>> {
    let transport = matinee_integrations::ReqwestTransport::new()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(Arc::new(CalendarService::new(store, transport)))
}

pub struct MatineeRoot {
    services: Services,
    model: AppModel,
    focus: FocusHandle,
    task: Option<JoinHandle<()>>,
    focus_username: bool,
    /// After sign-in starts, move focus onto the loading button once the
    /// fields have left the tab order.
    park_focus: bool,
    /// The signed-in root destination. Created once per sign-in and kept
    /// while pages cover it, so its scroll and focus survive.
    home: Option<Entity<HomeScreen>>,
    /// The other root destination, created the first time it is chosen and
    /// kept like Home.
    library: Option<Entity<LibraryScreen>>,
    /// Search, created the first time it is chosen and kept like Library.
    search: Option<Entity<SearchScreen>>,
    /// Calendar, created the first time it is chosen and kept like Search.
    calendar: Option<Entity<CalendarScreen>>,
    /// Which root shows when no page covers it.
    root: RootDestination,
    /// Roots that reload what playback changed when they next show.
    stale: StaleRoots,
    /// Review scenes never create a live Home.
    review: bool,
    /// Screens above the root: Details, then the Player over it.
    pages: Navigation<Page>,
    /// Move focus into the top page on the next frame.
    focus_page: bool,
    /// One Jellyfin client per signed-in session, shared by Home and Details.
    client: Option<Client>,
    artwork: ArtworkLoader,
}

/// A screen above Home.
enum Page {
    Details(Entity<DetailsScreen>),
    Player(Entity<PlayerScreen>),
}

impl MatineeRoot {
    fn new(
        services: Services,
        review: Option<ReviewScene>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let model = review
            .map(AppModel::review)
            .unwrap_or_else(AppModel::starting);
        let artwork = ArtworkLoader::new(Arc::clone(&services.runtime));
        let mut root = Self {
            services,
            focus_username: review == Some(ReviewScene::Focus),
            focus_page: false,
            model,
            focus: cx.focus_handle(),
            task: None,
            park_focus: false,
            home: None,
            library: None,
            search: None,
            calendar: None,
            root: RootDestination::Home,
            stale: StaleRoots::default(),
            review: review.is_some(),
            pages: Navigation::default(),
            client: None,
            artwork,
        };
        if let Some(scene) = review.and_then(ReviewScene::home_preview) {
            let runtime = Arc::clone(&root.services.runtime);
            let loader = root.artwork.clone();
            let home = cx.new(|cx| HomeScreen::preview(runtime, loader, scene, cx));
            root.adopt_home(home, window, cx);
        }
        if let Some(scene) = review.and_then(ReviewScene::library_preview) {
            let runtime = Arc::clone(&root.services.runtime);
            let loader = root.artwork.clone();
            let library = cx.new(|cx| LibraryScreen::preview(runtime, loader, scene, cx));
            root.root = RootDestination::Library(library.read(cx).kind());
            root.adopt_library(library, window, cx);
        }
        if let Some(scene) = review.and_then(ReviewScene::search_preview) {
            let runtime = Arc::clone(&root.services.runtime);
            let loader = root.artwork.clone();
            let search = cx.new(|cx| SearchScreen::preview(runtime, loader, scene, cx));
            root.root = RootDestination::Search;
            root.adopt_search(search, window, cx);
        }
        if let Some(scene) = review.and_then(ReviewScene::calendar_preview) {
            let runtime = Arc::clone(&root.services.runtime);
            let loader = root.artwork.clone();
            let calendar = cx.new(|cx| scene.screen(runtime, loader, cx));
            root.root = RootDestination::Calendar;
            root.adopt_calendar(calendar, window, cx);
        }
        if review.is_none() {
            root.start_restore(cx);
        }
        if let Some(scene) = review.and_then(ReviewScene::details_preview) {
            let runtime = Arc::clone(&root.services.runtime);
            let loader = root.artwork.clone();
            let details = cx.new(|cx| DetailsScreen::preview(runtime, loader, scene, cx));
            root.push_details(details, window, cx);
        }
        if let Some(scene) = review.and_then(ReviewScene::player_preview) {
            let runtime = Arc::clone(&root.services.runtime);
            let player = cx.new(|cx| PlayerScreen::preview(runtime, scene, window, cx));
            root.push_player(player, cx);
        }
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            // Close the Player before the window goes, so its stop report does
            // not depend on drop order. The window still closes.
            weak.update(cx, |root, cx| root.release_pages(cx)).ok();
            true
        });
        cx.on_app_quit(|root, cx| {
            root.prepare_exit(cx);
            async {}
        })
        .detach();
        root
    }

    fn replace_task(&mut self, task: JoinHandle<()>) {
        if let Some(previous) = self.task.replace(task) {
            previous.abort();
        }
    }

    fn start_restore(&mut self, cx: &mut Context<Self>) {
        let store = self.services.store.clone();
        let (task, rx) = self.services.runtime.spawn(async move {
            tokio::task::spawn_blocking(move || restore_session(&store))
                .await
                .unwrap_or(crate::session::Startup::VaultUnavailable)
        });
        self.replace_task(task);
        cx.spawn(async move |this, cx| {
            let startup = rx
                .await
                .unwrap_or(crate::session::Startup::VaultUnavailable);
            this.update(cx, |this, cx| {
                this.task = None;
                this.model.apply_startup(startup);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn start_sign_in(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.model.begin_sign_in() else {
            cx.notify();
            return;
        };
        let store = self.services.store.clone();
        let vault = Arc::clone(&self.services.vault);
        let (server, username, password) = request.into_parts();
        let (task, rx) = self.services.runtime.spawn(async move {
            match ReqwestTransport::new() {
                Ok(transport) => {
                    let result = authenticate(&transport, &server, &username, password).await;
                    // After any removal of an ended session, never before it.
                    let _vault = vault.lock().await;
                    accept_authentication(&store, result)
                }
                Err(error) => Err(error.to_string()),
            }
        });
        self.replace_task(task);
        self.park_focus = true;
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err("The request was cancelled.".into()));
            this.update(cx, |this, cx| {
                this.task = None;
                this.model.apply_sign_in(outcome);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn start_sign_out(&mut self, cx: &mut Context<Self>) {
        if !self.model.begin_sign_out() {
            return;
        }
        self.release_pages(cx);
        if let Some(home) = &self.home {
            home.update(cx, |home, cx| home.set_signing_out(true, cx));
        }
        if let Some(library) = &self.library {
            library.update(cx, |library, cx| library.set_signing_out(true, cx));
        }
        if let Some(search) = &self.search {
            search.update(cx, |search, cx| search.set_signing_out(true, cx));
        }
        if let Some(calendar) = &self.calendar {
            calendar.update(cx, |calendar, cx| calendar.set_signing_out(true, cx));
        }
        let store = self.services.store.clone();
        let (task, rx) = self.services.runtime.spawn(async move {
            tokio::task::spawn_blocking(move || forget_session(&store))
                .await
                .unwrap_or_else(|_| {
                    Err("The credential vault could not complete the request.".into())
                })
        });
        self.replace_task(task);
        cx.spawn(async move |this, cx| {
            let outcome = rx.await.unwrap_or_else(|_| {
                Err("The credential vault could not complete the request.".into())
            });
            this.update(cx, |this, cx| {
                this.task = None;
                this.model.apply_sign_out(outcome);
                if this.model.shows_login() {
                    this.leave_home();
                } else {
                    if let Some(home) = &this.home {
                        home.update(cx, |home, cx| home.set_signing_out(false, cx));
                    }
                    if let Some(library) = &this.library {
                        library.update(cx, |library, cx| library.set_signing_out(false, cx));
                    }
                    if let Some(search) = &this.search {
                        search.update(cx, |search, cx| search.set_signing_out(false, cx));
                    }
                    if let Some(calendar) = &this.calendar {
                        calendar.update(cx, |calendar, cx| calendar.set_signing_out(false, cx));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Jellyfin stopped accepting the session. Home, Details, or the
    /// Player's preparation reported it; this is the one place that acts.
    /// Every page closes (a Player that started sends its stop report,
    /// which may fail), Home and the client go, and Login returns with the
    /// server and username kept. The dead session is removed from the vault
    /// so the next launch does not restore it. Server outages never land
    /// here: only an authorization failure does.
    ///
    /// Idempotent. Several requests can report at once; the first call
    /// moves to Login and the rest find `AppModel::expire_session` false
    /// and do nothing: no second removal, no second transition, no touching
    /// what the person has started typing. Reports from screens that are no
    /// longer current are dropped by the callers (`is_current_*`).
    fn expire_session(&mut self, cx: &mut Context<Self>) {
        if !self.model.expire_session() {
            return;
        }
        self.release_pages(cx);
        self.leave_home();
        let store = self.services.store.clone();
        let vault = Arc::clone(&self.services.vault);
        // Take the vault turn now, on this thread, so a sign-in started
        // afterwards always saves after this removal.
        let turn = Arc::clone(&vault).try_lock_owned();
        // Final work: an orderly exit gives it a bounded chance to finish.
        // It is not the window's in-flight slot, so signing in again does
        // not abort it.
        self.services.runtime.spawn_final(async move {
            let _turn = match turn {
                Ok(turn) => turn,
                Err(_) => vault.lock_owned().await,
            };
            let _ = tokio::task::spawn_blocking(move || forget_session(&store)).await;
        });
        cx.notify();
    }

    fn is_current_home(&self, home: &Entity<HomeScreen>) -> bool {
        self.home.as_ref() == Some(home)
    }

    fn is_current_library(&self, library: &Entity<LibraryScreen>) -> bool {
        self.library.as_ref() == Some(library)
    }

    fn is_current_search(&self, search: &Entity<SearchScreen>) -> bool {
        self.search.as_ref() == Some(search)
    }

    fn is_current_calendar(&self, calendar: &Entity<CalendarScreen>) -> bool {
        self.calendar.as_ref() == Some(calendar)
    }

    fn is_current_details(&self, details: &Entity<DetailsScreen>) -> bool {
        self.pages
            .any(|page| matches!(page, Page::Details(open) if open == details))
    }

    fn is_current_player(&self, player: &Entity<PlayerScreen>) -> bool {
        self.pages
            .any(|page| matches!(page, Page::Player(open) if open == player))
    }

    /// Drop the roots and the session's client. Their in-flight requests
    /// and artwork fetches are aborted with them.
    fn leave_home(&mut self) {
        self.home = None;
        self.library = None;
        self.search = None;
        self.calendar = None;
        self.root = RootDestination::Home;
        self.stale.clear();
        self.client = None;
    }

    /// Create Home for a signed-in session, once.
    fn ensure_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.review || self.home.is_some() || !self.model.shows_home() {
            return;
        }
        let Some(client) = self.client() else {
            if self.model.notice().is_none() {
                self.model.note_unavailable();
            }
            return;
        };
        let runtime = Arc::clone(&self.services.runtime);
        let loader = self.artwork.clone();
        let home = cx.new(|cx| HomeScreen::open(runtime, client, loader, cx));
        self.adopt_home(home, window, cx);
    }

    fn adopt_home(
        &mut self,
        home: Entity<HomeScreen>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            &home,
            window,
            |this, home, event: &HomeEvent, window, cx| match event {
                // Reports from a Home that is no longer the root are stale.
                _ if !this.is_current_home(home) => {}
                HomeEvent::Open(item_id) => this.open_details(item_id.clone(), window, cx),
                HomeEvent::Play(item_id) => this.open_player(item_id.clone(), window, cx),
                HomeEvent::Navigate(destination) => this.navigate(*destination, None, window, cx),
                HomeEvent::Browse(kind, sort) => {
                    this.navigate(RootDestination::Library(*kind), Some(*sort), window, cx)
                }
                HomeEvent::SignOut => this.start_sign_out(cx),
                HomeEvent::SessionExpired => this.expire_session(cx),
            },
        )
        .detach();
        self.home = Some(home);
        self.focus_page = true;
    }

    fn adopt_library(
        &mut self,
        library: Entity<LibraryScreen>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            &library,
            window,
            |this, library, event: &LibraryEvent, window, cx| match event {
                // Reports from a Library that is no longer current are stale.
                _ if !this.is_current_library(library) => {}
                LibraryEvent::Open(item_id) => this.open_details(item_id.clone(), window, cx),
                LibraryEvent::Navigate(destination) => {
                    this.navigate(*destination, None, window, cx)
                }
                LibraryEvent::SignOut => this.start_sign_out(cx),
                LibraryEvent::SessionExpired => this.expire_session(cx),
            },
        )
        .detach();
        self.library = Some(library);
        self.focus_page = true;
    }

    /// Library for a signed-in session, created the first time it is
    /// chosen. Review scenes get the fixture Library, with no socket.
    fn ensure_library(
        &mut self,
        kind: matinee_core::LibraryKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<LibraryScreen>> {
        if self.library.is_none() && self.model.shows_home() {
            let runtime = Arc::clone(&self.services.runtime);
            let loader = self.artwork.clone();
            let library = if self.review {
                cx.new(|cx| {
                    LibraryScreen::preview(
                        runtime,
                        loader,
                        crate::library::LibraryPreview::Movies,
                        cx,
                    )
                })
            } else {
                let client = self.client()?;
                cx.new(|cx| LibraryScreen::open(runtime, client, loader, kind, cx))
            };
            self.adopt_library(library, window, cx);
        }
        self.library.clone()
    }

    fn adopt_search(
        &mut self,
        search: Entity<SearchScreen>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            &search,
            window,
            |this, search, event: &SearchEvent, window, cx| match event {
                // Reports from a Search that is no longer current are stale.
                _ if !this.is_current_search(search) => {}
                SearchEvent::Open(item_id) => this.open_details(item_id.clone(), window, cx),
                SearchEvent::Navigate(destination) => this.navigate(*destination, None, window, cx),
                SearchEvent::SignOut => this.start_sign_out(cx),
                SearchEvent::SessionExpired => this.expire_session(cx),
            },
        )
        .detach();
        self.search = Some(search);
        self.focus_page = true;
    }

    /// Search for a signed-in session, created the first time it is chosen.
    /// Review scenes get the fixture Search, with no socket.
    fn ensure_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<SearchScreen>> {
        if self.search.is_none() && self.model.shows_home() {
            let runtime = Arc::clone(&self.services.runtime);
            let loader = self.artwork.clone();
            let search = if self.review {
                cx.new(|cx| {
                    SearchScreen::preview(
                        runtime,
                        loader,
                        crate::search::SearchPreview::Results,
                        cx,
                    )
                })
            } else {
                let client = self.client()?;
                cx.new(|cx| SearchScreen::open(runtime, client, loader, cx))
            };
            self.adopt_search(search, window, cx);
        }
        self.search.clone()
    }

    fn adopt_calendar(
        &mut self,
        calendar: Entity<CalendarScreen>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            &calendar,
            window,
            |this, calendar, event: &CalendarScreenEvent, window, cx| match event {
                // Reports from a Calendar that is no longer current are stale.
                _ if !this.is_current_calendar(calendar) => {}
                CalendarScreenEvent::Navigate(destination) => {
                    this.navigate(*destination, None, window, cx)
                }
                CalendarScreenEvent::SignOut => this.start_sign_out(cx),
            },
        )
        .detach();
        self.calendar = Some(calendar);
        self.focus_page = true;
    }

    /// Calendar for a signed-in session, created the first time it is chosen.
    /// It reads the integrations vault, not the Jellyfin session, so a
    /// review scene gets a fixture with no socket.
    fn ensure_calendar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<CalendarScreen>> {
        if self.calendar.is_none() && self.model.shows_home() {
            let runtime = Arc::clone(&self.services.runtime);
            let loader = self.artwork.clone();
            let calendar = if self.review {
                cx.new(|cx| CalendarPreview::Populated.screen(runtime, loader, cx))
            } else {
                let service = Arc::clone(&self.services.calendar);
                let name = self
                    .model
                    .session()
                    .map(|session| session.user().name().to_string())
                    .unwrap_or_default();
                cx.new(|cx| CalendarScreen::open(runtime, service, loader, name, cx))
            };
            self.adopt_calendar(calendar, window, cx);
        }
        self.calendar.clone()
    }

    /// Move between root destinations from the app bar or Home's "View
    /// all". Pages are not touched (the bar is only on roots). Each root
    /// keeps its state; a root that missed playback reloads what it changed.
    fn navigate(
        &mut self,
        destination: RootDestination,
        sort: Option<matinee_core::LibrarySort>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.pages.is_empty() || !self.model.shows_home() {
            return;
        }
        match destination {
            RootDestination::Home => {
                let Some(home) = self.home.clone() else {
                    return;
                };
                self.root = destination;
                let refresh = self.stale.take(Root::Home);
                home.update(cx, |home, cx| home.resume(refresh, cx));
            }
            RootDestination::Library(kind) => {
                let Some(library) = self.ensure_library(kind, window, cx) else {
                    return;
                };
                self.root = destination;
                let playback = self.stale.take(Root::Library);
                library.update(cx, |library, cx| {
                    library.show(kind, sort, cx);
                    library.resume(playback, cx);
                });
            }
            RootDestination::Search => {
                let Some(search) = self.ensure_search(window, cx) else {
                    return;
                };
                self.root = destination;
                let playback = self.stale.take(Root::Search);
                search.update(cx, |search, cx| search.show(playback, cx));
            }
            RootDestination::Calendar => {
                let Some(calendar) = self.ensure_calendar(window, cx) else {
                    return;
                };
                self.root = destination;
                calendar.update(cx, |calendar, cx| calendar.show(cx));
            }
        }
        self.focus_page = true;
        cx.notify();
    }

    /// The current root is showing again. It refreshes only what playback
    /// above it may have changed.
    fn root_visible(&mut self, cx: &mut Context<Self>) {
        if self.pages.take_root_stale() {
            self.stale.mark_all();
        }
        match self.root.root() {
            Root::Home => {
                let refresh = self.stale.take(Root::Home);
                if let Some(home) = &self.home {
                    home.update(cx, |home, cx| home.resume(refresh, cx));
                }
            }
            Root::Library => {
                let playback = self.stale.take(Root::Library);
                if let Some(library) = &self.library {
                    library.update(cx, |library, cx| library.resume(playback, cx));
                }
            }
            Root::Search => {
                let playback = self.stale.take(Root::Search);
                if let Some(search) = &self.search {
                    search.update(cx, |search, cx| search.resume(playback, cx));
                }
            }
            // No page opens above Calendar, so there is nothing to resume.
            Root::Calendar => {}
        }
    }

    /// The shared client for the signed-in session. Review scenes have
    /// none, so nothing in them opens a socket.
    fn client(&mut self) -> Option<Client> {
        if self.review {
            return None;
        }
        if self.client.is_none() {
            let session = self.model.session()?.clone();
            let transport = matinee_jellyfin::ReqwestTransport::new().ok()?;
            self.client = Some(Arc::new(matinee_jellyfin::JellyfinClient::new(
                session, transport,
            )));
        }
        self.client.clone()
    }

    /// Home → Details for one item.
    fn open_details(
        &mut self,
        item_id: matinee_core::ItemId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client() else {
            return;
        };
        let runtime = Arc::clone(&self.services.runtime);
        let loader = self.artwork.clone();
        let details = cx.new(|cx| DetailsScreen::open(runtime, client, loader, item_id, cx));
        self.push_details(details, window, cx);
    }

    fn push_details(
        &mut self,
        details: Entity<DetailsScreen>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            &details,
            window,
            |this, details, event: &DetailsEvent, window, cx| match event {
                DetailsEvent::Play(item_id) => this.open_player(item_id.clone(), window, cx),
                DetailsEvent::Back => this.close_details(details, cx),
                DetailsEvent::SessionExpired => {
                    if this.is_current_details(details) {
                        this.expire_session(cx);
                    }
                }
            },
        )
        .detach();
        self.pages.push(Page::Details(details));
        self.focus_page = true;
        cx.notify();
    }

    fn close_details(&mut self, details: &Entity<DetailsScreen>, cx: &mut Context<Self>) {
        let closed = self
            .pages
            .pop_if(|page| matches!(page, Page::Details(top) if top == details));
        if closed.is_some() {
            if self.pages.is_empty() {
                self.root_visible(cx);
            }
            self.focus_page = true;
            cx.notify();
        }
    }

    /// Details or Home → Player. The Player plans, resumes, and reports on
    /// its own. Home is marked for a refresh, since progress will change.
    fn open_player(
        &mut self,
        item_id: matinee_core::ItemId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.model.session().cloned().filter(|_| !self.review) else {
            return;
        };
        let runtime = Arc::clone(&self.services.runtime);
        let player = cx.new(|cx| PlayerScreen::open(runtime, session, item_id, window, cx));
        self.pages.mark_root_stale();
        self.push_player(player, cx);
    }

    fn push_player(&mut self, player: Entity<PlayerScreen>, cx: &mut Context<Self>) {
        cx.subscribe(&player, |this, _, _: &LeavePlayer, cx| {
            this.release_player(cx);
            cx.notify();
        })
        .detach();
        cx.subscribe(&player, |this, player, _: &PlayerSessionEnded, cx| {
            if this.is_current_player(&player) {
                this.expire_session(cx);
            }
        })
        .detach();
        self.pages.push(Page::Player(player));
        self.focus_page = true;
        cx.notify();
    }

    /// Player → Details (or Home). The final stop report is started here
    /// and not awaited: the service runtime stays alive. Only
    /// [`Self::prepare_exit`] waits for it. Details underneath refreshes so
    /// the new position shows without reopening it.
    fn release_player(&mut self, cx: &mut Context<Self>) {
        let Some(Page::Player(player)) = self.pages.pop_if(|page| matches!(page, Page::Player(_)))
        else {
            return;
        };
        player.update(cx, |player, _| player.finish());
        if let Some(Page::Details(details)) = self.pages.top() {
            details.update(cx, |details, cx| details.resume(cx));
        } else if self.pages.is_empty() {
            self.root_visible(cx);
        }
        self.focus_page = true;
    }

    /// Window close, sign-out, and exit: finish every page, top first.
    fn release_pages(&mut self, cx: &mut Context<Self>) {
        let pages: Vec<Page> = self.pages.drain().collect();
        for page in pages {
            if let Page::Player(player) = page {
                player.update(cx, |player, _| player.finish());
            }
        }
    }

    /// Orderly application exit. GPUI calls this from its quit handlers,
    /// before it drops windows. The Player is finished explicitly, then the
    /// GPUI thread waits for final reports, bounded by
    /// [`crate::runtime::FINAL_WORK_BOUND`]. Exit continues either way.
    fn prepare_exit(&mut self, cx: &mut Context<Self>) {
        self.release_pages(cx);
        self.services.runtime.drain_final();
    }
}

impl Drop for MatineeRoot {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Render for MatineeRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_home(window, cx);
        if self.focus_username {
            self.focus_username = false;
            window.on_next_frame(|window, _| {
                window.focus_next();
                window.focus_next();
            });
        }
        if self.focus_page {
            self.focus_page = false;
            let focus = match self.pages.top() {
                Some(Page::Player(player)) if !player.read(cx).menu_open() => {
                    Some(player.read(cx).focus_handle().clone())
                }
                Some(Page::Player(_)) => None,
                Some(Page::Details(details)) => Some(details.read(cx).focus_handle().clone()),
                // Home and Library put focus on a card, the hero, or the
                // grid themselves, a frame later.
                None if self.model.shows_home() && self.root_slot().is_some() => None,
                None => Some(self.focus.clone()),
            };
            if let Some(focus) = focus {
                window.on_next_frame(move |window, _| window.focus(&focus));
            }
        }

        let theme = cx.theme().clone();
        let chrome = resolve_chrome(Platform::current(), ChromeIntent::PlatformDefault);
        let signing_in = self.model.fields_locked();
        let player = match self.pages.top() {
            Some(Page::Player(player)) => Some(player.clone()),
            _ => None,
        };
        let details = match self.pages.top() {
            Some(Page::Details(details)) => Some(details.clone()),
            _ => None,
        };
        let root_view = match (self.pages.is_empty(), self.model.shows_home()) {
            (true, true) => self.root_slot(),
            _ => None,
        };
        let playing = player.is_some();
        let fullscreen = window.is_fullscreen();
        if signing_in && self.park_focus {
            self.park_focus = false;
            // This frame's tab order no longer includes the fields. A focus
            // id that just disappeared resolves to the first stop (the
            // window), and the next stop is the loading button.
            window.on_next_frame(|window, _| {
                window.focus_next();
                window.focus_next();
            });
        }

        div()
            .id("matinee-root")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.colors.surface.canvas)
            .font_family(theme.typography.families.interface)
            .text_color(theme.colors.text.primary)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event, window, cx| {
                let player = match this.pages.top() {
                    Some(Page::Player(player)) => Some(player.clone()),
                    _ => None,
                };
                if let Some(player) = player {
                    let outcome = player.update(cx, |player, cx| player.on_key(event, window, cx));
                    if matches!(outcome, KeyOutcome::Leave) {
                        this.release_player(cx);
                        cx.notify();
                    }
                    return;
                }
                if plain_enter(event) && this.model.shows_login() {
                    this.start_sign_in(cx);
                }
                on_fullscreen_escape(event, window, cx);
            }))
            .when(self.pages.is_empty() && root_view.is_none(), |root| {
                root.child(atmosphere(&theme))
            })
            .child(
                v_stack(Space::S0)
                    .size_full()
                    .when(
                        chrome.band_height > 0.0 && !(playing && fullscreen),
                        |column| column.child(titlebar(&theme, chrome)),
                    )
                    .child(if let Some(player) = player {
                        div()
                            .id("matinee-player-slot")
                            .flex_1()
                            .w_full()
                            .min_h(px(0.0))
                            .overflow_hidden()
                            .child(player)
                            .into_any_element()
                    } else if let Some(root_view) = root_view {
                        div()
                            .id("matinee-root-slot")
                            .flex_1()
                            .w_full()
                            .min_h(px(0.0))
                            .overflow_hidden()
                            .child(root_view)
                            .into_any_element()
                    } else if let Some(details) = details {
                        div()
                            .id("matinee-details-slot")
                            .flex_1()
                            .w_full()
                            .min_h(px(0.0))
                            .overflow_hidden()
                            .child(details)
                            .into_any_element()
                    } else {
                        ScrollView::vertical("matinee-scroll")
                            .flex_1()
                            .w_full()
                            .min_h(px(0.0))
                            .child(self.body(window, chrome.band_height, signing_in, cx))
                            .into_any_element()
                    }),
            )
    }
}

impl MatineeRoot {
    /// The current root screen, if it exists.
    fn root_slot(&self) -> Option<AnyView> {
        match self.root.root() {
            Root::Home => self.home.clone().map(AnyView::from),
            Root::Library => self.library.clone().map(AnyView::from),
            Root::Search => self.search.clone().map(AnyView::from),
            Root::Calendar => self.calendar.clone().map(AnyView::from),
        }
    }

    fn body(
        &mut self,
        window: &mut Window,
        band: f32,
        signing_in: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let viewport = window.viewport_size();
        let width = f32::from(viewport.width);
        let height = (f32::from(viewport.height) - band).max(0.0);
        let card = card_width(width);

        div()
            .w_full()
            .min_h(px(height))
            .flex()
            .items_center()
            .justify_center()
            .p(Space::S6.px())
            .child(if self.model.phase() == Phase::Starting {
                starting_mark().into_any_element()
            } else if self.model.shows_home() {
                self.shell(cx).into_any_element()
            } else {
                self.login(card, signing_in, cx).into_any_element()
            })
    }

    fn login(&mut self, card: f32, signing_in: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let server = self.model.server().to_string();
        let username = self.model.username().to_string();
        let password = self.model.password().to_string();
        let warning = self.model.http_warning();
        let notice = self.model.notice().map(str::to_string);
        let label = self.model.button_label();
        let entity = cx.entity();

        Surface::new(SurfaceLevel::Elevated)
            .padding(Space::S6)
            .w(px(card))
            .child(
                v_stack(Space::S4)
                    .w_full()
                    .child(
                        Text::new("Your library, reimagined")
                            .role(TextRole::Caption)
                            .color(theme.colors.control.accent),
                    )
                    .child(Text::new("Movie night starts here.").role(TextRole::Title))
                    .child(
                        Text::new(LOGIN_COPY)
                            .role(TextRole::Body)
                            .tone(TextTone::Muted),
                    )
                    .child(
                        v_stack(Space::S3)
                            .w_full()
                            .child(
                                TextField::new("server", server)
                                    .fill()
                                    .disabled(signing_in)
                                    .label("Jellyfin server")
                                    .placeholder("http://jellyfin.local:8096")
                                    .on_change({
                                        let entity = entity.clone();
                                        move |value, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.model.set_server(value.to_string());
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .when_some(warning, |stack, warning| {
                                stack.child(
                                    Text::new(warning)
                                        .role(TextRole::Caption)
                                        .color(theme.colors.control.accent),
                                )
                            })
                            .child(
                                TextField::new("username", username)
                                    .fill()
                                    .disabled(signing_in)
                                    .label("Username")
                                    .on_change({
                                        let entity = entity.clone();
                                        move |value, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.model.set_username(value.to_string());
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .child(
                                TextField::new("password", password)
                                    .fill()
                                    .masked(true)
                                    .disabled(signing_in)
                                    .label("Password")
                                    .on_change({
                                        let entity = entity.clone();
                                        move |value, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.model.set_password(value.to_string());
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .when_some(notice, |stack, notice| {
                                stack.child(
                                    Text::new(notice)
                                        .role(TextRole::Caption)
                                        .color(theme.colors.text.danger),
                                )
                            })
                            .child(
                                Button::new("enter-matinee", label)
                                    .variant(ButtonVariant::Primary)
                                    .size(ButtonSize::Large)
                                    .fill()
                                    .loading(signing_in)
                                    .show_label_while_loading(true)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.start_sign_in(cx);
                                    })),
                            ),
                    ),
            )
    }

    /// Signed in, but Home could not start (no HTTP client). Says so and
    /// offers Sign out. Normally Home replaces this at once.
    fn shell(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let notice = self.model.notice().map(str::to_string);
        let signing_out = self.model.phase() == Phase::SigningOut;
        let label = self.model.button_label();

        Surface::new(SurfaceLevel::Elevated)
            .padding(Space::S8)
            .w(px(420.0))
            .child(
                v_stack(Space::S4)
                    .w_full()
                    .child(Text::new("Matinee").role(TextRole::Title))
                    .when_some(notice, |stack, notice| {
                        stack.child(
                            Text::new(notice)
                                .role(TextRole::Body)
                                .color(theme.colors.text.danger),
                        )
                    })
                    .child(
                        Button::new("sign-out", label)
                            .variant(ButtonVariant::Secondary)
                            .loading(signing_out)
                            .show_label_while_loading(true)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.start_sign_out(cx);
                            })),
                    ),
            )
    }
}

fn starting_mark() -> impl IntoElement {
    v_stack(Space::S2).child(Text::new("Matinee").role(TextRole::Title))
}

fn atmosphere(theme: &Theme) -> impl IntoElement {
    let full = theme.radius.get(Radius::Full);
    div()
        .absolute()
        .inset_0()
        .child(
            div()
                .absolute()
                .top(px(-180.0))
                .right(px(-140.0))
                .size(px(520.0))
                .rounded(px(full))
                .bg(theme.colors.control.accent.with_alpha(0.14)),
        )
        .child(
            div()
                .absolute()
                .bottom(px(-160.0))
                .left(px(-120.0))
                .size(px(460.0))
                .rounded(px(full))
                .bg(FADED_TEAL.with_alpha(0.18)),
        )
        .child(
            div()
                .absolute()
                .bottom(px(48.0))
                .right(px(72.0))
                .size(px(240.0))
                .rounded(px(full))
                .bg(theme.colors.control.destructive.with_alpha(0.12)),
        )
}

fn titlebar(theme: &Theme, chrome: atelier_app::ResolvedChrome) -> impl IntoElement {
    let row_height = chrome.band_height;
    h_stack(Space::S3)
        .w_full()
        .h(px(row_height))
        .flex_none()
        .items_center()
        .px(Space::S4.px())
        .when(chrome.leading_inset > 0.0, |row| {
            row.child(titlebar_leading(chrome.leading_inset, chrome))
        })
        .child(Text::new("Matinee").role(TextRole::Heading))
        .child(titlebar_spacer(chrome))
        .child(div().flex_1())
        .text_color(theme.colors.text.primary)
}

fn card_width(viewport_width: f32) -> f32 {
    (viewport_width - 64.0).clamp(280.0, 420.0)
}

fn plain_enter(event: &atelier_ui::gpui::KeyDownEvent) -> bool {
    let key = &event.keystroke;
    key.key == "enter"
        && !key.modifiers.shift
        && !key.modifiers.control
        && !key.modifiers.alt
        && !key.modifiers.platform
}

pub fn open_matinee(
    cx: &mut App,
    services: Services,
    review: Option<ReviewScene>,
    size: (f32, f32),
) {
    open_window(
        cx,
        WindowSpec::new("Matinee", size)
            .min_size((960.0, 620.0))
            .restoration_key("matinee"),
        move |window, cx| {
            cx.new(|cx| {
                let root = MatineeRoot::new(services, review, window, cx);
                window.focus(&root.focus);
                root
            })
        },
    )
    .expect("failed to open window");
}

#[cfg(test)]
mod tests {
    //! Home's scroll-owning objects across pages, on the headless GPUI test
    //! platform. Pixel layout is GPUI's; what is asserted here is that the
    //! very objects holding the offsets survive, with their offsets.

    use atelier_ui::gpui::{self, TestAppContext, VisualTestContext, point};

    use super::*;
    use crate::details::DetailsPreview;
    use crate::player::PlayerPreview;
    use matinee_core::{LibraryKind, LibrarySort};

    fn open(cx: &mut TestAppContext) -> (Entity<MatineeRoot>, &mut VisualTestContext) {
        let services = Services::memory().unwrap();
        let (root, cx) = cx.add_window_view(move |window, cx| {
            MatineeRoot::new(services, Some(ReviewScene::Home), window, cx)
        });
        cx.run_until_parked();
        (root, cx)
    }

    fn home_of(root: &Entity<MatineeRoot>, cx: &mut VisualTestContext) -> Entity<HomeScreen> {
        root.read_with(cx, |root, _| root.home.clone().expect("Home is the root"))
    }

    fn push_details(
        root: &Entity<MatineeRoot>,
        cx: &mut VisualTestContext,
    ) -> Entity<DetailsScreen> {
        let details = root.update_in(cx, |root, window, cx| {
            let runtime = Arc::clone(&root.services.runtime);
            let loader = root.artwork.clone();
            let details =
                cx.new(|cx| DetailsScreen::preview(runtime, loader, DetailsPreview::Movie, cx));
            root.push_details(details.clone(), window, cx);
            details
        });
        cx.run_until_parked();
        details
    }

    /// Scroll Home's page and every row, and return what was set.
    fn scroll_home(home: &Entity<HomeScreen>, cx: &mut VisualTestContext) -> (f32, Vec<f32>) {
        let (page, rails) = home.read_with(cx, |home, _| home.scroll_state());
        assert!(!rails.is_empty(), "rows were drawn");
        page.set_offset(point(px(0.0), px(-150.0)));
        let page_y = f32::from(page.offset().y);
        assert!(page_y < 0.0, "the page has room to scroll");
        let rows = rails
            .iter()
            .map(|(_, rail)| {
                rail.scroll().set_offset(point(px(-120.0), px(0.0)));
                f32::from(rail.scroll().offset().x)
            })
            .collect();
        cx.run_until_parked();
        (page_y, rows)
    }

    fn assert_kept(
        home: &Entity<HomeScreen>,
        cx: &mut VisualTestContext,
        before: &(f32, Vec<f32>),
    ) {
        let (page, rails) = home.read_with(cx, |home, _| home.scroll_state());
        assert_eq!(f32::from(page.offset().y), before.0, "page offset kept");
        let rows: Vec<f32> = rails
            .iter()
            .map(|(_, rail)| f32::from(rail.scroll().offset().x))
            .collect();
        assert_eq!(rows, before.1, "row offsets kept");
    }

    #[gpui::test]
    fn home_and_its_scroll_state_survive_details(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        let home = home_of(&root, cx);
        let (page, rails) = home.read_with(cx, |home, _| home.scroll_state());
        let before = scroll_home(&home, cx);

        let details = push_details(&root, cx);
        root.update(cx, |root, cx| root.close_details(&details, cx));
        cx.run_until_parked();

        assert_eq!(home_of(&root, cx), home, "the same Home entity");
        assert_kept(&home, cx, &before);
        // The handles held before are the live ones: moving them moves Home.
        page.set_offset(point(px(0.0), px(-60.0)));
        let (live, live_rails) = home.read_with(cx, |home, _| home.scroll_state());
        assert_eq!(f32::from(live.offset().y), -60.0);
        assert_eq!(live_rails.len(), rails.len());
    }

    #[gpui::test]
    fn home_and_its_scroll_state_survive_details_and_the_player(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        let home = home_of(&root, cx);
        let before = scroll_home(&home, cx);

        let details = push_details(&root, cx);
        root.update_in(cx, |root, window, cx| {
            let runtime = Arc::clone(&root.services.runtime);
            let player =
                cx.new(|cx| PlayerScreen::preview(runtime, PlayerPreview::Paused, window, cx));
            root.pages.mark_root_stale();
            root.push_player(player, cx);
        });
        cx.run_until_parked();
        root.update(cx, |root, cx| root.release_player(cx));
        cx.run_until_parked();
        root.update(cx, |root, cx| root.close_details(&details, cx));
        cx.run_until_parked();

        assert_eq!(home_of(&root, cx), home, "the same Home entity");
        assert!(root.read_with(cx, |root, _| root.pages.is_empty()));
        assert_kept(&home, cx, &before);
    }

    #[gpui::test]
    fn a_session_end_from_a_closed_page_is_ignored(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        let details = push_details(&root, cx);
        root.update(cx, |root, cx| root.close_details(&details, cx));
        cx.run_until_parked();
        // The closed Details reports late: it is no longer current.
        details.update(cx, |_, cx| cx.emit(DetailsEvent::SessionExpired));
        cx.run_until_parked();
        assert!(root.read_with(cx, |root, _| root.model.shows_home()));
        // The current Home reports twice: one transition, then nothing.
        let home = home_of(&root, cx);
        home.update(cx, |_, cx| {
            cx.emit(HomeEvent::SessionExpired);
            cx.emit(HomeEvent::SessionExpired);
        });
        cx.run_until_parked();
        root.read_with(cx, |root, _| {
            assert!(root.model.shows_login());
            assert!(root.home.is_none());
            assert!(root.pages.is_empty());
            assert_eq!(root.model.notice(), Some(crate::model::SESSION_ENDED));
        });
        // The old Home reporting again changes nothing.
        home.update(cx, |_, cx| cx.emit(HomeEvent::SessionExpired));
        cx.run_until_parked();
        assert!(root.read_with(cx, |root, _| root.model.shows_login()));
    }

    fn library_of(root: &Entity<MatineeRoot>, cx: &mut VisualTestContext) -> Entity<LibraryScreen> {
        root.read_with(cx, |root, _| root.library.clone().expect("Library exists"))
    }

    fn go(root: &Entity<MatineeRoot>, destination: RootDestination, cx: &mut VisualTestContext) {
        root.update_in(cx, |root, window, cx| {
            root.navigate(destination, None, window, cx)
        });
        cx.run_until_parked();
    }

    /// Scroll the Movies grid, focus a card, and return both.
    fn scroll_library(library: &Entity<LibraryScreen>, cx: &mut VisualTestContext) -> (f32, usize) {
        let grid = library.read_with(cx, |library, _| library.grid(LibraryKind::Movies));
        grid.scroll().set_offset(point(px(0.0), px(-600.0)));
        grid.focus_index(Some(23));
        library.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        let offset = f32::from(grid.scroll().offset().y);
        assert!(offset < -100.0, "the grid scrolled: {offset}");
        (offset, grid.focused().unwrap())
    }

    fn assert_library_kept(
        library: &Entity<LibraryScreen>,
        cx: &mut VisualTestContext,
        before: (f32, usize),
        items: usize,
    ) {
        let grid = library.read_with(cx, |library, _| library.grid(LibraryKind::Movies));
        assert_eq!(f32::from(grid.scroll().offset().y), before.0, "scroll kept");
        assert_eq!(grid.focused(), Some(before.1), "focused card kept");
        library.read_with(cx, |library, _| {
            assert_eq!(library.kind(), LibraryKind::Movies);
            assert_eq!(library.model.items().len(), items, "no reload");
        });
    }

    #[gpui::test]
    fn home_and_library_switch_without_losing_either(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        let home = home_of(&root, cx);
        let before_home = scroll_home(&home, cx);

        home.update(cx, |_, cx| {
            cx.emit(HomeEvent::Navigate(RootDestination::Library(
                LibraryKind::Movies,
            )))
        });
        cx.run_until_parked();
        let library = library_of(&root, cx);
        root.read_with(cx, |root, _| {
            assert_eq!(root.root, RootDestination::Library(LibraryKind::Movies));
            assert!(root.pages.is_empty(), "a root, not a page");
        });
        let before = scroll_library(&library, cx);
        let items = library.read_with(cx, |library, _| library.model.items().len());

        library.update(cx, |_, cx| {
            cx.emit(LibraryEvent::Navigate(RootDestination::Home))
        });
        cx.run_until_parked();
        assert_eq!(
            root.read_with(cx, |root, _| root.root),
            RootDestination::Home
        );
        assert_eq!(home_of(&root, cx), home, "the same Home");
        assert_kept(&home, cx, &before_home);

        go(&root, RootDestination::Library(LibraryKind::Movies), cx);
        assert_eq!(library_of(&root, cx), library, "the same Library");
        assert_library_kept(&library, cx, before, items);
    }

    #[gpui::test]
    fn home_rows_open_the_matching_library(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        let home = home_of(&root, cx);
        home.update(cx, |_, cx| {
            cx.emit(HomeEvent::Browse(
                LibraryKind::Series,
                LibrarySort::DateCreated,
            ))
        });
        cx.run_until_parked();
        let library = library_of(&root, cx);
        assert_eq!(
            root.read_with(cx, |root, _| root.root),
            RootDestination::Library(LibraryKind::Series)
        );
        library.read_with(cx, |library, _| {
            assert_eq!(library.kind(), LibraryKind::Series);
            assert_eq!(library.model.query().sort, LibrarySort::DateCreated);
        });
        // "View all" for movies: the same Library, now on Movies, newest first.
        go(&root, RootDestination::Home, cx);
        home.update(cx, |_, cx| {
            cx.emit(HomeEvent::Browse(
                LibraryKind::Movies,
                LibrarySort::DateCreated,
            ))
        });
        cx.run_until_parked();
        assert_eq!(library_of(&root, cx), library);
        library.read_with(cx, |library, _| {
            assert_eq!(library.kind(), LibraryKind::Movies);
            assert_eq!(library.model.query().sort, LibrarySort::DateCreated);
            assert!(!library.model.items().is_empty(), "loaded");
        });
        // Only the rows with a Library equivalent offer "View all".
        assert_eq!(
            crate::home::browse_target(matinee_core::HomeShelf::RecentMovies),
            Some(LibraryKind::Movies)
        );
        assert_eq!(
            crate::home::browse_target(matinee_core::HomeShelf::RecentSeries),
            Some(LibraryKind::Series)
        );
        assert_eq!(
            crate::home::browse_target(matinee_core::HomeShelf::Favorites),
            None
        );
    }

    #[gpui::test]
    fn library_survives_details_and_the_player(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        go(&root, RootDestination::Library(LibraryKind::Movies), cx);
        let library = library_of(&root, cx);
        let before = scroll_library(&library, cx);
        let items = library.read_with(cx, |library, _| library.model.items().len());

        // Library → Details → Library.
        let details = push_details(&root, cx);
        root.update(cx, |root, cx| root.close_details(&details, cx));
        cx.run_until_parked();
        assert_eq!(library_of(&root, cx), library);
        assert_eq!(
            root.read_with(cx, |root, _| root.root),
            RootDestination::Library(LibraryKind::Movies),
            "Back returns to Library, not Home"
        );
        assert_library_kept(&library, cx, before, items);

        // Library → Details → Player → Details → Library.
        library.update(cx, |library, _| {
            let id = library.model.items()[before.1].id().clone();
            library.model.note_opened(id);
        });
        let details = push_details(&root, cx);
        root.update_in(cx, |root, window, cx| {
            let runtime = Arc::clone(&root.services.runtime);
            let player =
                cx.new(|cx| PlayerScreen::preview(runtime, PlayerPreview::Paused, window, cx));
            root.pages.mark_root_stale();
            root.push_player(player, cx);
        });
        cx.run_until_parked();
        root.update(cx, |root, cx| root.release_player(cx));
        cx.run_until_parked();
        root.update(cx, |root, cx| root.close_details(&details, cx));
        cx.run_until_parked();
        assert_eq!(library_of(&root, cx), library);
        assert!(root.read_with(cx, |root, _| root.pages.is_empty()));
        assert_library_kept(&library, cx, before, items);
        // Library took its playback mark (one title reconciled); Home keeps
        // its own until it shows.
        root.update(cx, |root, _| {
            assert!(!root.stale.take(Root::Library));
            assert!(root.stale.take(Root::Home));
        });
    }

    #[gpui::test]
    fn a_session_end_from_library_signs_out_once(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        go(&root, RootDestination::Library(LibraryKind::Movies), cx);
        let library = library_of(&root, cx);
        library.update(cx, |_, cx| {
            cx.emit(LibraryEvent::SessionExpired);
            cx.emit(LibraryEvent::SessionExpired);
        });
        cx.run_until_parked();
        root.read_with(cx, |root, _| {
            assert!(root.model.shows_login());
            assert!(root.library.is_none());
            assert!(root.home.is_none());
            assert_eq!(root.root, RootDestination::Home);
            assert_eq!(root.model.notice(), Some(crate::model::SESSION_ENDED));
        });
        // The old Library reporting again changes nothing.
        library.update(cx, |_, cx| cx.emit(LibraryEvent::SessionExpired));
        cx.run_until_parked();
        assert!(root.read_with(cx, |root, _| root.model.shows_login()));
    }

    fn search_of(root: &Entity<MatineeRoot>, cx: &mut VisualTestContext) -> Entity<SearchScreen> {
        root.read_with(cx, |root, _| root.search.clone().expect("Search exists"))
    }

    /// Scroll Search's results, put the logical focus on a title, and return
    /// both with the text and the number of titles held.
    fn scroll_search(
        search: &Entity<SearchScreen>,
        cx: &mut VisualTestContext,
    ) -> (f32, usize, String, usize) {
        let grid = search.read_with(cx, |search, _| search.grid());
        grid.scroll().set_offset(point(px(0.0), px(-500.0)));
        grid.focus_index(Some(17));
        search.update(cx, |search, cx| {
            let id = search.model.items()[17].id().clone();
            search.model.note_focused(id);
            cx.notify();
        });
        cx.run_until_parked();
        let offset = f32::from(grid.scroll().offset().y);
        assert!(offset < -100.0, "the results scrolled: {offset}");
        search.read_with(cx, |search, _| {
            (
                offset,
                grid.focused().unwrap(),
                search.model.input().to_string(),
                search.model.items().len(),
            )
        })
    }

    fn assert_search_kept(
        search: &Entity<SearchScreen>,
        cx: &mut VisualTestContext,
        before: &(f32, usize, String, usize),
    ) {
        let grid = search.read_with(cx, |search, _| search.grid());
        assert_eq!(f32::from(grid.scroll().offset().y), before.0, "scroll kept");
        assert_eq!(grid.focused(), Some(before.1), "focused title kept");
        search.read_with(cx, |search, _| {
            assert_eq!(search.model.input(), before.2, "the text kept");
            assert_eq!(
                search
                    .model
                    .effective()
                    .map(matinee_core::SearchQuery::term),
                Some(before.2.as_str()),
                "the query kept"
            );
            assert_eq!(search.model.items().len(), before.3, "the pages kept");
            assert_eq!(search.model.focused_index(), Some(before.1));
        });
    }

    fn focused(handle: &FocusHandle, cx: &mut VisualTestContext) -> bool {
        cx.update(|window, _| handle.is_focused(window))
    }

    #[gpui::test]
    fn search_is_a_retained_root_beside_home_and_library(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        // Home → Search, from Home's app bar.
        let home = home_of(&root, cx);
        home.update(cx, |_, cx| {
            cx.emit(HomeEvent::Navigate(RootDestination::Search))
        });
        cx.run_until_parked();
        let search = search_of(&root, cx);
        root.read_with(cx, |root, _| {
            assert_eq!(root.root, RootDestination::Search);
            assert!(root.pages.is_empty(), "a root, not a page");
        });
        let field = search.read_with(cx, |search, _| search.field_focus());
        assert!(focused(&field, cx), "Search opens with the field focused");
        let requests = search.update(cx, |search, _| search.record_requests());
        let before = scroll_search(&search, cx);

        // Search → Home → Search.
        go(&root, RootDestination::Home, cx);
        assert_eq!(home_of(&root, cx), home);
        go(&root, RootDestination::Search, cx);
        assert_eq!(search_of(&root, cx), search, "the same Search");
        assert_search_kept(&search, cx, &before);
        assert!(focused(&field, cx), "chosen from the bar: the field");

        // Search → Library → Search.
        go(&root, RootDestination::Library(LibraryKind::Movies), cx);
        let library = library_of(&root, cx);
        go(&root, RootDestination::Search, cx);
        assert_eq!(search_of(&root, cx), search);
        assert_search_kept(&search, cx, &before);
        // Library → Search left Library as it was, too.
        go(&root, RootDestination::Library(LibraryKind::Movies), cx);
        assert_eq!(library_of(&root, cx), library);
        go(&root, RootDestination::Search, cx);
        assert!(
            requests.borrow().is_empty(),
            "switching roots asks Search for nothing"
        );
    }

    #[gpui::test]
    fn search_survives_details_and_the_player(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        go(&root, RootDestination::Search, cx);
        let search = search_of(&root, cx);
        let requests = search.update(cx, |search, _| search.record_requests());
        let before = scroll_search(&search, cx);
        let grid = search.read_with(cx, |search, _| search.grid());

        // Search → Details → Search.
        let opened = search.read_with(cx, |search, _| search.model.items()[before.1].id().clone());
        search.update(cx, |search, _| search.model.note_opened(opened.clone()));
        let details = push_details(&root, cx);
        root.update(cx, |root, cx| root.close_details(&details, cx));
        cx.run_until_parked();
        assert_eq!(search_of(&root, cx), search);
        assert_eq!(
            root.read_with(cx, |root, _| root.root),
            RootDestination::Search,
            "Back returns to Search, not Home"
        );
        assert_search_kept(&search, cx, &before);
        assert!(
            focused(grid.focus_handle(), cx),
            "back from Details: focus on the results, at the title"
        );
        assert!(requests.borrow().is_empty(), "Details → Back asks nothing");

        // Search → Details → Player → Details → Search.
        let details = push_details(&root, cx);
        root.update_in(cx, |root, window, cx| {
            let runtime = Arc::clone(&root.services.runtime);
            let player =
                cx.new(|cx| PlayerScreen::preview(runtime, PlayerPreview::Paused, window, cx));
            root.pages.mark_root_stale();
            root.push_player(player, cx);
        });
        cx.run_until_parked();
        root.update(cx, |root, cx| root.release_player(cx));
        cx.run_until_parked();
        root.update(cx, |root, cx| root.close_details(&details, cx));
        cx.run_until_parked();
        assert_eq!(search_of(&root, cx), search);
        assert!(root.read_with(cx, |root, _| root.pages.is_empty()));
        assert_search_kept(&search, cx, &before);
        assert!(focused(grid.focus_handle(), cx));
        // Playback reconciled exactly the opened title, and searched nothing.
        let sent = requests.borrow().clone();
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(
            matches!(&sent[0], crate::search::model::Request::Item { id, .. } if *id == opened)
        );
        // Search took its playback mark; Home and Library keep theirs.
        root.update(cx, |root, _| {
            assert!(!root.stale.take(Root::Search));
            assert!(root.stale.take(Root::Home));
            assert!(root.stale.take(Root::Library));
        });
    }

    #[gpui::test]
    fn search_reconciles_after_playback_even_when_shown_later(cx: &mut TestAppContext) {
        // Playback from Home while Search was hidden: Search reconciles the
        // title it had opened when it next shows, once.
        let (root, cx) = open(cx);
        go(&root, RootDestination::Search, cx);
        let search = search_of(&root, cx);
        let opened = search.read_with(cx, |search, _| search.model.items()[3].id().clone());
        search.update(cx, |search, _| search.model.note_opened(opened.clone()));
        let requests = search.update(cx, |search, _| search.record_requests());
        go(&root, RootDestination::Home, cx);
        let details = push_details(&root, cx);
        root.update_in(cx, |root, window, cx| {
            let runtime = Arc::clone(&root.services.runtime);
            let player =
                cx.new(|cx| PlayerScreen::preview(runtime, PlayerPreview::Paused, window, cx));
            root.pages.mark_root_stale();
            root.push_player(player, cx);
        });
        cx.run_until_parked();
        root.update(cx, |root, cx| root.release_player(cx));
        root.update(cx, |root, cx| root.close_details(&details, cx));
        cx.run_until_parked();
        assert!(requests.borrow().is_empty(), "hidden: nothing yet");
        go(&root, RootDestination::Search, cx);
        assert_eq!(requests.borrow().len(), 1, "one title on showing");
        go(&root, RootDestination::Home, cx);
        go(&root, RootDestination::Search, cx);
        assert_eq!(requests.borrow().len(), 1, "and only once");
    }

    #[gpui::test]
    fn a_session_end_from_search_signs_out_once_and_drops_it(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        go(&root, RootDestination::Search, cx);
        let search = search_of(&root, cx);
        let weak = search.downgrade();
        search.update(cx, |_, cx| {
            cx.emit(SearchEvent::SessionExpired);
            cx.emit(SearchEvent::SessionExpired);
        });
        cx.run_until_parked();
        root.read_with(cx, |root, _| {
            assert!(root.model.shows_login());
            assert!(root.search.is_none());
            assert!(root.home.is_none());
            assert_eq!(root.root, RootDestination::Home);
            assert_eq!(root.model.notice(), Some(crate::model::SESSION_ENDED));
        });
        // The old Search reporting again changes nothing.
        search.update(cx, |_, cx| cx.emit(SearchEvent::SessionExpired));
        cx.run_until_parked();
        assert!(root.read_with(cx, |root, _| root.model.shows_login()));
        drop(search);
        cx.run_until_parked();
        assert!(weak.upgrade().is_none(), "the old Search is released");
    }

    #[gpui::test]
    fn search_escape_on_an_empty_field_goes_home(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        go(&root, RootDestination::Search, cx);
        let search = search_of(&root, cx);
        search.update(cx, |_, cx| {
            cx.emit(SearchEvent::Navigate(RootDestination::Home))
        });
        cx.run_until_parked();
        assert_eq!(
            root.read_with(cx, |root, _| root.root),
            RootDestination::Home
        );
        assert!(root.read_with(cx, |root, _| root.search.is_some()), "kept");
    }

    #[gpui::test]
    fn search_review_scenes_open_on_search(cx: &mut TestAppContext) {
        let services = Services::memory().unwrap();
        let (root, cx) = cx.add_window_view(move |window, cx| {
            MatineeRoot::new(services, Some(ReviewScene::SearchStale), window, cx)
        });
        cx.run_until_parked();
        root.read_with(cx, |root, cx| {
            assert_eq!(root.root, RootDestination::Search);
            assert!(root.home.is_none(), "no live Home in a Search scene");
            let search = root.search.clone().expect("the fixture Search");
            assert!(search.read(cx).model.is_stale());
        });
    }

    #[gpui::test]
    fn sign_out_from_search_drops_it(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        go(&root, RootDestination::Search, cx);
        let search = search_of(&root, cx);
        let weak = search.downgrade();
        search.update(cx, |_, cx| cx.emit(SearchEvent::SignOut));
        drop(search);
        // The vault work runs on the service runtime; wait for its answer.
        for _ in 0..200 {
            cx.run_until_parked();
            if root.read_with(cx, |root, _| root.model.shows_login()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        root.read_with(cx, |root, _| {
            assert!(root.model.shows_login(), "signed out");
            assert!(root.search.is_none());
            assert_eq!(root.root, RootDestination::Home);
        });
        cx.run_until_parked();
        assert!(
            weak.upgrade().is_none(),
            "Search is released with the session"
        );
    }

    fn calendar_of(
        root: &Entity<MatineeRoot>,
        cx: &mut VisualTestContext,
    ) -> Entity<CalendarScreen> {
        root.read_with(cx, |root, _| {
            root.calendar.clone().expect("Calendar exists")
        })
    }

    #[gpui::test]
    fn calendar_is_a_retained_root_beside_home_library_and_search(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        go(&root, RootDestination::Calendar, cx);
        let calendar = calendar_of(&root, cx);
        root.read_with(cx, |root, _| {
            assert_eq!(root.root, RootDestination::Calendar)
        });
        // A month moved and a day chosen, then the calendar is left.
        calendar.update(cx, |calendar, _| calendar.model.next_month());
        let kept = calendar.read_with(cx, |calendar, _| {
            (calendar.model.month(), calendar.model.selected())
        });
        go(&root, RootDestination::Home, cx);
        root.read_with(cx, |root, _| assert_eq!(root.root, RootDestination::Home));
        go(&root, RootDestination::Search, cx);
        go(&root, RootDestination::Library(LibraryKind::Movies), cx);
        go(&root, RootDestination::Calendar, cx);
        assert_eq!(calendar_of(&root, cx), calendar, "the same Calendar");
        assert_eq!(
            calendar.read_with(cx, |calendar, _| {
                (calendar.model.month(), calendar.model.selected())
            }),
            kept,
            "the month and the day are where the person left them"
        );
        root.read_with(cx, |root, _| {
            assert_eq!(root.root, RootDestination::Calendar)
        });
    }

    #[gpui::test]
    fn calendar_has_no_live_home_and_no_details_route(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        go(&root, RootDestination::Calendar, cx);
        // Calendar never opens Details: its events have no Jellyfin identity.
        // The only pages above a root come from Home, Library, and Search.
        root.read_with(cx, |root, _| {
            assert!(root.pages.is_empty(), "no page above Calendar");
            assert_eq!(root.root.root(), Root::Calendar);
        });
    }

    #[gpui::test]
    fn sign_out_from_calendar_drops_it(cx: &mut TestAppContext) {
        let (root, cx) = open(cx);
        go(&root, RootDestination::Calendar, cx);
        let calendar = calendar_of(&root, cx);
        let weak = calendar.downgrade();
        calendar.update(cx, |_, cx| cx.emit(CalendarScreenEvent::SignOut));
        drop(calendar);
        // The vault work runs on the service runtime; wait for its answer.
        for _ in 0..200 {
            cx.run_until_parked();
            if root.read_with(cx, |root, _| root.model.shows_login()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        root.read_with(cx, |root, _| {
            assert!(root.model.shows_login(), "signed out");
            assert!(root.calendar.is_none());
            assert_eq!(root.root, RootDestination::Home);
        });
        cx.run_until_parked();
        assert!(weak.upgrade().is_none(), "the Calendar was released");
    }
}
