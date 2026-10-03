//! Home shelves. Each shelf is its own list so one failure can stay local
//! to the client that fills it.

use crate::item::MediaItem;

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
