//! Native playback feasibility spike for Matinee Next. Not production code:
//! see `docs/architecture/playback.md` for findings and the recommendation.

pub mod frame;
#[cfg(feature = "mpv")]
pub mod mpv;
