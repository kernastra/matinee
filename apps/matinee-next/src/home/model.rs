//! Home state. No GPUI, no HTTP.
//!
//! Home is five shelves, each loaded by its own request. The screen runs
//! every [`Request`] on the service runtime and hands the answer back with
//! the [`Ticket`] it was issued under. A shelf applies only the ticket it is
//! waiting for, so a late answer from an earlier refresh never replaces a
//! newer one. One shelf failing leaves the others alone.
//!
//! The hero is chosen from the shelves in a fixed order and then pinned by
//! item id, so it does not change while sections arrive or on a refresh that
//! still contains it.

use std::time::Duration;

use matinee_core::{HomeShelf, ItemId, ItemKind, MediaItem};
use matinee_jellyfin::JellyfinError;

use crate::details::{PlayAction, runtime_label};

/// Items shown per shelf. The server is asked for the same number.
pub(crate) const SHELF_LIMIT: usize = 12;

/// Shelves the hero may come from, highest priority first.
const HERO_ORDER: [HomeShelf; 4] = [
    HomeShelf::ContinueWatching,
    HomeShelf::NextUp,
    HomeShelf::RecentMovies,
    HomeShelf::RecentSeries,
];

/// Identifies one request. Only the latest ticket for a shelf applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ticket(u64);

/// Why a shelf, or all of Home, could not be loaded. Copy is fixed; no
/// server text, URL, or token is painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HomeFailure {
    SignedOut,
    Unreachable,
    Unreadable,
}

impl HomeFailure {
    pub(crate) fn from_error(error: &JellyfinError) -> Self {
        match error {
            JellyfinError::Unauthorized | JellyfinError::AuthRejected => Self::SignedOut,
            JellyfinError::Unreachable { .. } | JellyfinError::Cancelled => Self::Unreachable,
            _ => Self::Unreadable,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::SignedOut => "Your Jellyfin session has ended",
            Self::Unreachable => "Jellyfin isn't answering",
            Self::Unreadable => "Home couldn't be loaded",
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

/// One shelf. Each is independent of the others.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ShelfState {
    Loading,
    Ready(Vec<MediaItem>),
    Failed(HomeFailure),
}

/// Work the screen runs on the service runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub shelf: HomeShelf,
    pub ticket: Ticket,
}

/// A shelf's answer, already mapped out of the Jellyfin client.
pub(crate) type Response = Result<Vec<MediaItem>, HomeFailure>;

/// What applying an answer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Applied {
    /// A late or unknown ticket. Nothing changed.
    Ignored,
    Updated,
    /// Jellyfin no longer accepts the session. The application decides what
    /// that means; Home only reports it.
    SessionExpired,
}

/// The page as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageState {
    /// At least one shelf is loading or has something to show.
    Content,
    /// Every shelf answered and none has anything: an empty library.
    Empty,
    /// Every shelf failed. The page offers Try again.
    Failed(HomeFailure),
}

/// The hero, once chosen.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HeroPick {
    pub item: MediaItem,
    pub shelf: HomeShelf,
}

impl HeroPick {
    /// The small line above the title.
    pub(crate) fn eyebrow(&self) -> String {
        match self.shelf {
            HomeShelf::ContinueWatching => "Continue watching".into(),
            HomeShelf::NextUp => match self.item.hierarchy.episode_label() {
                Some(label) => format!("Up next · {label}"),
                None => "Up next".into(),
            },
            _ => "Tonight's feature".into(),
        }
    }

    /// The hero's title: the series for an episode, so the episode name can
    /// sit underneath it.
    pub(crate) fn title(&self) -> &str {
        card_title(&self.item)
    }

    /// Play or Resume, opening the existing Player. A series has none; its
    /// Details chooses the episode.
    pub(crate) fn play(&self) -> Option<PlayAction> {
        match self.item.kind {
            ItemKind::Series | ItemKind::Season | ItemKind::Collection => None,
            _ => Some(PlayAction::for_item(&self.item)),
        }
    }
}

/// What the hero area shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum HeroState {
    /// A higher-priority shelf has not answered yet.
    Loading,
    Ready(Box<HeroPick>),
    /// Nothing to feature.
    None,
}

/// Where focus should go when Home is shown again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FocusTarget {
    Card(HomeShelf, ItemId),
    Hero,
}

struct Slot {
    shelf: HomeShelf,
    state: ShelfState,
    ticket: Option<Ticket>,
}

