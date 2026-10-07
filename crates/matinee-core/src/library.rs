//! Library queries and collection membership.
//!
//! Sorts, filters, and pages are typed. Callers do not pass server query
//! strings; the server client maps these onto its own parameters.

use crate::id::{ItemId, LibraryId};
use crate::item::MediaItem;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LibraryKind {
    Movies,
    Series,
}

/// Sorts the shipping library uses.
///
/// Name is ascending. The other three are descending.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LibrarySort {
    #[default]
    Name,
    DateCreated,
    ProductionYear,
    CommunityRating,
}

impl LibrarySort {
    pub const ALL: [LibrarySort; 4] = [
        LibrarySort::Name,
        LibrarySort::DateCreated,
        LibrarySort::ProductionYear,
        LibrarySort::CommunityRating,
    ];

    pub fn descending(self) -> bool {
        !matches!(self, Self::Name)
    }

    /// The shipping menu's label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Title",
            Self::DateCreated => "Recently added",
            Self::ProductionYear => "Release year",
            Self::CommunityRating => "Rating",
        }
    }
}

impl LibraryKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Movies => "Movies",
            Self::Series => "Series",
        }
    }
}

/// Which titles to show by the person's own state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WatchFilter {
    #[default]
    All,
    Unwatched,
    Watched,
    Favorites,
}

impl WatchFilter {
    pub const ALL: [WatchFilter; 4] = [
        WatchFilter::All,
        WatchFilter::Unwatched,
        WatchFilter::Watched,
        WatchFilter::Favorites,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All titles",
            Self::Unwatched => "Unwatched",
            Self::Watched => "Watched",
            Self::Favorites => "Favorites",
        }
    }
}

/// Narrowing that the server applies. The default matches everything.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LibraryFilter {
    pub watch: WatchFilter,
    /// One genre, by its server id.
    pub genre: Option<ItemId>,
}

impl LibraryFilter {
    /// Whether anything is narrowed.
    pub fn is_active(&self) -> bool {
        *self != Self::default()
    }
}

/// One server-side query over a library: what, where, in what order, and
/// narrowed how. Paging is separate ([`LibraryPageRequest`]), so the same
/// query identifies every page of one result.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LibraryQuery {
    pub kind: LibraryKind,
    /// One library (a server view). `None` is every library, as shipping.
    pub view: Option<LibraryId>,
    pub sort: LibrarySort,
    pub filter: LibraryFilter,
}

impl LibraryQuery {
    pub fn new(kind: LibraryKind) -> Self {
        Self {
            kind,
            view: None,
            sort: LibrarySort::default(),
            filter: LibraryFilter::default(),
        }
    }
}

/// Which slice of a query's result to fetch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryPageRequest {
    pub start: usize,
    pub limit: usize,
}

/// One slice of a query's result.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryPage {
    pub items: Vec<MediaItem>,
    /// Index of the first item in the whole result.
    pub start: usize,
    /// How many items the whole result has, when the server says.
    pub total: Option<usize>,
}

impl LibraryPage {
    /// Whether items remain after this page. Without a total, a short page
    /// is the end.
    pub fn has_more(&self, requested: usize) -> bool {
        match self.total {
            Some(total) => self.start + self.items.len() < total && !self.items.is_empty(),
            None => self.items.len() >= requested && requested > 0,
        }
    }
}

/// What a library (a server view) holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LibraryContent {
    Movies,
    Series,
    /// A mixed library: movies and series can both be inside.
    Mixed,
}

impl LibraryContent {
    /// Whether this library can hold titles of `kind`.
    pub fn holds(self, kind: LibraryKind) -> bool {
        matches!(
            (self, kind),
            (Self::Mixed, _)
                | (Self::Movies, LibraryKind::Movies)
                | (Self::Series, LibraryKind::Series)
        )
    }
}

/// One of the person's libraries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryView {
    pub id: LibraryId,
    pub name: String,
    pub content: LibraryContent,
}

/// A genre the server can filter by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryGenre {
    pub id: ItemId,
    pub name: String,
}

/// A box set and the titles that belong to it.
#[derive(Clone, Debug, PartialEq)]
pub struct CollectionContext {
    pub collection: MediaItem,
    pub items: Vec<MediaItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_sort_is_ascending_and_the_others_are_not() {
        assert!(!LibrarySort::Name.descending());
        assert!(LibrarySort::DateCreated.descending());
        assert!(LibrarySort::ProductionYear.descending());
        assert!(LibrarySort::CommunityRating.descending());
    }

    #[test]
    fn a_page_knows_whether_more_remain() {
        let page = |start, count, total| {
            LibraryPage {
                items: Vec::new(),
                start,
                total,
            }
            .with_len(count)
        };
        assert!(page(0, 100, Some(250)).has_more(100));
        assert!(page(100, 100, Some(250)).has_more(100));
        assert!(!page(200, 50, Some(250)).has_more(100));
        assert!(!page(0, 0, Some(0)).has_more(100));
        // A total that says more but an empty page: the end, not a loop.
        assert!(!page(300, 0, Some(400)).has_more(100));
        // No total: a full page may have more, a short one does not.
        assert!(page(0, 100, None).has_more(100));
        assert!(!page(0, 40, None).has_more(100));
    }

    #[test]
    fn mixed_libraries_hold_both_kinds() {
        assert!(LibraryContent::Mixed.holds(LibraryKind::Movies));
        assert!(LibraryContent::Mixed.holds(LibraryKind::Series));
        assert!(LibraryContent::Movies.holds(LibraryKind::Movies));
        assert!(!LibraryContent::Movies.holds(LibraryKind::Series));
        assert!(!LibraryContent::Series.holds(LibraryKind::Movies));
    }

    #[test]
    fn the_default_filter_is_inactive() {
        assert!(!LibraryFilter::default().is_active());
        let watched = LibraryFilter {
            watch: WatchFilter::Watched,
            genre: None,
        };
        assert!(watched.is_active());
    }

    impl LibraryPage {
        fn with_len(mut self, count: usize) -> Self {
            self.items = (0..count)
                .map(|index| MediaItem {
                    identity: crate::item::ItemIdentity {
                        id: ItemId::parse(format!("item-{index}")).unwrap(),
                        name: format!("Item {index}"),
                    },
                    kind: crate::item::ItemKind::Movie,
                    metadata: Default::default(),
                    artwork: Default::default(),
                    user: Default::default(),
                    hierarchy: Default::default(),
                    media: Default::default(),
                    people: Vec::new(),
                    chapters: Vec::new(),
                })
                .collect();
            self
        }
    }
}
