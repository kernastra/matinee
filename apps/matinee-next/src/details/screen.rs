//! Details screen.
//!
//! Paints [`DetailsModel`] and runs its requests on the service runtime.
//! Artwork comes from the shared [`ArtworkLoader`]; this screen decides which
//! images it wants from what the model shows, starts the missing ones, and
//! cancels the ones it no longer shows. A late image for a slot that is gone
//! is dropped.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use atelier_ui::gpui::{EventEmitter, KeyDownEvent, linear_color_stop, linear_gradient, point};
use atelier_ui::prelude::*;
use matinee_core::{Chapter, CollectionContext, Credit, ImageRole, ItemId, ItemKind, MediaItem};
use matinee_jellyfin::{ArtworkRequest, ArtworkUrls, Session};
use tokio::task::JoinHandle;

use super::load;
use super::model::{
    DetailsFailure, DetailsModel, Hero, NO_OVERVIEW, PlayAction, Request, Section, cast, credited,
    episode_heading, eyebrow, genre_line, meta_line, progress_fraction, score_line, summary,
};
use crate::artwork::{
    Artwork, ArtworkLoad, ArtworkLoader, BACKDROP_WIDTH, Client, PORTRAIT_WIDTH, POSTER_WIDTH,
    THUMB_WIDTH,
};
use crate::player::format_clock;
use crate::runtime::ServiceRuntime;

/// What the shell does for this screen.
pub(crate) enum DetailsEvent {
    Play(ItemId),
    Back,
}

pub(crate) struct DetailsScreen {
    runtime: Arc<ServiceRuntime>,
    /// `None` for review scenes, which never open a socket.
    client: Option<Client>,
    /// Builds artwork addresses. No token is ever put on them.
    session: Session,
    loader: ArtworkLoader,
    pub(super) model: DetailsModel,
    tasks: Vec<JoinHandle<()>>,
    pub(super) art: HashMap<String, Artwork>,
    art_tasks: HashMap<String, JoinHandle<()>>,
    focus: FocusHandle,
    primary_focus: FocusHandle,
    focus_primary: bool,
    scroll: ScrollControl,
}

impl EventEmitter<DetailsEvent> for DetailsScreen {}

