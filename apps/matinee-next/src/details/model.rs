//! Details state. No GPUI, no HTTP.
//!
//! The screen turns each [`Request`] into service-runtime work and hands the
//! answer back with the [`Ticket`] it was issued under. An answer whose ticket
//! is no longer the section's current one is ignored, which is how a late
//! response for a previous title, or a previous season, never overwrites the
//! current one.

use std::time::Duration;

use matinee_core::{CollectionContext, Credit, ItemId, ItemKind, MediaItem, Person};
use matinee_jellyfin::JellyfinError;

use crate::player::{format_clock, resume_start};

/// Shown when Jellyfin has no overview.
pub(crate) const NO_OVERVIEW: &str = "No overview is available for this title.";

/// Cast members shown in the hero's lower section.
pub(crate) const CAST_LIMIT: usize = 10;

/// Related titles shown per row.
pub(crate) const RELATED_LIMIT: usize = 12;

/// Identifies one request. Only the latest ticket for a section applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ticket(u64);

/// Why the title itself could not be shown. The copy is fixed; server text,
/// URLs, and tokens are never painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DetailsFailure {
    NotFound,
    SignedOut,
    Unreachable,
    Unreadable,
}

impl DetailsFailure {
    pub(crate) fn from_error(error: &JellyfinError) -> Self {
        match error {
            JellyfinError::NotFound => Self::NotFound,
            JellyfinError::Unauthorized | JellyfinError::AuthRejected => Self::SignedOut,
            JellyfinError::Unreachable { .. } | JellyfinError::Cancelled => Self::Unreachable,
            _ => Self::Unreadable,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::NotFound => "This title isn't in your library",
            Self::SignedOut => "Your Jellyfin session has ended",
            Self::Unreachable => "Jellyfin isn't answering",
            Self::Unreadable => "This title couldn't be opened",
        }
    }

    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::NotFound => {
                "Jellyfin has no item with that ID. It may have been removed, or the ID may belong to another server."
            }
            Self::SignedOut => "Sign out and sign in again to keep browsing.",
            Self::Unreachable => {
                "Check that the server is running and reachable from this computer, then try again."
            }
            Self::Unreadable => "Jellyfin sent a response Matinee could not read. Try again.",
        }
    }
}

/// A secondary section. Its failure never takes the hero down with it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Section<T> {
    Loading,
    Ready(T),
    Unavailable,
}

impl<T> Section<T> {
    pub(crate) fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }
}

/// The title the screen is about.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Hero {
    Loading,
    Ready(Box<MediaItem>),
    Failed(DetailsFailure),
}

/// Work the screen runs on the service runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    Item {
        item: ItemId,
        ticket: Ticket,
    },
    /// Similar titles and collection membership, fetched together.
    Related {
        item: ItemId,
        ticket: Ticket,
    },
    /// Seasons and the next-up episode, fetched together.
    Outline {
        series: ItemId,
        ticket: Ticket,
    },
    Episodes {
        series: ItemId,
        season: ItemId,
        ticket: Ticket,
    },
}

impl Request {
    pub(crate) fn ticket(&self) -> Ticket {
        match self {
            Self::Item { ticket, .. }
            | Self::Related { ticket, .. }
            | Self::Outline { ticket, .. }
            | Self::Episodes { ticket, .. } => *ticket,
        }
    }
}

/// Answers, already mapped out of the Jellyfin client.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Response {
    Item(Result<MediaItem, DetailsFailure>),
    Related {
        similar: Option<Vec<MediaItem>>,
        collections: Option<Vec<CollectionContext>>,
    },
    Outline {
        seasons: Option<Vec<MediaItem>>,
        next_up: Option<MediaItem>,
    },
    Episodes(Option<Vec<MediaItem>>),
}

/// What Play does and what its button says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlayAction {
    pub target: ItemId,
    /// Present when the Player will resume, following its own resume rule.
    pub resume: Option<Duration>,
    /// `S1 E2` when the action plays an episode from a series page.
    pub episode: Option<String>,
}

