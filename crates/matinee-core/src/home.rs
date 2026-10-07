//! Home shelves. Each shelf is its own list so one failure can stay local
//! to the client that fills it.

use crate::item::MediaItem;

/// One independently loaded Home shelf in the native app.
///
/// Each shelf is its own request, so one failing leaves the others usable.
/// Order here is the order Home shows them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HomeShelf {
    /// Titles with a saved position, most recently watched first.
    ContinueWatching,
    /// The next unwatched episode of each series in progress.
    NextUp,
    /// Movies, newest additions first.
    RecentMovies,
    /// Series, newest additions first.
    RecentSeries,
    /// Titles the person marked as favorites.
    Favorites,
}

impl HomeShelf {
    pub const ALL: [HomeShelf; 5] = [
        HomeShelf::ContinueWatching,
        HomeShelf::NextUp,
        HomeShelf::RecentMovies,
        HomeShelf::RecentSeries,
        HomeShelf::Favorites,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::ContinueWatching => "Continue Watching",
            Self::NextUp => "Next Up",
            Self::RecentMovies => "Recently Added Movies",
            Self::RecentSeries => "Recently Added Series",
            Self::Favorites => "Favorites",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct HomeFeed {
    pub resume: Vec<MediaItem>,
    pub latest: Vec<MediaItem>,
    pub movies: Vec<MediaItem>,
    pub series: Vec<MediaItem>,
    pub top_rated: Vec<MediaItem>,
    pub favorites: Vec<MediaItem>,
}

impl HomeFeed {
    pub fn is_empty(&self) -> bool {
        self.resume.is_empty()
            && self.latest.is_empty()
            && self.movies.is_empty()
            && self.series.is_empty()
            && self.top_rated.is_empty()
            && self.favorites.is_empty()
    }
}