impl DetailsScreen {
    pub(crate) fn open(
        runtime: Arc<ServiceRuntime>,
        client: Client,
        loader: ArtworkLoader,
        item_id: ItemId,
        cx: &mut Context<Self>,
    ) -> Self {
        let (model, requests) = DetailsModel::open(item_id);
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
        model: DetailsModel,
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
            primary_focus: cx.focus_handle().tab_index(0).tab_stop(true),
            focus_primary: true,
            scroll: ScrollControl::new(),
        }
    }

    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// The Player closed over this screen. Refresh what playback changed and
    /// put focus back on Play.
    pub(crate) fn resume(&mut self, cx: &mut Context<Self>) {
        let requests = self.model.refresh();
        self.run(requests, cx);
        self.focus_primary = true;
        cx.notify();
    }

    fn play(&mut self, target: ItemId, cx: &mut Context<Self>) {
        cx.emit(DetailsEvent::Play(target));
    }

    /// A related title replaces this one, as in the shipping app.
    fn show(&mut self, item_id: ItemId, cx: &mut Context<Self>) {
        if &item_id == self.model.item_id() {
            return;
        }
        for task in self.tasks.drain(..) {
            task.abort();
        }
        let requests = self.model.show(item_id);
        self.scroll.set_offset(point(px(0.0), px(0.0)));
        self.run(requests, cx);
        self.sync_artwork(cx);
        self.focus_primary = true;
        cx.notify();
    }

    fn select_season(&mut self, season: ItemId, cx: &mut Context<Self>) {
        let requests = self.model.select_season(season);
        self.run(requests, cx);
        self.sync_artwork(cx);
        cx.notify();
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let requests = self.model.retry();
        self.run(requests, cx);
        cx.notify();
    }

    fn run(&mut self, requests: Vec<Request>, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        for request in requests {
            let ticket = request.ticket();
            let client = Arc::clone(&client);
            let (task, rx) = self
                .runtime
                .spawn(async move { load::run(client.as_ref(), &request).await });
            self.tasks.retain(|task| !task.is_finished());
            self.tasks.push(task);
            cx.spawn(async move |this, cx| {
                let Ok(response) = rx.await else {
                    return;
                };
                this.update(cx, |this, cx| {
                    let next = this.model.apply(ticket, response);
                    this.run(next, cx);
                    this.sync_artwork(cx);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    /// Every image the current model shows, in paint order.
    pub(super) fn wanted_artwork(&self) -> Vec<ArtworkRequest> {
        let urls = self.session.artwork();
        let Some(item) = self.model.item() else {
            return Vec::new();
        };
        let mut wanted: Vec<ArtworkRequest> = Vec::new();
        wanted.extend(backdrop_request(item, &urls));
        wanted.extend(hero_art_request(item, &urls));
        for person in cast(item) {
            wanted.extend(urls.person_request(person, PORTRAIT_WIDTH));
        }
        if let Some(episodes) = self.model.episodes().ready() {
            for episode in episodes {
                wanted.extend(urls.item_request(episode, ImageRole::Primary, THUMB_WIDTH));
            }
        }
        if let Some(contexts) = self.model.collections().ready() {
            for context in contexts {
                for title in &context.items {
                    wanted.extend(urls.item_request(title, ImageRole::Primary, POSTER_WIDTH));
                }
            }
        }
        if let Some(similar) = self.model.similar().ready() {
            for title in similar {
                wanted.extend(urls.item_request(title, ImageRole::Primary, POSTER_WIDTH));
            }
        }
        wanted
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

    fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let plain = !key.modifiers.shift
            && !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.platform;
        if plain && key.key == "escape" {
            cx.emit(DetailsEvent::Back);
            cx.stop_propagation();
        }
    }
}

impl Drop for DetailsScreen {
    fn drop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        for (_, task) in self.art_tasks.drain() {
            task.abort();
        }
    }
}

/// Item backdrop, or the series backdrop for an episode or season.
fn backdrop_request(item: &MediaItem, urls: &ArtworkUrls<'_>) -> Option<ArtworkRequest> {
    urls.item_request(item, ImageRole::Backdrop, BACKDROP_WIDTH)
        .or_else(|| {
            let series = item.hierarchy.series_id.as_ref()?;
            Some(urls.image_request(series, ImageRole::Backdrop, BACKDROP_WIDTH))
        })
}

/// Poster for a title, still frame for an episode.
fn hero_art_request(item: &MediaItem, urls: &ArtworkUrls<'_>) -> Option<ArtworkRequest> {
    let width = if item.kind == ItemKind::Episode {
        THUMB_WIDTH
    } else {
        POSTER_WIDTH
    };
    urls.item_request(item, ImageRole::Primary, width)
}

/// Measurements that follow the window width.
#[derive(Clone, Copy)]
struct Layout {
    gutter: Space,
    hero_height: f32,
    poster: (f32, f32),
    still: (f32, f32),
    copy_width: f32,
    compact: bool,
    thumb: (f32, f32),
}

impl Layout {
    fn for_viewport(width: f32, height: f32) -> Self {
        let compact = width < 1100.0;
        let wide = width >= 1600.0;
        let poster_width = if compact {
            156.0
        } else if wide {
            248.0
        } else {
            208.0
        };
        let still_width = if compact { 256.0 } else { 352.0 };
        Self {
            gutter: if compact { Space::S10 } else { Space::S16 },
            hero_height: (height * 0.8).clamp(460.0, 820.0),
            poster: (poster_width, poster_width * 1.5),
            still: (still_width, still_width * 9.0 / 16.0),
            copy_width: if wide { 720.0 } else { 620.0 },
            compact,
            thumb: if compact {
                (176.0, 99.0)
            } else {
                (224.0, 126.0)
            },
        }
    }
}

impl Render for DetailsScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_primary && !matches!(self.model.hero(), Hero::Loading) {
            self.focus_primary = false;
            let target = if self.model.primary().is_some() {
                self.primary_focus.clone()
            } else {
                self.focus.clone()
            };
            window.on_next_frame(move |window, _| window.focus(&target));
        }
        let theme = cx.theme().clone();
        let viewport = window.viewport_size();
        let layout = Layout::for_viewport(f32::from(viewport.width), f32::from(viewport.height));
        let backdrop = self
            .model
            .item()
            .map(|item| self.art(backdrop_request(item, &self.session.artwork()).as_ref()));

        div()
            .id("details")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.colors.surface.canvas)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event, _, cx| this.on_key(event, cx)))
            .child(backdrop_layer(
                &theme,
                backdrop.as_ref(),
                f32::from(viewport.width),
                layout.hero_height,
            ))
            .child(
                ScrollView::vertical("details-scroll")
                    .control(self.scroll.clone())
                    .size_full()
                    .child(
                        v_stack(Space::S0)
                            .w_full()
                            .pb(Space::S16.px())
                            .child(self.hero(&theme, layout, cx))
                            .child(self.lower(&theme, layout, cx)),
                    ),
            )
    }
}