impl PlayAction {
    pub(crate) fn for_item(item: &MediaItem) -> Self {
        let resume = item
            .user
            .resume_position()
            .and_then(|position| resume_start(position, item.metadata.runtime));
        Self {
            target: item.id().clone(),
            resume,
            episode: None,
        }
    }

    pub(crate) fn label(&self) -> String {
        match (&self.episode, self.resume) {
            (None, Some(at)) => format!("Resume · {}", format_clock(at)),
            (None, None) => "Play".into(),
            (Some(episode), Some(at)) => format!("Resume {episode} · {}", format_clock(at)),
            (Some(episode), None) => format!("Play {episode}"),
        }
    }
}

pub(crate) struct DetailsModel {
    item_id: ItemId,
    issued: u64,
    hero: Hero,
    hero_ticket: Option<Ticket>,
    similar: Section<Vec<MediaItem>>,
    collections: Section<Vec<CollectionContext>>,
    related_ticket: Option<Ticket>,
    seasons: Section<Vec<MediaItem>>,
    next_up: Option<MediaItem>,
    outline_ticket: Option<Ticket>,
    selected_season: Option<ItemId>,
    episodes: Section<Vec<MediaItem>>,
    episodes_ticket: Option<Ticket>,
}

impl DetailsModel {
    /// A model for `item_id` and the request that loads it.
    pub(crate) fn open(item_id: ItemId) -> (Self, Vec<Request>) {
        let mut model = Self {
            item_id: item_id.clone(),
            issued: 0,
            hero: Hero::Loading,
            hero_ticket: None,
            similar: Section::Loading,
            collections: Section::Loading,
            related_ticket: None,
            seasons: Section::Loading,
            next_up: None,
            outline_ticket: None,
            selected_season: None,
            episodes: Section::Loading,
            episodes_ticket: None,
        };
        let requests = model.show(item_id);
        (model, requests)
    }

    /// Replace the title on screen, for a related title chosen here. Every
    /// section starts over, and answers for the previous title are ignored.
    pub(crate) fn show(&mut self, item_id: ItemId) -> Vec<Request> {
        self.item_id = item_id;
        self.hero = Hero::Loading;
        self.similar = Section::Loading;
        self.collections = Section::Loading;
        self.related_ticket = None;
        self.seasons = Section::Loading;
        self.next_up = None;
        self.outline_ticket = None;
        self.selected_season = None;
        self.episodes = Section::Loading;
        self.episodes_ticket = None;
        vec![self.request_item()]
    }

    /// Reload what playback changes: the title's user data and, for a series,
    /// next up and the visible season. What is on screen stays until the new
    /// answers arrive, so returning from the Player does not flash.
    pub(crate) fn refresh(&mut self) -> Vec<Request> {
        let mut requests = vec![self.request_item()];
        if self.is_series() && self.seasons.ready().is_some() {
            requests.push(self.request_outline());
            if let Some(season) = self.selected_season.clone() {
                requests.push(self.request_episodes(season));
            }
        }
        requests
    }

    /// Try the title again after a failure.
    pub(crate) fn retry(&mut self) -> Vec<Request> {
        let id = self.item_id.clone();
        self.show(id)
    }

    pub(crate) fn select_season(&mut self, season: ItemId) -> Vec<Request> {
        let unchanged = self.selected_season.as_ref() == Some(&season)
            && !matches!(self.episodes, Section::Unavailable);
        if unchanged || self.seasons.ready().is_none() {
            return Vec::new();
        }
        self.episodes = Section::Loading;
        vec![self.request_episodes(season)]
    }

