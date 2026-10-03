//! A title in the library: identity, kind, metadata, people, and chapters.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::date::CalendarDate;
use crate::id::ItemId;
use crate::images::{ImageTag, ItemArtwork};
use crate::media::TechnicalMedia;
use crate::progress::UserItemState;

/// One library title.
///
/// The parts stay separate so a missing overview is not the same kind of
/// fact as a missing id.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaItem {
    pub identity: ItemIdentity,
    pub kind: ItemKind,
    pub metadata: ItemMetadata,
    pub artwork: ItemArtwork,
    pub user: UserItemState,
    pub hierarchy: ItemHierarchy,
    pub media: TechnicalMedia,
    pub people: Vec<Person>,
    pub chapters: Vec<Chapter>,
}

impl MediaItem {
    pub fn id(&self) -> &ItemId {
        &self.identity.id
    }

    pub fn name(&self) -> &str {
        &self.identity.name
    }

    pub fn is_resumable(&self) -> bool {
        self.user.is_resumable()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemIdentity {
    pub id: ItemId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Movie,
    Series,
    Season,
    Episode,
    /// A box set or other collection of titles.
    Collection,
    /// A server type this model does not name. The label is the server's word.
    Other(String),
}

impl ItemKind {
    pub fn label(&self) -> &str {
        match self {
            Self::Movie => "Movie",
            Self::Series => "Series",
            Self::Season => "Season",
            Self::Episode => "Episode",
            Self::Collection => "Collection",
            Self::Other(label) => label,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ItemMetadata {
    pub overview: Option<String>,
    pub year: Option<i32>,
    pub runtime: Option<Duration>,
    pub community_rating: Option<f64>,
    pub critic_rating: Option<f64>,
    pub official_rating: Option<String>,
    pub genres: Vec<String>,
    pub taglines: Vec<String>,
    pub studios: Vec<String>,
    pub production_locations: Vec<String>,
    pub provider_ids: BTreeMap<String, String>,
    pub premiere: Option<CalendarDate>,
    pub date_created: Option<CalendarDate>,
    pub end_date: Option<CalendarDate>,
}

/// Where an item sits in a series or a parent folder.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ItemHierarchy {
    pub parent_id: Option<ItemId>,
    pub series_id: Option<ItemId>,
    pub series_name: Option<String>,
    pub season_id: Option<ItemId>,
    pub season_name: Option<String>,
    /// Episode number inside the season.
    pub index: Option<u32>,
    /// Season number.
    pub parent_index: Option<u32>,
}

impl ItemHierarchy {
    /// `S1 E2` when both numbers exist.
    pub fn episode_label(&self) -> Option<String> {
        match (self.parent_index, self.index) {
            (Some(season), Some(episode)) => Some(format!("S{season} E{episode}")),
            (None, Some(episode)) => Some(format!("E{episode}")),
            _ => None,
        }
    }
}

/// A cast or crew credit. Enough for the existing details views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub id: Option<ItemId>,
    pub name: String,
    pub role: Option<String>,
    pub credit: Credit,
    pub image: Option<ImageTag>,
}

impl Person {
    pub fn has_image(&self) -> bool {
        self.image.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credit {
    Actor,
    Director,
    Writer,
    Producer,
    Other(String),
}

impl Credit {
    pub fn label(&self) -> &str {
        match self {
            Self::Actor => "Actor",
            Self::Director => "Director",
            Self::Writer => "Writer",
            Self::Producer => "Producer",
            Self::Other(label) => label,
        }
    }
}

/// A chapter marker. `index` is the position used to request its image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chapter {
    pub index: u32,
    pub name: Option<String>,
    pub start: Duration,
    pub image: Option<ImageTag>,
}

impl Chapter {
    pub fn has_image(&self) -> bool {
        self.image.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_label_uses_season_and_episode_numbers() {
        let hierarchy = ItemHierarchy {
            parent_index: Some(1),
            index: Some(2),
            ..ItemHierarchy::default()
        };
        assert_eq!(hierarchy.episode_label().as_deref(), Some("S1 E2"));
    }

    #[test]
    fn unknown_kind_keeps_its_label() {
        assert_eq!(ItemKind::Other("Trailer".into()).label(), "Trailer");
        assert_eq!(ItemKind::Collection.label(), "Collection");
    }
}
