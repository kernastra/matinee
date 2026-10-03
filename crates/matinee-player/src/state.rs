//! One coherent playback snapshot.

#![forbid(unsafe_code)]
//!
//! Flags from the engine are reduced to a single [`PlaybackState`]. Callers
//! do not combine pause, eof, and idle themselves.

use std::time::Duration;

use crate::tracks::{Track, TrackId, TrackTable};

/// Where playback is. Seek does not have its own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Idle,
    Loading,
    Playing,
    Paused,
    Buffering,
    Ended,
    Error,
}

/// What the UI paints and what command checks read.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub state: PlaybackState,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub volume: f32,
    pub muted: bool,
    pub audio_tracks: Vec<Track>,
    pub subtitle_tracks: Vec<Track>,
    pub selected_audio: Option<TrackId>,
    pub selected_subtitle: Option<TrackId>,
    /// Pixels the engine is producing, after the software-frame cap.
    pub frame_size: Option<(u32, u32)>,
    /// Decoded picture size before the software-frame cap.
    pub source_size: Option<(u32, u32)>,
    pub error: Option<String>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            state: PlaybackState::Idle,
            position: Duration::ZERO,
            duration: None,
            volume: 1.0,
            muted: false,
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
            selected_audio: None,
            selected_subtitle: None,
            frame_size: None,
            source_size: None,
            error: None,
        }
    }
}

/// Discrete edges. Position and volume live on [`Snapshot`], not here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerEvent {
    Loaded,
    Ended,
    Error(String),
    TracksChanged,
    ResolutionChanged { width: u32, height: u32 },
}

/// Inputs that determine [`PlaybackState`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineFlags {
    pub opening: bool,
    pub idle: bool,
    pub paused: bool,
    pub caching: bool,
    pub eof: bool,
    pub failed: bool,
}

pub fn derive_state(flags: &EngineFlags) -> PlaybackState {
    if flags.failed {
        PlaybackState::Error
    } else if flags.opening {
        PlaybackState::Loading
    } else if flags.idle {
        PlaybackState::Idle
    } else if flags.eof {
        PlaybackState::Ended
    } else if flags.caching {
        PlaybackState::Buffering
    } else if flags.paused {
        PlaybackState::Paused
    } else {
        PlaybackState::Playing
    }
}

/// Engine observations accumulated between commands.
#[derive(Clone, Debug)]
pub struct Session {
    pub opening: bool,
    pub idle: bool,
    pub paused: bool,
    pub caching: bool,
    pub eof: bool,
    pub error: Option<String>,
    pub duration: Option<Duration>,
    pub volume: f32,
    pub muted: bool,
    pub tracks: TrackTable,
    pub source_size: Option<(u32, u32)>,
    pub frame_size: Option<(u32, u32)>,
    loaded_for_open: bool,
    ended_for_open: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            opening: false,
            idle: true,
            paused: false,
            caching: false,
            eof: false,
            error: None,
            duration: None,
            volume: 1.0,
            muted: false,
            tracks: TrackTable::new(),
            source_size: None,
            frame_size: None,
            loaded_for_open: false,
            ended_for_open: false,
        }
    }
}

impl Session {
    pub fn state(&self) -> PlaybackState {
        derive_state(&EngineFlags {
            opening: self.opening,
            idle: self.idle,
            paused: self.paused,
            caching: self.caching,
            eof: self.eof,
            failed: self.error.is_some(),
        })
    }

    pub fn snapshot(&self, position: Duration) -> Snapshot {
        Snapshot {
            state: self.state(),
            position,
            duration: self.duration,
            volume: self.volume,
            muted: self.muted,
            audio_tracks: self.tracks.audio(),
            subtitle_tracks: self.tracks.subtitles(),
            selected_audio: self.tracks.selected(crate::tracks::TrackKind::Audio),
            selected_subtitle: self.tracks.selected(crate::tracks::TrackKind::Subtitle),
            frame_size: self.frame_size,
            source_size: self.source_size,
            error: self.error.clone(),
        }
    }