    pub(crate) fn apply(&mut self, ticket: Ticket, response: Response) -> Vec<Request> {
        match response {
            Response::Item(result) => self.apply_item(ticket, result),
            Response::Related {
                similar,
                collections,
            } => {
                if self.related_ticket != Some(ticket) {
                    return Vec::new();
                }
                self.related_ticket = None;
                let own = self.item_id.clone();
                self.similar = match similar {
                    Some(items) => Section::Ready(
                        items
                            .into_iter()
                            .filter(|item| item.id() != &own)
                            .take(RELATED_LIMIT)
                            .collect(),
                    ),
                    None => Section::Unavailable,
                };
                self.collections = match collections {
                    Some(contexts) => Section::Ready(
                        contexts
                            .into_iter()
                            .map(|mut context| {
                                context.items.retain(|item| item.id() != &own);
                                context.items.truncate(RELATED_LIMIT);
                                context
                            })
                            .filter(|context| !context.items.is_empty())
                            .collect(),
                    ),
                    None => Section::Unavailable,
                };
                Vec::new()
            }
            Response::Outline { seasons, next_up } => {
                if self.outline_ticket != Some(ticket) {
                    return Vec::new();
                }
                self.outline_ticket = None;
                self.next_up = next_up;
                let Some(seasons) = seasons else {
                    if self.seasons.ready().is_none() {
                        self.seasons = Section::Unavailable;
                        self.episodes = Section::Unavailable;
                    }
                    return Vec::new();
                };
                let keep = self
                    .selected_season
                    .as_ref()
                    .filter(|selected| seasons.iter().any(|season| season.id() == *selected))
                    .cloned();
                let choice = keep.clone().or_else(|| {
                    let next_up_season = self
                        .next_up
                        .as_ref()
                        .and_then(|episode| episode.hierarchy.season_id.clone())
                        .filter(|id| seasons.iter().any(|season| season.id() == id));
                    next_up_season.or_else(|| seasons.first().map(|season| season.id().clone()))
                });
                self.seasons = Section::Ready(seasons);
                match (keep, choice) {
                    // A refresh keeps the season the viewer chose.
                    (Some(_), _) => Vec::new(),
                    (None, Some(season)) => {
                        self.episodes = Section::Loading;
                        vec![self.request_episodes(season)]
                    }
                    (None, None) => {
                        self.selected_season = None;
                        self.episodes = Section::Ready(Vec::new());
                        Vec::new()
                    }
                }
            }
            Response::Episodes(episodes) => {
                if self.episodes_ticket != Some(ticket) {
                    return Vec::new();
                }
                self.episodes_ticket = None;
                match episodes {
                    Some(episodes) => self.episodes = Section::Ready(episodes),
                    // A failed refresh keeps the episodes already shown.
                    None if self.episodes.ready().is_none() => {
                        self.episodes = Section::Unavailable;
                    }
                    None => {}
                }
                Vec::new()
            }
        }
    }

    fn apply_item(
        &mut self,
        ticket: Ticket,
        result: Result<MediaItem, DetailsFailure>,
    ) -> Vec<Request> {
        if self.hero_ticket != Some(ticket) {
            return Vec::new();
        }
        self.hero_ticket = None;
        match result {
            Ok(item) => {
                let first = !matches!(self.hero, Hero::Ready(_));
                let series = item.kind == ItemKind::Series;
                self.hero = Hero::Ready(Box::new(item));
                match (first, series) {
                    (false, _) => Vec::new(),
                    (true, true) => vec![self.request_outline()],
                    (true, false) => vec![self.request_related()],
                }
            }
            // A failed refresh keeps the title already on screen.
            Err(_) if matches!(self.hero, Hero::Ready(_)) => Vec::new(),
            Err(failure) => {
                self.hero = Hero::Failed(failure);
                Vec::new()
            }
        }
    }

    fn ticket(&mut self) -> Ticket {
        self.issued += 1;
        Ticket(self.issued)
    }

    fn request_item(&mut self) -> Request {
        let ticket = self.ticket();
        self.hero_ticket = Some(ticket);
        Request::Item {
            item: self.item_id.clone(),
            ticket,
        }
    }

    fn request_related(&mut self) -> Request {
        let ticket = self.ticket();
        self.related_ticket = Some(ticket);
        Request::Related {
            item: self.item_id.clone(),
            ticket,
        }
    }

    fn request_outline(&mut self) -> Request {
        let ticket = self.ticket();
        self.outline_ticket = Some(ticket);
        Request::Outline {
            series: self.item_id.clone(),
            ticket,
        }
    }

