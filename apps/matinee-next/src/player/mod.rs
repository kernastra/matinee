//! Native Player.
//!
//! [`model::PlayerModel`] is the playback model. The GPUI screen reads it and
//! forwards input. Jellyfin HTTP runs on the application [`crate::runtime::ServiceRuntime`].
//! Frames come from [`matinee_player::Player`] and are painted by Atelier's
//! `ExternalFrameSurface`. This module does not open a second engine or a
//! second Tokio runtime.
//!
//! # Shortcuts
//!
//! These apply while the video surface is focused and no track menu is open:
//!
//! | Key | Action |
//! |---|---|
//! | Space | Play or pause |
//! | Left | Seek backward 10 seconds |
//! | Right | Seek forward 10 seconds |
//! | Up | Volume up |
//! | Down | Volume down |
//! | M | Mute or unmute |
//! | F | Fullscreen |
//! | Escape | Leave fullscreen, or return to the shell |
//!
//! A focused button, slider, or menu keeps its own keys. Escape still leaves
//! fullscreen, and closes an open track menu before it leaves the player.
//!
//! # Progress reports
//!
//! Start is sent once the file has loaded. Progress is sent immediately on
//! pause, resume, and seek, and every 10 seconds of wall time while playback
//! is playing. Stop is sent on natural completion and when the player closes.
//! A failed report is a notice. It does not stop playback.

mod frame;
mod model;
mod prepare;
mod screen;

pub(crate) use model::{PlayerPreview, format_clock};
pub(crate) use prepare::resume_start;
pub(crate) use screen::{KeyOutcome, LeavePlayer, PlayerScreen};

use std::time::Duration;

/// How long the controls stay up during playback with no new input.
pub(crate) const CONTROLS_IDLE: Duration = Duration::from_secs(5);

/// Wall-clock gap between periodic Jellyfin progress reports.
pub(crate) const PROGRESS_INTERVAL: Duration = Duration::from_secs(10);

/// Minimum gap between seek commands while a scrubber drag is moving.
pub(crate) const SEEK_THROTTLE: Duration = Duration::from_millis(200);

/// Shipping treats a resume inside this tail as finished and starts over.
pub(crate) const COMPLETED_TAIL: Duration = Duration::from_secs(30);

pub(crate) const SEEK_STEP_MS: i64 = 10_000;

pub(crate) const VOLUME_STEP: f32 = 0.05;