impl DetailsScreen {
    fn hero(&self, theme: &Theme, layout: Layout, cx: &mut Context<Self>) -> AnyElement {
        let back = Button::new("details-back", "Back")
            .variant(ButtonVariant::Subtle)
            .size(ButtonSize::Small)
            .icon(IconName::ChevronLeft)
            .on_click(cx.listener(|_, _, _, cx| cx.emit(DetailsEvent::Back)));
        let body = match self.model.hero() {
            Hero::Loading => hero_skeleton(theme, layout).into_any_element(),
            Hero::Failed(failure) => self.failure(theme, *failure, cx).into_any_element(),
            Hero::Ready(item) => self.ready_hero(theme, layout, item, cx).into_any_element(),
        };
        v_stack(Space::S0)
            .w_full()
            .min_h(px(layout.hero_height))
            .px(layout.gutter.px())
            .pt(Space::S6.px())
            .pb(Space::S10.px())
            .child(h_stack(Space::S0).child(back))
            .child(div().flex_1().min_h(Space::S10.px()))
            .child(body)
            .into_any_element()
    }

    fn ready_hero(
        &self,
        theme: &Theme,
        layout: Layout,
        item: &MediaItem,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let urls = self.session.artwork();
        let episode = item.kind == ItemKind::Episode;
        let (art_w, art_h) = if episode { layout.still } else { layout.poster };
        let art = self.art(hero_art_request(item, &urls).as_ref());
        let seasons = self.model.seasons().ready().map(Vec::len);
        let title_role = if layout.compact || item.name().chars().count() > 26 {
            TextRole::Title
        } else {
            TextRole::Display
        };
        let primary = self.model.primary();
        let progress = match &primary {
            Some(action) if action.episode.is_some() => {
                self.model.next_up().and_then(progress_fraction)
            }
            Some(_) => progress_fraction(item),
            None => None,
        }
        .filter(|_| {
            primary
                .as_ref()
                .is_some_and(|action| action.resume.is_some())
        });
        let watched = item.user.is_played()
            && primary
                .as_ref()
                .is_none_or(|action| action.resume.is_none());
        let directors = credited(item, &Credit::Director, 2);
        let writers = credited(item, &Credit::Writer, 3);
        let technical = item.media.summary();
        let technical = [technical.video, technical.audio]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("   ");
        let overview = item
            .metadata
            .overview
            .clone()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| NO_OVERVIEW.into());
        let tagline = item.metadata.taglines.first().cloned();

