//! Matinee's playback engine.
//!
//! Applications load a URL, read a [`Snapshot`], and take [`CpuFrame`]s.
//! They never see a libmpv handle or a GPUI type. Presentation is an
//! external frame surface in the UI framework. Jellyfin chooses a direct or
//! transcoded URL before [`Player::load`]; this crate plays that URL either way.
//!
//! The shared library is loaded at runtime. A missing or incompatible build
//! is a [`PlayerError`]. The process keeps running.

mod engine;
mod error;
mod frame;
mod player;
mod profile;
mod state;
mod tracks;

pub use engine::{EngineInfo, LoadRequest, measure_software_render, probe_engine};
pub use error::PlayerError;
pub use frame::{CpuFrame, Diagnostics};
pub use player::Player;
pub use profile::{PROFILE_NAME, native_device_profile};
pub use state::{PlaybackState, PlayerEvent, Snapshot};
pub use tracks::{SubtitleForm, Track, TrackId, TrackKind};
