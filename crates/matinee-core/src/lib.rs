//! Matinee's domain model.
//!
//! Items, people, artwork identity, progress, libraries, playback plans, and
//! native playback capabilities live here. Server payloads, HTTP, the
//! presentation framework, and the playback engine do not. A client converts
//! responses into these types at the boundary. Time is [`std::time::Duration`].
//! Tick counts never leave that boundary.

mod capability;
mod date;
mod home;
mod id;
mod images;
mod item;
mod library;
mod media;
mod playback;
mod progress;
mod user;

pub use capability::{
    NATIVE_PLAYBACK_NAME, PlaybackCapabilities, SubtitleCapabilities, TranscodeTarget,
    native_playback,
};
pub use date::CalendarDate;
pub use home::{HomeFeed, HomeShelf};
pub use id::{IdError, ItemId, LibraryId, MediaSourceId, PlaySessionId, UserId};
pub use images::{ImageRole, ImageTag, ItemArtwork};
pub use item::{
    Chapter, Credit, ItemHierarchy, ItemIdentity, ItemKind, ItemMetadata, MediaItem, Person,
};
pub use library::{CollectionContext, LibraryKind, LibrarySort};
pub use media::{
    AudioStream, Delivery, DynamicRange, MediaSource, MediaStream, SubtitleStream, TechnicalMedia,
    TechnicalSummary, VideoStream,
};
pub use playback::{
    PlaybackMethod, PlaybackOptions, PlaybackPlan, PlaybackReport, ReportKind, StreamAuthorization,
};
pub use progress::{UserItemState, ViewingProgress};
pub use user::User;