        h_stack(Space::S8)
            .w_full()
            .items_end()
            .child(art_frame(
                theme,
                "hero-art",
                &art,
                art_w,
                art_h,
                Radius::Large,
                item.name(),
            ))
            .child(
                v_stack(Space::S3)
                    .flex_1()
                    .max_w(px(layout.copy_width))
                    .child(
                        Text::new(eyebrow(item, seasons))
                            .role(TextRole::Caption)
                            .color(theme.colors.control.accent),
                    )
                    .child(Text::new(item.name().to_string()).role(title_role))
                    .when_some(tagline, |stack, tagline| {
                        stack.child(
                            Text::new(tagline)
                                .role(TextRole::Subheading)
                                .tone(TextTone::Secondary),
                        )
                    })
                    .child(
                        h_stack(Space::S4)
                            .flex_wrap()
                            .child(
                                Text::new(meta_line(item))
                                    .role(TextRole::Metadata)
                                    .tone(TextTone::Secondary),
                            )
                            .when_some(score_line(item), |row, scores| {
                                row.child(
                                    Text::new(scores)
                                        .role(TextRole::Metadata)
                                        .tone(TextTone::Muted),
                                )
                            }),
                    )
                    .when_some(genre_line(item), |stack, genres| {
                        stack.child(
                            Text::new(genres)
                                .role(TextRole::Label)
                                .tone(TextTone::Secondary),
                        )
                    })
                    .child(
                        div().pt(Space::S1.px()).child(
                            Text::new(overview)
                                .role(TextRole::Body)
                                .tone(TextTone::Secondary),
                        ),
                    )
                    .when(!directors.is_empty() || !writers.is_empty(), |stack| {
                        stack.child(
                            h_stack(Space::S6)
                                .flex_wrap()
                                .when(!directors.is_empty(), |row| {
                                    row.child(credit_line("Directed by", &directors))
                                })
                                .when(!writers.is_empty(), |row| {
                                    row.child(credit_line("Written by", &writers))
                                }),
                        )
                    })
                    .child(
                        h_stack(Space::S4)
                            .pt(Space::S3.px())
                            .items_center()
                            .flex_wrap()
                            .when_some(primary.clone(), |row, action| {
                                row.child(self.primary_button(action, cx))
                            })
                            .when_some(progress, |row, fraction| {
                                row.child(progress_line(theme, fraction, 160.0))
                            })
                            .when(watched, |row| {
                                row.child(
                                    h_stack(Space::S1)
                                        .items_center()
                                        .child(
                                            Icon::new(IconName::Check)
                                                .size(IconSize::Small)
                                                .color(theme.colors.text.muted),
                                        )
                                        .child(
                                            Text::new("Watched")
                                                .role(TextRole::Caption)
                                                .tone(TextTone::Muted),
                                        ),
                                )
                            })
                            .when(primary.is_none() && self.model.is_series(), |row| {
                                row.child(
                                    Text::new(series_status(self.model.seasons()))
                                        .role(TextRole::Caption)
                                        .tone(TextTone::Muted),
                                )
                            }),
                    )
                    .when(!technical.is_empty(), |stack| {
                        stack.child(
                            Text::new(technical)
                                .role(TextRole::Caption)
                                .tone(TextTone::Muted),
                        )
                    }),
            )
    }

    fn primary_button(&self, action: PlayAction, cx: &mut Context<Self>) -> impl IntoElement {
        let target = action.target.clone();
        Button::new("details-play", action.label())
            .variant(ButtonVariant::Primary)
            .size(ButtonSize::Large)
            .icon(IconName::Play)
            .focus_handle(self.primary_focus.clone())
            .on_click(cx.listener(move |this, _, _, cx| this.play(target.clone(), cx)))
    }

    fn failure(
        &self,
        theme: &Theme,
        failure: DetailsFailure,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_stack(Space::S3)
            .max_w(px(560.0))
            .child(
                Text::new("Details")
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
                h_stack(Space::S3)
                    .pt(Space::S3.px())
                    .child(
                        Button::new("details-retry", "Try again")
                            .variant(ButtonVariant::Primary)
                            .focus_handle(self.primary_focus.clone())
                            .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                    )
                    .child(
                        Button::new("details-failure-back", "Back to Matinee")
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(DetailsEvent::Back))),
                    ),
            )
    }

    fn lower(&self, theme: &Theme, layout: Layout, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self.model.item() else {
            return div().into_any_element();
        };
        let urls = self.session.artwork();
        let mut sections: Vec<AnyElement> = Vec::new();
        if self.model.is_series() {
            sections.push(self.episodes_section(theme, layout, cx).into_any_element());
        }
        let people = cast(item);
        if !people.is_empty() {
            let tiles: Vec<AnyElement> = people
                .iter()
                .enumerate()
                .map(|(index, person)| {
                    let art = self.art(urls.person_request(person, PORTRAIT_WIDTH).as_ref());
                    cast_tile(
                        theme,
                        index,
                        person.name.as_str(),
                        person.role.as_deref(),
                        &art,
                    )
                    .into_any_element()
                })
                .collect();
            sections.push(shelf(theme, "Cast", None, tiles, "details-cast").into_any_element());
        }
        if !item.chapters.is_empty() {
            sections.push(chapters_section(theme, &item.chapters).into_any_element());
        }
        if let Some(contexts) = self.model.collections().ready() {
            for (index, context) in contexts.iter().enumerate() {
                sections.push(
                    self.collection_shelf(theme, index, context, &urls, cx)
                        .into_any_element(),
                );
            }
        }
        match self.model.similar() {
            Section::Ready(similar) if !similar.is_empty() => {
                let tiles = similar
                    .iter()
                    .map(|title| {
                        self.poster_tile(theme, "similar", title, &urls, cx)
                            .into_any_element()
                    })
                    .collect();
                sections.push(
                    shelf(
                        theme,
                        "More like this",
                        Some("From your library"),
                        tiles,
                        "details-similar",
                    )
                    .into_any_element(),
                );
            }
            Section::Loading if !self.model.is_series() => {
                sections.push(shelf_skeleton(theme, "More like this").into_any_element());
            }
            Section::Unavailable => {
                sections.push(
                    quiet_note(theme, "More like this isn't available right now.")
                        .into_any_element(),
                );
            }
            _ => {}
        }
        v_stack(Space::S12)
            .w_full()
            .px(layout.gutter.px())
            .pt(Space::S4.px())
            .children(sections)
            .into_any_element()
    }

    fn episodes_section(
        &self,
        theme: &Theme,
        layout: Layout,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let urls = self.session.artwork();
        let header = match self.model.seasons() {
            Section::Loading => Text::new("Loading seasons…")
                .role(TextRole::Caption)
                .tone(TextTone::Muted)
                .into_any_element(),
            Section::Unavailable => {
                quiet_note(theme, "Seasons couldn't be loaded from Jellyfin.").into_any_element()
            }
            Section::Ready(seasons) if seasons.is_empty() => {
                quiet_note(theme, "This series has no seasons in your library yet.")
                    .into_any_element()
            }
            Section::Ready(seasons) => {
                let selected = self
                    .model
                    .selected_season()
                    .and_then(|id| seasons.iter().position(|season| season.id() == id))
                    .unwrap_or(0);
                let ids: Vec<ItemId> = seasons.iter().map(|season| season.id().clone()).collect();
                let segments = seasons
                    .iter()
                    .map(|season| Segment::new(season.name().to_string()))
                    .collect();
                ScrollView::new("details-seasons")
                    .axis(ScrollAxis::Horizontal)
                    .w_full()
                    .child({
                        let screen = cx.entity();
                        SegmentedControl::new("details-season-picker", segments, selected)
                            .on_change(move |index, _, cx| {
                                if let Some(id) = ids.get(index) {
                                    let id = id.clone();
                                    screen.update(cx, |this, cx| this.select_season(id, cx));
                                }
                            })
                    })
                    .into_any_element()
            }
        };
        let body = match self.model.episodes() {
            Section::Loading if self.model.seasons().ready().is_some() => v_stack(Space::S3)
                .children((0..3).map(|index| episode_skeleton(theme, layout, index)))
                .into_any_element(),
            Section::Loading => div().into_any_element(),
            Section::Unavailable => h_stack(Space::S3)
                .items_center()
                .child(quiet_note(theme, "Episodes couldn't be loaded."))
                .when_some(self.model.selected_season().cloned(), |row, season| {
                    row.child(
                        Button::new("details-episodes-retry", "Try again")
                            .variant(ButtonVariant::Secondary)
                            .size(ButtonSize::Small)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_season(season.clone(), cx);
                            })),
                    )
                })
                .into_any_element(),
            Section::Ready(episodes) if episodes.is_empty() => {
                quiet_note(theme, "No episodes are in this season yet.").into_any_element()
            }
            Section::Ready(episodes) => v_stack(Space::S1)
                .w_full()
                .children(episodes.iter().map(|episode| {
                    self.episode_row(theme, layout, episode, &urls, cx)
                        .into_any_element()
                }))
                .into_any_element(),
        };
        v_stack(Space::S5)
            .w_full()
            .child(Text::new("Episodes").role(TextRole::Heading))
            .child(header)
            .child(body)
    }

    fn episode_row(
        &self,
        theme: &Theme,
        layout: Layout,
        episode: &MediaItem,
        urls: &ArtworkUrls<'_>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let action = PlayAction::for_item(episode);
        let target = action.target.clone();
        let art = self.art(
            urls.item_request(episode, ImageRole::Primary, THUMB_WIDTH)
                .as_ref(),
        );
        let progress = action.resume.and(progress_fraction(episode));
        let played = episode.user.is_played() && action.resume.is_none();
        let overview = episode
            .metadata
            .overview
            .as_deref()
            .map(|text| summary(text, 220))
            .filter(|text| !text.is_empty());
        let (thumb_w, thumb_h) = layout.thumb;
        let label = format!("{} {}", action.label(), episode.name());
        Pressable::new(("episode", stable_index(episode.id())), label)
            .w_full()
            .p(Space::S2.px())
            .on_press(cx.listener(move |this, _, _, cx| this.play(target.clone(), cx)))
            .child(
                h_stack(Space::S5)
                    .w_full()
                    .items_center()
                    .child(
                        div()
                            .relative()
                            .flex_none()
                            .child(art_frame(
                                theme,
                                ("episode-art", stable_index(episode.id())),
                                &art,
                                thumb_w,
                                thumb_h,
                                Radius::Medium,
                                episode.name(),
                            ))
                            .when_some(progress, |frame, fraction| {
                                frame.child(
                                    div()
                                        .absolute()
                                        .left(Space::S2.px())
                                        .right(Space::S2.px())
                                        .bottom(Space::S2.px())
                                        .child(progress_line(theme, fraction, thumb_w - 16.0)),
                                )
                            }),
                    )
                    .child(
                        v_stack(Space::S1)
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                h_stack(Space::S3)
                                    .items_center()
                                    .child(
                                        Text::new(episode_heading(episode))
                                            .role(TextRole::Caption)
                                            .tone(TextTone::Muted),
                                    )
                                    .when(played, |row| {
                                        row.child(
                                            h_stack(Space::S1)
                                                .items_center()
                                                .child(
                                                    Icon::new(IconName::Check)
                                                        .size(IconSize::Small)
                                                        .color(theme.colors.text.muted),
                                                )
                                                .child(
                                                    Text::new("Watched")
                                                        .role(TextRole::Caption)
                                                        .tone(TextTone::Muted),
                                                ),
                                        )
                                    }),
                            )
                            .child(Text::new(episode.name().to_string()).role(TextRole::Subheading))
                            .when_some(overview, |stack, text| {
                                stack.child(
                                    Text::new(text).role(TextRole::Body).tone(TextTone::Muted),
                                )
                            })
                            .when_some(action.resume, |stack, at| {
                                stack.child(
                                    Text::new(format!("Resume from {}", format_clock(at)))
                                        .role(TextRole::Caption)
                                        .color(theme.colors.control.accent),
                                )
                            }),
                    ),
            )
    }

    fn collection_shelf(
        &self,
        theme: &Theme,
        index: usize,
        context: &CollectionContext,
        urls: &ArtworkUrls<'_>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tiles = context
            .items
            .iter()
            .map(|title| {
                self.poster_tile(theme, "collection", title, urls, cx)
                    .into_any_element()
            })
            .collect();
        shelf(
            theme,
            context.collection.name(),
            Some("Part of a collection"),
            tiles,
            ("details-collection", index),
        )
    }

    fn poster_tile(
        &self,
        theme: &Theme,
        group: &'static str,
        title: &MediaItem,
        urls: &ArtworkUrls<'_>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let art = self.art(
            urls.item_request(title, ImageRole::Primary, POSTER_WIDTH)
                .as_ref(),
        );
        let target = title.id().clone();
        let year = title.metadata.year.map(|year| year.to_string());
        Pressable::new((group, stable_index(title.id())), title.name().to_string())
            .w(px(POSTER_TILE.0 + Space::S2.value()))
            .flex_none()
            .p(Space::S1.px())
            .on_press(cx.listener(move |this, _, _, cx| this.show(target.clone(), cx)))
            .child(
                v_stack(Space::S2)
                    .child(art_frame(
                        theme,
                        (group, stable_index(title.id())),
                        &art,
                        POSTER_TILE.0,
                        POSTER_TILE.1,
                        Radius::Medium,
                        title.name(),
                    ))
                    .child(
                        Text::new(title.name().to_string())
                            .role(TextRole::Label)
                            .truncate(),
                    )
                    .when_some(year, |stack, year| {
                        stack.child(
                            Text::new(year)
                                .role(TextRole::Caption)
                                .tone(TextTone::Muted),
                        )
                    }),
            )
    }
}