pub(crate) struct HomeModel {
    issued: u64,
    slots: Vec<Slot>,
    /// The featured item, pinned once chosen.
    hero: Option<(HomeShelf, ItemId)>,
    /// The card most recently opened, for focus on return.
    opened: Option<(HomeShelf, ItemId)>,
}

impl HomeModel {
    /// A loading Home and one request per shelf, all at once.
    pub(crate) fn open() -> (Self, Vec<Request>) {
        let mut model = Self {
            issued: 0,
            slots: HomeShelf::ALL
                .iter()
                .map(|shelf| Slot {
                    shelf: *shelf,
                    state: ShelfState::Loading,
                    ticket: None,
                })
                .collect(),
            hero: None,
            opened: None,
        };
        let requests = model.refresh();
        (model, requests)
    }

    /// Ask every shelf again. What is on screen stays until each answer
    /// arrives, so a refresh does not flash.
    pub(crate) fn refresh(&mut self) -> Vec<Request> {
        let shelves: Vec<HomeShelf> = self.slots.iter().map(|slot| slot.shelf).collect();
        shelves
            .into_iter()
            .map(|shelf| self.request(shelf))
            .collect()
    }

    /// Try again after Home failed as a whole: back to loading, then refresh.
    pub(crate) fn retry(&mut self) -> Vec<Request> {
        for slot in &mut self.slots {
            slot.state = ShelfState::Loading;
        }
        self.hero = None;
        self.refresh()
    }

    /// Ask one shelf again, after its own failure.
    pub(crate) fn retry_shelf(&mut self, shelf: HomeShelf) -> Vec<Request> {
        let slot = self.slot_mut(shelf);
        if !matches!(slot.state, ShelfState::Failed(_)) {
            return Vec::new();
        }
        slot.state = ShelfState::Loading;
        vec![self.request(shelf)]
    }

    pub(crate) fn apply(&mut self, request: Request, response: Response) -> Applied {
        let slot = self.slot_mut(request.shelf);
        if slot.ticket != Some(request.ticket) {
            return Applied::Ignored;
        }
        slot.ticket = None;
        let expired = response == Err(HomeFailure::SignedOut);
        match response {
            Ok(mut items) => {
                items.truncate(SHELF_LIMIT);
                slot.state = ShelfState::Ready(items);
            }
            // A failed refresh keeps the shelf already on screen.
            Err(_) if matches!(slot.state, ShelfState::Ready(_)) => {}
            Err(failure) => slot.state = ShelfState::Failed(failure),
        }
        self.settle_hero();
        if expired {
            Applied::SessionExpired
        } else {
            Applied::Updated
        }
    }

    /// Whether any shelf is waiting for an answer.
    pub(crate) fn is_loading(&self) -> bool {
        self.slots.iter().any(|slot| slot.ticket.is_some())
    }

    pub(crate) fn shelf(&self, shelf: HomeShelf) -> &ShelfState {
        &self.slot(shelf).state
    }

    /// The items a shelf shows. Next Up leaves out anything Continue
    /// Watching already shows.
    pub(crate) fn items(&self, shelf: HomeShelf) -> Vec<&MediaItem> {
        let ShelfState::Ready(items) = self.shelf(shelf) else {
            return Vec::new();
        };
        let resuming: Vec<&ItemId> = match (shelf, self.shelf(HomeShelf::ContinueWatching)) {
            (HomeShelf::NextUp, ShelfState::Ready(resume)) => {
                resume.iter().map(MediaItem::id).collect()
            }
            _ => Vec::new(),
        };
        items
            .iter()
            .filter(|item| !resuming.contains(&item.id()))
            .collect()
    }

    pub(crate) fn page(&self) -> PageState {
        let mut failure = None;
        let mut empty = true;
        for slot in &self.slots {
            match &slot.state {
                ShelfState::Loading => return PageState::Content,
                ShelfState::Ready(items) => empty &= items.is_empty(),
                ShelfState::Failed(reason) => {
                    failure.get_or_insert(*reason);
                }
            }
        }
        let all_failed = self
            .slots
            .iter()
            .all(|slot| matches!(slot.state, ShelfState::Failed(_)));
        match (all_failed, failure) {
            (true, Some(reason)) => PageState::Failed(reason),
            _ if empty && failure.is_none() => PageState::Empty,
            _ => PageState::Content,
        }
    }

