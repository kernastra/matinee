//! Matinee's domain model.
//!
//! Items, people, artwork identity, progress, libraries, and playback plans
//! live here. Jellyfin JSON, HTTP, GPUI, and the playback engine do not.
//! A client converts server responses into these types at the boundary.
//! Time is [`std::time::Duration`]. Tick counts never leave that boundary.

mod home;
mod id;
mod images;
mod item;
mod library;
mod media;
mod playback;
mod progress;
mod user;

pub use home::HomeFeed;
pub use id::{IdError, ItemId, LibraryId, MediaSourceId, PlaySessionId, UserId};
pub use images::{ImageRole, ImageTag, ItemArtwork};
pub use item::{
    Chapter, Credit, ItemHierarchy, ItemIdentity, ItemKind, ItemMetadata, MediaItem, Person,
};
pub use library::{CollectionContext, LibraryKind, LibrarySort};
pub use media::{
    AudioStream, Delivery, DynamicRange, MediaSource, MediaStream, SubtitleStream, TechnicalMedia,
    VideoStream,
};
pub use playback::{PlaybackMethod, PlaybackOptions, PlaybackPlan, PlaybackReport, ReportKind};
pub use progress::{UserItemState, ViewingProgress};
pub use user::User;