const POSTER_TILE: (f32, f32) = (136.0, 204.0);
const PORTRAIT: f32 = 88.0;

/// A small stable number for element ids derived from an item id.
fn stable_index(id: &ItemId) -> usize {
    id.as_str().bytes().fold(0usize, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as usize)
    })
}

fn series_status(seasons: &Section<Vec<MediaItem>>) -> &'static str {
    match seasons {
        Section::Loading => "Finding where you left off…",
        _ => "Choose an episode below.",
    }
}

fn credit_line(label: &str, names: &[String]) -> impl IntoElement {
    h_stack(Space::S2)
        .child(
            Text::new(label.to_string())
                .role(TextRole::Caption)
                .tone(TextTone::Muted),
        )
        .child(
            Text::new(names.join(", "))
                .role(TextRole::Label)
                .tone(TextTone::Secondary),
        )
}

/// A quiet progress line: amber over a faint track.
fn progress_line(theme: &Theme, fraction: f32, width: f32) -> impl IntoElement {
    let fraction = fraction.clamp(0.0, 1.0);
    div()
        .w(px(width))
        .h(px(3.0))
        .rounded(px(theme.radius.get(Radius::Full)))
        .bg(theme.colors.text.primary.with_alpha(0.18))
        .child(
            div()
                .h_full()
                .w(px(width * fraction))
                .rounded(px(theme.radius.get(Radius::Full)))
                .bg(theme.colors.control.accent),
        )
}