    pub(crate) fn hero(&self) -> HeroState {
        if let Some((shelf, id)) = &self.hero
            && let Some(item) = self.find(*shelf, id)
        {
            return HeroState::Ready(Box::new(HeroPick {
                item: item.clone(),
                shelf: *shelf,
            }));
        }
        if HERO_ORDER
            .iter()
            .any(|shelf| matches!(self.shelf(*shelf), ShelfState::Loading))
        {
            HeroState::Loading
        } else {
            HeroState::None
        }
    }

    /// Remember the card the person opened so focus can return to it.
    pub(crate) fn note_opened(&mut self, shelf: HomeShelf, item: ItemId) {
        self.opened = Some((shelf, item));
    }

    /// The hero was used instead of a card: focus returns to the hero.
    pub(crate) fn note_hero_used(&mut self) {
        self.opened = None;
    }

    /// Where focus goes when Home is shown again: the opened card while it
    /// is still on Home, otherwise the hero.
    pub(crate) fn return_focus(&self) -> FocusTarget {
        match &self.opened {
            Some((shelf, id)) if self.items(*shelf).iter().any(|item| item.id() == id) => {
                FocusTarget::Card(*shelf, id.clone())
            }
            _ => FocusTarget::Hero,
        }
    }

    /// Keep the pinned hero while it is still on Home. Otherwise pick the
    /// first item with backdrop art from the highest-priority shelf, but
    /// only once every shelf above it has answered, so the hero never
    /// switches as later shelves arrive. Without any backdrop, the first
    /// item of any of those shelves stands in.
    fn settle_hero(&mut self) {
        if let Some((shelf, id)) = &self.hero
            && self.find(*shelf, id).is_some()
        {
            return;
        }
        self.hero = None;
        let mut fallback = None;
        for shelf in HERO_ORDER {
            match self.shelf(shelf) {
                ShelfState::Loading => return,
                ShelfState::Failed(_) => continue,
                ShelfState::Ready(_) => {}
            }
            let items = self.items(shelf);
            if let Some(item) = items.iter().find(|item| has_backdrop(item)) {
                self.hero = Some((shelf, item.id().clone()));
                return;
            }
            if fallback.is_none() {
                fallback = items.first().map(|item| (shelf, item.id().clone()));
            }
        }
        self.hero = fallback;
    }

    fn find(&self, shelf: HomeShelf, id: &ItemId) -> Option<&MediaItem> {
        self.items(shelf).into_iter().find(|item| item.id() == id)
    }

    fn request(&mut self, shelf: HomeShelf) -> Request {
        self.issued += 1;
        let ticket = Ticket(self.issued);
        self.slot_mut(shelf).ticket = Some(ticket);
        Request { shelf, ticket }
    }

    fn slot(&self, shelf: HomeShelf) -> &Slot {
        self.slots
            .iter()
            .find(|slot| slot.shelf == shelf)
            .expect("every shelf has a slot")
    }

    fn slot_mut(&mut self, shelf: HomeShelf) -> &mut Slot {
        self.slots
            .iter_mut()
            .find(|slot| slot.shelf == shelf)
            .expect("every shelf has a slot")
    }
}

/// An item's own backdrop, or for an episode its series' backdrop.
pub(crate) fn has_backdrop(item: &MediaItem) -> bool {
    item.artwork.has_backdrop()
        || (item.kind == ItemKind::Episode && item.hierarchy.series_id.is_some())
}

/// The main line on a card: the series for an episode, otherwise the title.
pub(crate) fn card_title(item: &MediaItem) -> &str {
    if item.kind == ItemKind::Episode
        && let Some(series) = item
            .hierarchy
            .series_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
    {
        return series;
    }
    item.name()
}

/// The quiet line under a card's title. `S2 E5 · Low Tide` for an episode,
/// the year otherwise.
pub(crate) fn card_detail(item: &MediaItem) -> Option<String> {
    if item.kind == ItemKind::Episode {
        return Some(match item.hierarchy.episode_label() {
            Some(label) => format!("{label} · {}", item.name()),
            None => item.name().to_string(),
        });
    }
    item.metadata.year.map(|year| year.to_string())
}