    pub fn begin_load(&mut self) -> Vec<PlayerEvent> {
        self.opening = true;
        self.idle = false;
        self.paused = false;
        self.caching = false;
        self.eof = false;
        self.error = None;
        self.duration = None;
        self.source_size = None;
        self.frame_size = None;
        self.tracks.clear();
        self.loaded_for_open = false;
        self.ended_for_open = false;
        vec![PlayerEvent::TracksChanged]
    }

    pub fn note_file_loaded(&mut self) -> Vec<PlayerEvent> {
        if self.loaded_for_open {
            return Vec::new();
        }
        self.loaded_for_open = true;
        vec![PlayerEvent::Loaded]
    }

    pub fn note_restart(&mut self) {
        self.opening = false;
        self.idle = false;
    }

    /// Play was requested after the file had ended. The next end may emit again.
    pub fn resume_from_end(&mut self) {
        self.eof = false;
        self.ended_for_open = false;
    }

    pub fn note_end(&mut self, reason: i32, error: i32) -> Vec<PlayerEvent> {
        self.opening = false;
        // libmpv end-file reasons: 0 eof, 2 stop, 3 quit, 4 error, 5 redirect.
        if reason == 4 || error < 0 {
            let message = format!("playback ended ({reason}, {error})");
            self.error = Some(message.clone());
            self.eof = false;
            return vec![PlayerEvent::Error(message)];
        }
        if reason == 0 {
            self.eof = true;
            self.idle = false;
            if self.ended_for_open {
                return Vec::new();
            }
            self.ended_for_open = true;
            return vec![PlayerEvent::Ended];
        }
        if reason == 2 || reason == 3 {
            self.idle = true;
            self.eof = false;
            self.paused = false;
        }
        Vec::new()
    }

    pub fn note_stopped(&mut self) {
        self.opening = false;
        self.idle = true;
        self.eof = false;
        self.paused = false;
        self.caching = false;
        self.error = None;
    }

    pub fn note_source_size(&mut self, width: u32, height: u32) -> Vec<PlayerEvent> {
        let next = Some((width, height));
        if self.source_size == next {
            return Vec::new();
        }
        self.source_size = next;
        vec![PlayerEvent::ResolutionChanged { width, height }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(mutate: impl FnOnce(&mut EngineFlags)) -> PlaybackState {
        let mut flags = EngineFlags {
            opening: false,
            idle: false,
            paused: false,
            caching: false,
            eof: false,
            failed: false,
        };
        mutate(&mut flags);
        derive_state(&flags)
    }

    #[test]
    fn state_priority_is_coherent() {
        assert_eq!(flags(|f| f.idle = true), PlaybackState::Idle);
        assert_eq!(flags(|f| f.opening = true), PlaybackState::Loading);
        assert_eq!(
            flags(|f| {
                f.opening = true;
                f.failed = true;
            }),
            PlaybackState::Error
        );
        assert_eq!(flags(|f| f.eof = true), PlaybackState::Ended);
        assert_eq!(
            flags(|f| {
                f.eof = true;
                f.paused = true;
            }),
            PlaybackState::Ended
        );
        assert_eq!(flags(|f| f.caching = true), PlaybackState::Buffering);
        assert_eq!(
            flags(|f| {
                f.caching = true;
                f.paused = true;
            }),
            PlaybackState::Buffering
        );
        assert_eq!(flags(|f| f.paused = true), PlaybackState::Paused);
        assert_eq!(flags(|_| {}), PlaybackState::Playing);
    }

    #[test]
    fn load_resets_and_end_emits_once() {
        let mut session = Session::default();
        assert_eq!(session.begin_load(), vec![PlayerEvent::TracksChanged]);
        assert_eq!(session.state(), PlaybackState::Loading);
        assert_eq!(session.note_file_loaded(), vec![PlayerEvent::Loaded]);
        assert!(session.note_file_loaded().is_empty());
        session.note_restart();
        assert_eq!(session.state(), PlaybackState::Playing);
        assert_eq!(session.note_end(0, 0), vec![PlayerEvent::Ended]);
        assert!(session.note_end(0, 0).is_empty());
        assert_eq!(session.state(), PlaybackState::Ended);
        let error = session.note_end(4, -2);
        assert!(matches!(error[0], PlayerEvent::Error(_)));
        assert_eq!(session.state(), PlaybackState::Error);
    }
}