    fn request_episodes(&mut self, season: ItemId) -> Request {
        let ticket = self.ticket();
        self.selected_season = Some(season.clone());
        self.episodes_ticket = Some(ticket);
        Request::Episodes {
            series: self.item_id.clone(),
            season,
            ticket,
        }
    }

    pub(crate) fn item_id(&self) -> &ItemId {
        &self.item_id
    }

    pub(crate) fn hero(&self) -> &Hero {
        &self.hero
    }

    pub(crate) fn item(&self) -> Option<&MediaItem> {
        match &self.hero {
            Hero::Ready(item) => Some(item),
            _ => None,
        }
    }

    pub(crate) fn is_series(&self) -> bool {
        self.item()
            .is_some_and(|item| item.kind == ItemKind::Series)
    }

    pub(crate) fn similar(&self) -> &Section<Vec<MediaItem>> {
        &self.similar
    }

    pub(crate) fn collections(&self) -> &Section<Vec<CollectionContext>> {
        &self.collections
    }

    pub(crate) fn seasons(&self) -> &Section<Vec<MediaItem>> {
        &self.seasons
    }

    pub(crate) fn selected_season(&self) -> Option<&ItemId> {
        self.selected_season.as_ref()
    }

    pub(crate) fn episodes(&self) -> &Section<Vec<MediaItem>> {
        &self.episodes
    }

    pub(crate) fn next_up(&self) -> Option<&MediaItem> {
        self.next_up.as_ref()
    }

    /// The hero's Play or Resume. A series plays its next-up episode and has
    /// no primary action when Jellyfin has none. Seasons and collections are
    /// not played from here.
    pub(crate) fn primary(&self) -> Option<PlayAction> {
        let item = self.item()?;
        match item.kind {
            ItemKind::Series => {
                let episode = self.next_up.as_ref()?;
                Some(PlayAction {
                    episode: episode.hierarchy.episode_label(),
                    ..PlayAction::for_item(episode)
                })
            }
            ItemKind::Season | ItemKind::Collection => None,
            _ => Some(PlayAction::for_item(item)),
        }
    }
}

/// `2h 4m`, `48m`.
pub(crate) fn runtime_label(runtime: Duration) -> Option<String> {
    let minutes = (runtime.as_secs() + 30) / 60;
    match minutes {
        0 => None,
        1..=59 => Some(format!("{minutes}m")),
        _ if minutes.is_multiple_of(60) => Some(format!("{}h", minutes / 60)),
        _ => Some(format!("{}h {}m", minutes / 60, minutes % 60)),
    }
}

/// Year, content rating, and runtime: `2019 · PG-13 · 2h 4m`.
pub(crate) fn meta_line(item: &MediaItem) -> String {
    let mut parts = Vec::new();
    if let Some(year) = item.metadata.year {
        parts.push(year.to_string());
    }
    if let Some(rating) = item
        .metadata
        .official_rating
        .as_deref()
        .map(str::trim)
        .filter(|rating| !rating.is_empty())
    {
        parts.push(rating.to_string());
    }
    if item.kind != ItemKind::Series
        && let Some(runtime) = item.metadata.runtime.and_then(runtime_label)
    {
        parts.push(runtime);
    }
    parts.join(" · ")
}

/// Audience and critic scores in the shipping wording.
pub(crate) fn score_line(item: &MediaItem) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(score) = item.metadata.community_rating.filter(|score| *score > 0.0) {
        parts.push(format!("{}% audience", (score * 10.0).round() as i64));
    }
    if let Some(score) = item.metadata.critic_rating.filter(|score| *score > 0.0) {
        parts.push(format!("{}% critics", score.round() as i64));
    }
    (!parts.is_empty()).then(|| parts.join("   "))
}

pub(crate) fn genre_line(item: &MediaItem) -> Option<String> {
    let genres: Vec<&str> = item
        .metadata
        .genres
        .iter()
        .take(3)
        .map(String::as_str)
        .collect();
    (!genres.is_empty()).then(|| genres.join(" / "))
}