/// Artwork in a fixed frame. Loading and missing art are calm surfaces;
/// missing art carries the title so the frame still says something.
fn art_frame(
    theme: &Theme,
    id: impl Into<ElementId>,
    art: &Artwork,
    width: f32,
    height: f32,
    radius: Radius,
    title: &str,
) -> AnyElement {
    match art {
        Artwork::Ready(image) => Image::decoded(id, image.clone())
            .frame(width, height)
            .fit(ImageFit::Fill)
            .radius(radius)
            .label(title.to_string())
            .into_any_element(),
        Artwork::Loading => div()
            .id(id)
            .flex_none()
            .w(px(width))
            .h(px(height))
            .rounded(px(theme.radius.get(radius)))
            .bg(theme.colors.surface.elevated)
            .into_any_element(),
        Artwork::Missing | Artwork::Failed => div()
            .id(id)
            .flex_none()
            .w(px(width))
            .h(px(height))
            .p(Space::S3.px())
            .flex()
            .items_end()
            .rounded(px(theme.radius.get(radius)))
            .bg(theme.colors.surface.elevated)
            .border(px(1.0))
            .border_color(theme.colors.border.subtle)
            .child(
                Text::new(title.to_string())
                    .role(TextRole::Caption)
                    .tone(TextTone::Muted),
            )
            .into_any_element(),
    }
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
                Image::decoded("details-backdrop", image.clone())
                    .frame(width, height)
                    .fit(ImageFit::Fill)
                    .radius(Radius::None)
                    .label("Backdrop"),
            )
        })
        // Readability: the copy sits on the left and the page continues below.
        .child(div().absolute().inset_0().bg(linear_gradient(
            90.0,
            linear_color_stop(canvas.with_alpha(0.94), 0.0),
            linear_color_stop(canvas.with_alpha(0.25), 1.0),
        )))
        .child(div().absolute().inset_0().bg(linear_gradient(
            180.0,
            linear_color_stop(canvas.with_alpha(0.15), 0.35),
            linear_color_stop(canvas, 1.0),
        )))
}

