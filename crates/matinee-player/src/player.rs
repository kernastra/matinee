//! Public playback commands. They are checked against the latest snapshot,
//! then handed to the owner thread. Nothing here calls into libmpv.

#![forbid(unsafe_code)]

use std::time::Duration;

use crate::engine::{Engine, LoadRequest, OpenOptions, Request};
use crate::error::PlayerError;
use crate::frame::{CpuFrame, Diagnostics};
use crate::state::{PlaybackState, PlayerEvent, Snapshot};
use crate::tracks::TrackId;

/// A playback session. Dropping it stops the owner and render threads and
/// destroys the engine before the process continues.
pub struct Player {
    engine: Engine,
}

impl Player {
    /// Open the engine with sound output. A missing library is [`PlayerError::LibraryMissing`].
    pub fn open() -> Result<Self, PlayerError> {
        Self::open_with(true)
    }

    /// `audio: false` selects a null output. Tests and headless runs use that.
    pub fn open_with(audio: bool) -> Result<Self, PlayerError> {
        Ok(Self {
            engine: Engine::open(OpenOptions { audio })?,
        })
    }

    pub fn load(&self, request: LoadRequest) -> Result<(), PlayerError> {
        self.dispatch(Request::Load(request))
    }

    pub fn play(&self) -> Result<(), PlayerError> {
        self.dispatch(Request::Play)
    }

    pub fn pause(&self) -> Result<(), PlayerError> {
        self.dispatch(Request::Pause)
    }

    pub fn toggle(&self) -> Result<(), PlayerError> {
        let request = match self.snapshot().state {
            PlaybackState::Playing | PlaybackState::Buffering => Request::Pause,
            _ => Request::Play,
        };
        self.dispatch(request)
    }

    pub fn stop(&self) -> Result<(), PlayerError> {
        self.dispatch(Request::Stop)
    }

    pub fn seek(&self, position: Duration) -> Result<(), PlayerError> {
        self.dispatch(Request::Seek(position))
    }

    /// Signed offset in milliseconds. Negative seeks backward.
    pub fn seek_by_ms(&self, delta_ms: i64) -> Result<(), PlayerError> {
        self.dispatch(Request::SeekByMs(delta_ms))
    }

    /// Linear gain from 0 to 1.
    pub fn set_volume(&self, volume: f32) -> Result<(), PlayerError> {
        self.dispatch(Request::Volume(volume))
    }

    pub fn set_muted(&self, muted: bool) -> Result<(), PlayerError> {
        self.dispatch(Request::Mute(muted))
    }

    pub fn select_audio(&self, id: TrackId) -> Result<(), PlayerError> {
        self.dispatch(Request::Audio(id))
    }

    pub fn select_subtitle(&self, id: TrackId) -> Result<(), PlayerError> {
        self.dispatch(Request::Subtitle(id))
    }

    pub fn disable_subtitles(&self) -> Result<(), PlayerError> {
        self.dispatch(Request::SubtitlesOff)
    }

    /// External subtitle file or URL. The engine renders it into the frame.
    pub fn add_subtitle(&self, source: impl Into<String>) -> Result<(), PlayerError> {
        self.dispatch(Request::AddSubtitle(source.into()))
    }

    pub fn snapshot(&self) -> Snapshot {
        self.engine.shared().snapshot()
    }

    pub fn poll_event(&self) -> Option<PlayerEvent> {
        self.engine.shared().poll_event()
    }

    pub fn take_frame(&self) -> Option<CpuFrame> {
        self.engine.frames().take()
    }

    /// Called on the render thread after a frame is stored. It must only signal
    /// another thread. It must not call back into the player.
    pub fn set_frame_listener(&self, listener: impl Fn() + Send + Sync + 'static) {
        self.engine.frames().set_listener(listener);
    }

    pub fn diagnostics(&self) -> Diagnostics {
        self.engine.diagnostics()
    }

    fn dispatch(&self, request: Request) -> Result<(), PlayerError> {
        validate(&self.snapshot(), &request)?;
        self.engine.send(request)
    }
}