/// Watched fraction for a quiet progress line: the server's percentage, or
/// resume position over runtime.
pub(crate) fn progress_fraction(item: &MediaItem) -> Option<f32> {
    let progress = item.user.viewing_progress()?;
    progress.fraction.or_else(|| {
        let position = progress.position?;
        let runtime = item.metadata.runtime.filter(|runtime| !runtime.is_zero())?;
        Some((position.as_secs_f32() / runtime.as_secs_f32()).clamp(0.0, 1.0))
    })
}

/// Names for one credit, in server order.
pub(crate) fn credited(item: &MediaItem, credit: &Credit, limit: usize) -> Vec<String> {
    item.people
        .iter()
        .filter(|person| &person.credit == credit)
        .take(limit)
        .map(|person| person.name.clone())
        .collect()
}

pub(crate) fn cast(item: &MediaItem) -> Vec<&Person> {
    item.people
        .iter()
        .filter(|person| person.credit == Credit::Actor)
        .take(CAST_LIMIT)
        .collect()
}

/// The line above the title: `Movie`, `Series · 3 seasons`, `Harbor Lights · S2 E5`.
pub(crate) fn eyebrow(item: &MediaItem, seasons: Option<usize>) -> String {
    match &item.kind {
        ItemKind::Series => match seasons {
            Some(1) => "Series · 1 season".into(),
            Some(count) if count > 1 => format!("Series · {count} seasons"),
            _ => "Series".into(),
        },
        ItemKind::Episode => {
            let series = item
                .hierarchy
                .series_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty());
            match (series, item.hierarchy.episode_label()) {
                (Some(series), Some(label)) => format!("{series} · {label}"),
                (Some(series), None) => series.to_string(),
                (None, Some(label)) => format!("Episode · {label}"),
                (None, None) => "Episode".into(),
            }
        }
        kind => kind.label().to_string(),
    }
}

/// Shorten at a word boundary for list rows. The hero shows the full text.
pub(crate) fn summary(text: &str, limit: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let cut: String = text.chars().take(limit).collect();
    let cut = cut
        .rsplit_once(char::is_whitespace)
        .map(|(head, _)| head)
        .unwrap_or(&cut);
    format!("{}…", cut.trim_end_matches([',', ';', ':', '.', ' ']))
}