fn hero_skeleton(theme: &Theme, layout: Layout) -> impl IntoElement {
    let block = |width: f32, height: f32| {
        div()
            .w(px(width))
            .h(px(height))
            .rounded(px(theme.radius.get(Radius::Small)))
            .bg(theme.colors.surface.elevated)
    };
    h_stack(Space::S8)
        .items_end()
        .child(
            div()
                .w(px(layout.poster.0))
                .h(px(layout.poster.1))
                .rounded(px(theme.radius.get(Radius::Large)))
                .bg(theme.colors.surface.panel),
        )
        .child(
            v_stack(Space::S3)
                .child(block(96.0, 12.0))
                .child(block(420.0, 44.0))
                .child(block(220.0, 14.0))
                .child(
                    v_stack(Space::S2)
                        .pt(Space::S2.px())
                        .child(block(560.0, 14.0))
                        .child(block(540.0, 14.0))
                        .child(block(380.0, 14.0)),
                )
                .child(div().pt(Space::S3.px()).child(block(168.0, 44.0)))
                .child(
                    Text::new("Loading from Jellyfin…")
                        .role(TextRole::Caption)
                        .tone(TextTone::Muted),
                ),
        )
}

fn episode_skeleton(theme: &Theme, layout: Layout, index: usize) -> AnyElement {
    h_stack(Space::S5)
        .id(("episode-skeleton", index))
        .p(Space::S2.px())
        .items_center()
        .child(
            div()
                .w(px(layout.thumb.0))
                .h(px(layout.thumb.1))
                .rounded(px(theme.radius.get(Radius::Medium)))
                .bg(theme.colors.surface.panel),
        )
        .child(
            v_stack(Space::S2)
                .child(
                    div()
                        .w(px(80.0))
                        .h(px(10.0))
                        .rounded(px(3.0))
                        .bg(theme.colors.surface.elevated),
                )
                .child(
                    div()
                        .w(px(260.0))
                        .h(px(16.0))
                        .rounded(px(3.0))
                        .bg(theme.colors.surface.elevated),
                )
                .child(
                    div()
                        .w(px(420.0))
                        .h(px(12.0))
                        .rounded(px(3.0))
                        .bg(theme.colors.surface.panel),
                ),
        )
        .into_any_element()
}

fn shelf(
    theme: &Theme,
    title: &str,
    eyebrow: Option<&str>,
    tiles: Vec<AnyElement>,
    id: impl Into<ElementId>,
) -> impl IntoElement {
    v_stack(Space::S4)
        .w_full()
        .child(
            v_stack(Space::S1)
                .when_some(eyebrow, |stack, eyebrow| {
                    stack.child(
                        Text::new(eyebrow.to_string())
                            .role(TextRole::Caption)
                            .color(theme.colors.control.accent),
                    )
                })
                .child(Text::new(title.to_string()).role(TextRole::Heading)),
        )
        .child(
            ScrollView::new(id)
                .axis(ScrollAxis::Horizontal)
                .w_full()
                .child(
                    h_stack(Space::S4)
                        .items_start()
                        .pb(Space::S2.px())
                        .children(tiles),
                ),
        )
}

fn shelf_skeleton(theme: &Theme, title: &str) -> impl IntoElement {
    v_stack(Space::S4)
        .child(Text::new(title.to_string()).role(TextRole::Heading))
        .child(h_stack(Space::S4).children((0..5usize).map(|index| {
            div()
                .id(("shelf-skeleton", index))
                .w(px(POSTER_TILE.0))
                .h(px(POSTER_TILE.1))
                .rounded(px(theme.radius.get(Radius::Medium)))
                .bg(theme.colors.surface.panel)
        })))
}