pub(crate) fn validate(snapshot: &Snapshot, request: &Request) -> Result<(), PlayerError> {
    match request {
        Request::Load(load) => {
            if load.url.trim().is_empty() {
                return Err(PlayerError::InvalidCommand("url is empty".into()));
            }
            if load.url.contains('\0') {
                return Err(PlayerError::InvalidCommand("url contains NUL".into()));
            }
            for (name, value) in &load.headers {
                if name.is_empty()
                    || name.contains([':', '\r', '\n'])
                    || value.contains(['\r', '\n'])
                {
                    return Err(PlayerError::InvalidCommand("invalid header".into()));
                }
            }
            Ok(())
        }
        Request::Play => match snapshot.state {
            PlaybackState::Idle | PlaybackState::Error => {
                Err(PlayerError::InvalidCommand("nothing is loaded".into()))
            }
            _ => Ok(()),
        },
        Request::Pause | Request::Stop | Request::Seek(_) | Request::SeekByMs(_) => {
            match snapshot.state {
                PlaybackState::Idle | PlaybackState::Error => {
                    Err(PlayerError::InvalidCommand("nothing is loaded".into()))
                }
                _ => Ok(()),
            }
        }
        Request::Volume(volume) => {
            if volume.is_finite() && (0.0..=1.0).contains(volume) {
                Ok(())
            } else {
                Err(PlayerError::InvalidCommand(
                    "volume must be between 0 and 1".into(),
                ))
            }
        }
        Request::Mute(_) | Request::SubtitlesOff => Ok(()),
        Request::Audio(id) => {
            if snapshot.audio_tracks.iter().any(|track| track.id == *id) {
                Ok(())
            } else {
                Err(PlayerError::InvalidCommand(format!(
                    "unknown audio track {id}"
                )))
            }
        }
        Request::Subtitle(id) => {
            if snapshot.subtitle_tracks.iter().any(|track| track.id == *id) {
                Ok(())
            } else {
                Err(PlayerError::InvalidCommand(format!(
                    "unknown subtitle track {id}"
                )))
            }
        }
        Request::AddSubtitle(source) => {
            if source.trim().is_empty() {
                return Err(PlayerError::InvalidCommand(
                    "subtitle source is empty".into(),
                ));
            }
            match snapshot.state {
                PlaybackState::Idle | PlaybackState::Error => {
                    Err(PlayerError::InvalidCommand("nothing is loaded".into()))
                }
                _ => Ok(()),
            }
        }
        Request::Shutdown => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> Snapshot {
        Snapshot::default()
    }

    #[test]
    fn idle_rejects_transport_and_accepts_a_url() {
        let snapshot = idle();
        assert!(validate(&snapshot, &Request::Play).is_err());
        assert!(validate(&snapshot, &Request::Seek(Duration::from_secs(1))).is_err());
        assert!(validate(&snapshot, &Request::Volume(1.5)).is_err());
        assert!(validate(&snapshot, &Request::Volume(0.5)).is_ok());
        assert!(
            validate(
                &snapshot,
                &Request::Load(LoadRequest {
                    url: "file.mkv".into(),
                    start: Some(Duration::from_secs(3)),
                    headers: vec![("Authorization".into(), "MediaBrowser Token=x".into())],
                })
            )
            .is_ok()
        );
        assert!(
            validate(
                &snapshot,
                &Request::Load(LoadRequest {
                    url: " ".into(),
                    start: None,
                    headers: Vec::new(),
                })
            )
            .is_err()
        );
    }

    #[test]
    fn unknown_tracks_are_rejected() {
        let snapshot = idle();
        let id = TrackId::from_raw(4);
        assert!(validate(&snapshot, &Request::Audio(id)).is_err());
        assert!(validate(&snapshot, &Request::Subtitle(id)).is_err());
        assert!(validate(&snapshot, &Request::AddSubtitle(String::new())).is_err());
    }
}