/// `E3 · 42m`, or `E3` without a runtime.
pub(crate) fn episode_heading(episode: &MediaItem) -> String {
    let number = episode
        .hierarchy
        .index
        .map(|index| format!("E{index}"))
        .unwrap_or_else(|| "Special".into());
    match episode.metadata.runtime.and_then(runtime_label) {
        Some(runtime) => format!("{number} · {runtime}"),
        None => number,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matinee_core::{ItemHierarchy, ItemIdentity, ItemMetadata, TechnicalMedia, UserItemState};

    fn id(value: &str) -> ItemId {
        ItemId::parse(value).unwrap()
    }

    pub(crate) fn item(value: &str, kind: ItemKind) -> MediaItem {
        MediaItem {
            identity: ItemIdentity {
                id: id(value),
                name: value.to_string(),
            },
            kind,
            metadata: ItemMetadata {
                runtime: Some(Duration::from_secs(2 * 3600 + 4 * 60)),
                ..ItemMetadata::default()
            },
            artwork: Default::default(),
            user: UserItemState::default(),
            hierarchy: ItemHierarchy::default(),
            media: TechnicalMedia::default(),
            people: Vec::new(),
            chapters: Vec::new(),
        }
    }

    fn with_progress(mut item: MediaItem, position: Duration) -> MediaItem {
        item.user = UserItemState::from_parts(Some(position), None, false, false, 0);
        item
    }

    fn episode(value: &str, season: &str, index: u32) -> MediaItem {
        let mut episode = item(value, ItemKind::Episode);
        episode.hierarchy.season_id = Some(id(season));
        episode.hierarchy.parent_index = Some(1);
        episode.hierarchy.index = Some(index);
        episode.metadata.runtime = Some(Duration::from_secs(42 * 60));
        episode
    }

    fn ticket_of(request: &Request) -> Ticket {
        request.ticket()
    }

    /// Open a movie and answer every request.
    fn ready_movie(movie: MediaItem) -> DetailsModel {
        let (mut model, requests) = DetailsModel::open(movie.id().clone());
        let follow = model.apply(ticket_of(&requests[0]), Response::Item(Ok(movie)));
        assert!(matches!(follow[0], Request::Related { .. }));
        model.apply(
            ticket_of(&follow[0]),
            Response::Related {
                similar: Some(Vec::new()),
                collections: Some(Vec::new()),
            },
        );
        model
    }

    #[test]
    fn an_item_id_loads_then_becomes_ready() {
        let (mut model, requests) = DetailsModel::open(id("movie-1"));
        assert_eq!(model.hero(), &Hero::Loading);
        assert!(
            matches!(&requests[..], [Request::Item { item, .. }] if item.as_str() == "movie-1")
        );
        let follow = model.apply(
            ticket_of(&requests[0]),
            Response::Item(Ok(item("movie-1", ItemKind::Movie))),
        );
        assert!(matches!(model.hero(), Hero::Ready(_)));
        assert!(matches!(&follow[..], [Request::Related { .. }]));
        assert_eq!(
            model.similar(),
            &Section::Loading,
            "secondary sections load after the hero"
        );
    }

    #[test]
    fn a_failed_item_request_is_a_designed_failure() {
        let (mut model, requests) = DetailsModel::open(id("gone"));
        let follow = model.apply(
            ticket_of(&requests[0]),
            Response::Item(Err(DetailsFailure::from_error(&JellyfinError::NotFound))),
        );
        assert!(follow.is_empty());
        assert_eq!(model.hero(), &Hero::Failed(DetailsFailure::NotFound));
        assert!(!DetailsFailure::NotFound.message().contains("http"));
        assert!(model.primary().is_none());
        let retry = model.retry();
        assert_eq!(model.hero(), &Hero::Loading);
        assert!(matches!(&retry[..], [Request::Item { .. }]));
    }

    #[test]
    fn progress_makes_the_primary_action_resume() {
        let movie = with_progress(
            item("movie-1", ItemKind::Movie),
            Duration::from_secs(18 * 60 + 21),
        );
        let model = ready_movie(movie);
        let action = model.primary().unwrap();
        assert_eq!(action.resume, Some(Duration::from_secs(18 * 60 + 21)));
        assert_eq!(action.label(), "Resume · 18:21");
        assert_eq!(action.target.as_str(), "movie-1");
    }

    #[test]
    fn a_fresh_or_finished_title_plays() {
        let fresh = ready_movie(item("movie-1", ItemKind::Movie));
        assert_eq!(fresh.primary().unwrap().label(), "Play");
        // Inside the Player's 30-second completion tail, the Player starts over.
        let runtime = Duration::from_secs(2 * 3600 + 4 * 60);
        let tail = ready_movie(with_progress(
            item("movie-1", ItemKind::Movie),
            runtime - Duration::from_secs(10),
        ));
        assert_eq!(tail.primary().unwrap().label(), "Play");
    }

    #[test]
    fn returning_from_the_player_shows_the_new_position() {
        let mut model = ready_movie(with_progress(
            item("movie-1", ItemKind::Movie),
            Duration::from_secs(18 * 60 + 21),
        ));
        let requests = model.refresh();
        assert!(matches!(&requests[..], [Request::Item { .. }]));
        assert_eq!(
            model.primary().unwrap().label(),
            "Resume · 18:21",
            "the old position stays on screen until the answer arrives"
        );
        let watched = with_progress(
            item("movie-1", ItemKind::Movie),
            Duration::from_secs(38 * 60 + 5),
        );
        let follow = model.apply(ticket_of(&requests[0]), Response::Item(Ok(watched)));
        assert!(
            follow.is_empty(),
            "a refresh does not reload related titles"
        );
        assert_eq!(model.primary().unwrap().label(), "Resume · 38:05");
    }

    #[test]
    fn a_failed_refresh_keeps_the_title() {
        let mut model = ready_movie(item("movie-1", ItemKind::Movie));
        let requests = model.refresh();
        model.apply(
            ticket_of(&requests[0]),
            Response::Item(Err(DetailsFailure::Unreachable)),
        );
        assert!(model.item().is_some());
    }

    #[test]
    fn a_series_loads_seasons_then_the_next_up_season() {
        let series = item("series-1", ItemKind::Series);
        let (mut model, requests) = DetailsModel::open(series.id().clone());
        let follow = model.apply(ticket_of(&requests[0]), Response::Item(Ok(series)));
        assert!(matches!(&follow[..], [Request::Outline { .. }]));
        let next_up = with_progress(
            episode("ep-2-3", "season-2", 3),
            Duration::from_secs(12 * 60 + 3),
        );
        let follow = model.apply(
            ticket_of(&follow[0]),
            Response::Outline {
                seasons: Some(vec![
                    item("season-1", ItemKind::Season),
                    item("season-2", ItemKind::Season),
                ]),
                next_up: Some(next_up),
            },
        );
        assert!(
            matches!(&follow[..], [Request::Episodes { season, .. }] if season.as_str() == "season-2"),
            "the next-up season opens first"
        );
        assert_eq!(model.selected_season().unwrap().as_str(), "season-2");
        assert_eq!(model.primary().unwrap().label(), "Resume S1 E3 · 12:03");
        assert_eq!(model.primary().unwrap().target.as_str(), "ep-2-3");

        model.apply(
            ticket_of(&follow[0]),
            Response::Episodes(Some(vec![episode("ep-2-1", "season-2", 1)])),
        );
        assert_eq!(model.episodes().ready().unwrap().len(), 1);

        // Choosing another season loads its episodes.
        let follow = model.select_season(id("season-1"));
        assert_eq!(model.episodes(), &Section::Loading);
        model.apply(
            ticket_of(&follow[0]),
            Response::Episodes(Some(vec![
                episode("ep-1-1", "season-1", 1),
                episode("ep-1-2", "season-1", 2),
            ])),
        );
        assert_eq!(model.episodes().ready().unwrap().len(), 2);
        assert!(
            model.select_season(id("season-1")).is_empty(),
            "same season, no reload"
        );
    }

    #[test]
    fn a_late_season_answer_does_not_replace_the_chosen_one() {
        let series = item("series-1", ItemKind::Series);
        let (mut model, requests) = DetailsModel::open(series.id().clone());
        let follow = model.apply(ticket_of(&requests[0]), Response::Item(Ok(series)));
        let follow = model.apply(
            ticket_of(&follow[0]),
            Response::Outline {
                seasons: Some(vec![
                    item("season-1", ItemKind::Season),
                    item("season-2", ItemKind::Season),
                ]),
                next_up: None,
            },
        );
        let first = ticket_of(&follow[0]);
        let second = ticket_of(&model.select_season(id("season-2"))[0]);
        model.apply(
            second,
            Response::Episodes(Some(vec![episode("ep-2-1", "season-2", 1)])),
        );
        model.apply(
            first,
            Response::Episodes(Some(vec![episode("ep-1-1", "season-1", 1)])),
        );
        let shown = model.episodes().ready().unwrap();
        assert_eq!(shown[0].id().as_str(), "ep-2-1");
    }

    #[test]
    fn a_late_answer_for_a_previous_title_is_ignored() {
        let (mut model, requests) = DetailsModel::open(id("movie-a"));
        let stale = ticket_of(&requests[0]);
        let requests = model.show(id("movie-b"));
        model.apply(stale, Response::Item(Ok(item("movie-a", ItemKind::Movie))));
        assert_eq!(
            model.hero(),
            &Hero::Loading,
            "A's answer does not land on B"
        );
        model.apply(
            ticket_of(&requests[0]),
            Response::Item(Ok(item("movie-b", ItemKind::Movie))),
        );
        assert_eq!(model.item().unwrap().id().as_str(), "movie-b");
    }

    #[test]
    fn a_secondary_failure_leaves_the_hero_usable() {
        let movie = item("movie-1", ItemKind::Movie);
        let (mut model, requests) = DetailsModel::open(movie.id().clone());
        let follow = model.apply(ticket_of(&requests[0]), Response::Item(Ok(movie)));
        model.apply(
            ticket_of(&follow[0]),
            Response::Related {
                similar: None,
                collections: Some(Vec::new()),
            },
        );
        assert_eq!(model.similar(), &Section::Unavailable);
        assert_eq!(model.collections(), &Section::Ready(Vec::new()));
        assert!(model.item().is_some());
        assert_eq!(model.primary().unwrap().label(), "Play");

        let series = item("series-1", ItemKind::Series);
        let (mut model, requests) = DetailsModel::open(series.id().clone());
        let follow = model.apply(ticket_of(&requests[0]), Response::Item(Ok(series)));
        model.apply(
            ticket_of(&follow[0]),
            Response::Outline {
                seasons: None,
                next_up: None,
            },
        );
        assert_eq!(model.seasons(), &Section::Unavailable);
        assert!(model.item().is_some());
    }

    #[test]
    fn related_titles_leave_out_the_title_itself() {
        let movie = item("movie-1", ItemKind::Movie);
        let (mut model, requests) = DetailsModel::open(movie.id().clone());
        let follow = model.apply(ticket_of(&requests[0]), Response::Item(Ok(movie.clone())));
        model.apply(
            ticket_of(&follow[0]),
            Response::Related {
                similar: Some(vec![movie.clone(), item("movie-2", ItemKind::Movie)]),
                collections: Some(vec![
                    CollectionContext {
                        collection: item("saga", ItemKind::Collection),
                        items: vec![movie.clone(), item("movie-3", ItemKind::Movie)],
                    },
                    CollectionContext {
                        collection: item("alone", ItemKind::Collection),
                        items: vec![movie],
                    },
                ]),
            },
        );
        assert_eq!(model.similar().ready().unwrap().len(), 1);
        let collections = model.collections().ready().unwrap();
        assert_eq!(
            collections.len(),
            1,
            "a collection of only this title is hidden"
        );
        assert_eq!(collections[0].items[0].id().as_str(), "movie-3");
    }

    #[test]
    fn labels_follow_the_shipping_wording() {
        assert_eq!(
            runtime_label(Duration::from_secs(124 * 60)).as_deref(),
            Some("2h 4m")
        );
        assert_eq!(
            runtime_label(Duration::from_secs(42 * 60)).as_deref(),
            Some("42m")
        );
        assert_eq!(
            runtime_label(Duration::from_secs(120 * 60)).as_deref(),
            Some("2h")
        );
        assert_eq!(runtime_label(Duration::ZERO), None);
        let mut movie = item("movie-1", ItemKind::Movie);
        movie.metadata.year = Some(2019);
        movie.metadata.official_rating = Some("PG-13".into());
        movie.metadata.community_rating = Some(7.8);
        movie.metadata.critic_rating = Some(91.0);
        movie.metadata.genres = vec![
            "Drama".into(),
            "Mystery".into(),
            "Thriller".into(),
            "War".into(),
        ];
        assert_eq!(meta_line(&movie), "2019 · PG-13 · 2h 4m");
        assert_eq!(
            score_line(&movie).as_deref(),
            Some("78% audience   91% critics")
        );
        assert_eq!(
            genre_line(&movie).as_deref(),
            Some("Drama / Mystery / Thriller")
        );
        assert_eq!(episode_heading(&episode("ep", "s", 3)), "E3 · 42m");
        assert_eq!(summary("A short line.", 40), "A short line.");
        assert_eq!(summary("One two three four five", 12), "One two…");
        let mut ep = episode("ep", "s", 5);
        ep.hierarchy.series_name = Some("Harbor Lights".into());
        ep.hierarchy.parent_index = Some(2);
        assert_eq!(eyebrow(&ep, None), "Harbor Lights · S2 E5");
        assert_eq!(
            eyebrow(&item("s", ItemKind::Series), Some(3)),
            "Series · 3 seasons"
        );
        let halfway = with_progress(
            item("movie-1", ItemKind::Movie),
            Duration::from_secs(62 * 60),
        );
        assert!((progress_fraction(&halfway).unwrap() - 0.5).abs() < 0.01);
    }
}