/// `38m left`, from the saved position and the runtime.
pub(crate) fn remaining_label(item: &MediaItem) -> Option<String> {
    let position = item.user.resume_position()?;
    let runtime = item.metadata.runtime?;
    let left = runtime.checked_sub(position)?;
    if left < Duration::from_secs(60) {
        return None;
    }
    runtime_label(left).map(|label| format!("{label} left"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use matinee_core::{
        ImageTag, ItemHierarchy, ItemIdentity, ItemMetadata, TechnicalMedia, UserItemState,
    };

    pub(crate) fn id(value: &str) -> ItemId {
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
                runtime: Some(Duration::from_secs(100 * 60)),
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

    pub(crate) fn with_backdrop(mut item: MediaItem) -> MediaItem {
        item.artwork.backdrops = ImageTag::parse("back").into_iter().collect();
        item.artwork.primary = ImageTag::parse("poster");
        item
    }

    pub(crate) fn resumed(mut item: MediaItem, minutes: u64) -> MediaItem {
        item.user = UserItemState::from_parts(
            Some(Duration::from_secs(minutes * 60)),
            None,
            false,
            false,
            0,
        );
        item
    }

    fn request(requests: &[Request], shelf: HomeShelf) -> Request {
        *requests
            .iter()
            .find(|request| request.shelf == shelf)
            .unwrap()
    }

    /// Answer every request with these items.
    pub(crate) fn answer_all(
        model: &mut HomeModel,
        requests: &[Request],
        answer: impl Fn(HomeShelf) -> Response,
    ) {
        for request in requests {
            model.apply(*request, answer(request.shelf));
        }
    }

    #[test]
    fn home_opens_with_one_request_per_shelf_and_loads() {
        let (mut model, requests) = HomeModel::open();
        assert_eq!(requests.len(), HomeShelf::ALL.len());
        for shelf in HomeShelf::ALL {
            assert_eq!(model.shelf(shelf), &ShelfState::Loading);
        }
        assert_eq!(model.hero(), HeroState::Loading);
        assert_eq!(model.page(), PageState::Content);
        assert!(model.is_loading());
        answer_all(&mut model, &requests, |shelf| {
            Ok(vec![with_backdrop(item(
                &format!("{shelf:?}").to_lowercase(),
                ItemKind::Movie,
            ))])
        });
        assert!(!model.is_loading());
        for shelf in HomeShelf::ALL {
            assert_eq!(model.items(shelf).len(), 1, "{shelf:?}");
        }
        assert!(
            matches!(model.hero(), HeroState::Ready(pick) if pick.shelf == HomeShelf::ContinueWatching)
        );
    }

    #[test]
    fn one_failed_shelf_leaves_a_usable_home() {
        let (mut model, requests) = HomeModel::open();
        answer_all(&mut model, &requests, |shelf| match shelf {
            HomeShelf::NextUp => Err(HomeFailure::Unreachable),
            _ => Ok(vec![item("m", ItemKind::Movie)]),
        });
        assert_eq!(
            model.shelf(HomeShelf::NextUp),
            &ShelfState::Failed(HomeFailure::Unreachable)
        );
        assert_eq!(model.items(HomeShelf::ContinueWatching).len(), 1);
        assert_eq!(model.items(HomeShelf::RecentMovies).len(), 1);
        assert_eq!(model.page(), PageState::Content);
        assert!(matches!(model.hero(), HeroState::Ready(_)));
        // Only the failed shelf is asked again.
        let retry = model.retry_shelf(HomeShelf::NextUp);
        assert_eq!(retry.len(), 1);
        assert_eq!(model.shelf(HomeShelf::NextUp), &ShelfState::Loading);
        assert!(model.retry_shelf(HomeShelf::RecentMovies).is_empty());
    }

    #[test]
    fn an_empty_shelf_is_ready_not_loading_or_failed() {
        let (mut model, requests) = HomeModel::open();
        model.apply(
            request(&requests, HomeShelf::ContinueWatching),
            Ok(Vec::new()),
        );
        assert_eq!(
            model.shelf(HomeShelf::ContinueWatching),
            &ShelfState::Ready(Vec::new())
        );
        assert_eq!(model.page(), PageState::Content, "others still loading");
    }

    #[test]
    fn an_empty_library_is_its_own_state_and_total_failure_offers_retry() {
        let (mut model, requests) = HomeModel::open();
        answer_all(&mut model, &requests, |_| Ok(Vec::new()));
        assert_eq!(model.page(), PageState::Empty);
        assert_eq!(model.hero(), HeroState::None);

        let (mut model, requests) = HomeModel::open();
        answer_all(&mut model, &requests, |_| Err(HomeFailure::Unreachable));
        assert_eq!(model.page(), PageState::Failed(HomeFailure::Unreachable));
        assert!(!HomeFailure::Unreachable.message().contains("http"));
        let retry = model.retry();
        assert_eq!(retry.len(), HomeShelf::ALL.len());
        assert_eq!(model.page(), PageState::Content);
        assert_eq!(model.hero(), HeroState::Loading);

        // Some empty, one failed: still Home, not the empty-library page.
        let (mut model, requests) = HomeModel::open();
        answer_all(&mut model, &requests, |shelf| match shelf {
            HomeShelf::Favorites => Err(HomeFailure::Unreadable),
            _ => Ok(Vec::new()),
        });
        assert_eq!(model.page(), PageState::Content);
    }

    #[test]
    fn a_late_answer_from_an_earlier_refresh_is_ignored() {
        let (mut model, first) = HomeModel::open();
        let second = model.refresh();
        let shelf = HomeShelf::ContinueWatching;
        let newer = vec![resumed(item("movie-b", ItemKind::Movie), 40)];
        assert_eq!(
            model.apply(request(&second, shelf), Ok(newer.clone())),
            Applied::Updated
        );
        assert_eq!(
            model.apply(
                request(&first, shelf),
                Ok(vec![item("movie-a", ItemKind::Movie)])
            ),
            Applied::Ignored
        );
        assert_eq!(model.shelf(shelf), &ShelfState::Ready(newer));
        // A late failure is ignored as well.
        assert_eq!(
            model.apply(request(&first, shelf), Err(HomeFailure::SignedOut)),
            Applied::Ignored
        );
    }

    #[test]
    fn a_failed_refresh_keeps_what_is_shown() {
        let (mut model, requests) = HomeModel::open();
        answer_all(&mut model, &requests, |_| {
            Ok(vec![item("m", ItemKind::Movie)])
        });
        let refresh = model.refresh();
        assert!(model.is_loading());
        assert_eq!(
            model.items(HomeShelf::RecentMovies).len(),
            1,
            "kept while refreshing"
        );
        answer_all(&mut model, &refresh, |_| Err(HomeFailure::Unreachable));
        assert_eq!(model.items(HomeShelf::RecentMovies).len(), 1);
        assert_eq!(model.page(), PageState::Content);
    }

    #[test]
    fn an_unauthorized_shelf_reports_the_session_expired() {
        let (mut model, requests) = HomeModel::open();
        let outcome = model.apply(
            request(&requests, HomeShelf::ContinueWatching),
            Err(HomeFailure::from_error(&JellyfinError::Unauthorized)),
        );
        assert_eq!(outcome, Applied::SessionExpired);
    }

    #[test]
    fn the_hero_waits_for_higher_shelves_then_stays_pinned() {
        let (mut model, requests) = HomeModel::open();
        let movie = with_backdrop(item("movie-1", ItemKind::Movie));
        // Recent movies answer first: the hero still waits for Continue
        // Watching and Next Up, so it does not switch when they arrive.
        model.apply(
            request(&requests, HomeShelf::RecentMovies),
            Ok(vec![movie.clone()]),
        );
        assert_eq!(model.hero(), HeroState::Loading);
        let resume = with_backdrop(resumed(item("resume-1", ItemKind::Movie), 30));
        model.apply(
            request(&requests, HomeShelf::ContinueWatching),
            Ok(vec![resume.clone()]),
        );
        let HeroState::Ready(pick) = model.hero() else {
            panic!("Continue Watching decides the hero");
        };
        assert_eq!(pick.item.id().as_str(), "resume-1");
        assert_eq!(pick.eyebrow(), "Continue watching");
        assert_eq!(pick.play().unwrap().label(), "Resume · 30:00");

        // A refresh that still contains it keeps it, even if order changes.
        let refresh = model.refresh();
        let other = with_backdrop(resumed(item("resume-0", ItemKind::Movie), 5));
        model.apply(
            request(&refresh, HomeShelf::ContinueWatching),
            Ok(vec![other.clone(), resumed(resume.clone(), 50)]),
        );
        let HeroState::Ready(pick) = model.hero() else {
            panic!();
        };
        assert_eq!(pick.item.id().as_str(), "resume-1", "pinned");
        assert_eq!(pick.play().unwrap().label(), "Resume · 50:00", "fresh data");

        // Once it leaves Home, the next choice follows the same order.
        let refresh = model.refresh();
        model.apply(
            request(&refresh, HomeShelf::ContinueWatching),
            Ok(vec![other]),
        );
        let HeroState::Ready(pick) = model.hero() else {
            panic!();
        };
        assert_eq!(pick.item.id().as_str(), "resume-0");
    }

    #[test]
    fn the_hero_prefers_backdrops_and_falls_back_to_a_fresh_title() {
        let (mut model, requests) = HomeModel::open();
        answer_all(&mut model, &requests, |shelf| match shelf {
            // No backdrop on the only resumable title.
            HomeShelf::ContinueWatching => Ok(vec![resumed(item("plain", ItemKind::Movie), 3)]),
            HomeShelf::NextUp => Err(HomeFailure::Unreachable),
            HomeShelf::RecentMovies => Ok(vec![with_backdrop(item("fresh", ItemKind::Movie))]),
            _ => Ok(Vec::new()),
        });
        let HeroState::Ready(pick) = model.hero() else {
            panic!();
        };
        assert_eq!(pick.item.id().as_str(), "fresh");
        assert_eq!(pick.eyebrow(), "Tonight's feature");
        assert_eq!(pick.play().unwrap().label(), "Play");

        let (mut model, requests) = HomeModel::open();
        answer_all(&mut model, &requests, |shelf| match shelf {
            HomeShelf::RecentSeries => Ok(vec![item("series", ItemKind::Series)]),
            _ => Ok(Vec::new()),
        });
        let HeroState::Ready(pick) = model.hero() else {
            panic!("no backdrop anywhere still features something");
        };
        assert_eq!(pick.item.id().as_str(), "series");
        assert!(pick.play().is_none(), "a series opens Details instead");
    }

    #[test]
    fn next_up_leaves_out_what_continue_watching_shows() {
        let (mut model, requests) = HomeModel::open();
        let mut episode = item("ep-5", ItemKind::Episode);
        episode.hierarchy = ItemHierarchy {
            series_id: Some(id("harbor")),
            series_name: Some("Harbor Lights".into()),
            index: Some(5),
            parent_index: Some(2),
            ..ItemHierarchy::default()
        };
        answer_all(&mut model, &requests, |shelf| match shelf {
            HomeShelf::ContinueWatching => Ok(vec![episode.clone()]),
            HomeShelf::NextUp => Ok(vec![episode.clone(), item("ep-9", ItemKind::Episode)]),
            _ => Ok(Vec::new()),
        });
        let next: Vec<&str> = model
            .items(HomeShelf::NextUp)
            .iter()
            .map(|item| item.id().as_str())
            .collect();
        assert_eq!(next, vec!["ep-9"]);
        // Episodes have a series backdrop, so one can be the hero.
        let HeroState::Ready(pick) = model.hero() else {
            panic!();
        };
        assert_eq!(pick.title(), "Harbor Lights");
        assert_eq!(card_detail(&pick.item).as_deref(), Some("S2 E5 · ep-5"));
    }

    #[test]
    fn focus_returns_to_the_opened_card_while_it_is_still_shown() {
        let (mut model, requests) = HomeModel::open();
        answer_all(&mut model, &requests, |_| {
            Ok(vec![item("m", ItemKind::Movie)])
        });
        assert_eq!(model.return_focus(), FocusTarget::Hero);
        model.note_opened(HomeShelf::RecentMovies, id("m"));
        assert_eq!(
            model.return_focus(),
            FocusTarget::Card(HomeShelf::RecentMovies, id("m"))
        );
        model.note_hero_used();
        assert_eq!(model.return_focus(), FocusTarget::Hero);
        model.note_opened(HomeShelf::RecentMovies, id("m"));
        let refresh = model.refresh();
        answer_all(&mut model, &refresh, |_| Ok(Vec::new()));
        assert_eq!(model.return_focus(), FocusTarget::Hero);
    }

    #[test]
    fn card_lines_are_quiet() {
        let mut movie = resumed(item("m", ItemKind::Movie), 62);
        movie.metadata.year = Some(2019);
        assert_eq!(card_title(&movie), "m");
        assert_eq!(card_detail(&movie).as_deref(), Some("2019"));
        assert_eq!(remaining_label(&movie).as_deref(), Some("38m left"));
        assert_eq!(remaining_label(&item("fresh", ItemKind::Movie)), None);
    }

    #[test]
    fn shelves_are_capped() {
        let (mut model, requests) = HomeModel::open();
        answer_all(&mut model, &requests, |_| {
            Ok((0..40)
                .map(|index| item(&format!("m-{index}"), ItemKind::Movie))
                .collect())
        });
        assert_eq!(model.items(HomeShelf::RecentMovies).len(), SHELF_LIMIT);
    }
}