fn cast_tile(
    theme: &Theme,
    index: usize,
    name: &str,
    role: Option<&str>,
    art: &Artwork,
) -> impl IntoElement {
    let initial = name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    v_stack(Space::S2)
        .id(("cast", index))
        .w(px(PORTRAIT + 32.0))
        .flex_none()
        .items_center()
        .child(match art {
            Artwork::Ready(image) => Image::decoded(("cast-art", index), image.clone())
                .frame(PORTRAIT, PORTRAIT)
                .fit(ImageFit::Fill)
                .radius(Radius::Full)
                .label(name.to_string())
                .into_any_element(),
            _ => div()
                .size(px(PORTRAIT))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(theme.radius.get(Radius::Full)))
                .bg(theme.colors.surface.elevated)
                .child(
                    Text::new(initial)
                        .role(TextRole::Subheading)
                        .tone(TextTone::Muted),
                )
                .into_any_element(),
        })
        .child(Text::new(name.to_string()).role(TextRole::Label).truncate())
        .when_some(
            role.filter(|role| !role.trim().is_empty()),
            |stack, role| {
                stack.child(
                    Text::new(role.to_string())
                        .role(TextRole::Caption)
                        .tone(TextTone::Muted)
                        .truncate(),
                )
            },
        )
}

/// Chapters are informational here. Playing from a chapter is deferred until
/// the Player can start at a chosen position.
fn chapters_section(theme: &Theme, chapters: &[Chapter]) -> impl IntoElement {
    v_stack(Space::S4)
        .w_full()
        .child(
            v_stack(Space::S1)
                .child(
                    Text::new(format!("{} chapters", chapters.len()))
                        .role(TextRole::Caption)
                        .color(theme.colors.control.accent),
                )
                .child(Text::new("Scenes").role(TextRole::Heading)),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(Space::S2.px())
                .children(chapters.iter().take(24).map(|chapter| {
                    h_stack(Space::S3)
                        .id(("chapter", chapter.index as usize))
                        .w(px(280.0))
                        .py(Space::S1.px())
                        .child(
                            Text::new(format_clock(chapter.start))
                                .role(TextRole::Metadata)
                                .tone(TextTone::Muted),
                        )
                        .child(
                            Text::new(
                                chapter
                                    .name
                                    .clone()
                                    .filter(|name| !name.trim().is_empty())
                                    .unwrap_or_else(|| format!("Chapter {}", chapter.index + 1)),
                            )
                            .role(TextRole::Label)
                            .tone(TextTone::Secondary)
                            .truncate(),
                        )
                })),
        )
}

fn quiet_note(theme: &Theme, text: &str) -> impl IntoElement {
    Text::new(text.to_string())
        .role(TextRole::Caption)
        .color(theme.colors.text.muted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use matinee_core::{ImageTag, ItemIdentity, ItemMetadata, TechnicalMedia, UserItemState};

    fn titled(kind: ItemKind) -> MediaItem {
        MediaItem {
            identity: ItemIdentity {
                id: ItemId::parse("ep-1").unwrap(),
                name: "Low Tide".into(),
            },
            kind,
            metadata: ItemMetadata::default(),
            artwork: Default::default(),
            user: UserItemState::default(),
            hierarchy: Default::default(),
            media: TechnicalMedia::default(),
            people: Vec::new(),
            chapters: Vec::new(),
        }
    }

    #[test]
    fn details_artwork_addresses_carry_no_token() {
        let session = crate::model::review_session();
        let urls = session.artwork();
        let mut episode = titled(ItemKind::Episode);
        episode.artwork.primary = ImageTag::parse("still");
        episode.hierarchy.series_id = ItemId::parse("series-1").ok();
        // No episode backdrop: the series backdrop stands in.
        let backdrop = backdrop_request(&episode, &urls).unwrap();
        assert!(backdrop.url.contains("/Items/series-1/Images/Backdrop"));
        let still = hero_art_request(&episode, &urls).unwrap();
        assert!(still.url.contains(&format!("maxWidth={THUMB_WIDTH}")));
        for request in [backdrop, still] {
            assert!(!request.url.contains("api_key"), "{}", request.url);
            assert!(
                !request.url.contains(crate::test_support::FIXTURE_TOKEN),
                "{}",
                request.url
            );
        }
        let bare = titled(ItemKind::Movie);
        assert!(
            backdrop_request(&bare, &urls).is_none(),
            "no art is not a request"
        );
        assert!(hero_art_request(&bare, &urls).is_none());
    }
}
