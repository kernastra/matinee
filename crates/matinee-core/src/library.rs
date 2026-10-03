//! Library queries and collection membership.
//!
//! Sorts are typed. Callers do not pass server query strings.

use crate::item::MediaItem;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryKind {
    Movies,
    Series,
}

/// Sorts the shipping library uses.
///
/// Name is ascending. The other three are descending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibrarySort {
    Name,
    DateCreated,
    ProductionYear,
    CommunityRating,
}

impl LibrarySort {
    pub fn descending(self) -> bool {
        !matches!(self, Self::Name)
    }
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
}
