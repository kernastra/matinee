//! Authoritative Player state.
//!
//! The screen applies [`Directive`]s. It does not decide when to seek, what
//! to report, or which source to invent. Snapshot state remains the playback
//! state once a file is open.

use std::fmt;
use std::time::{Duration, Instant};

use matinee_core::{
    ItemId, MediaSourceId, MediaStream, PlaySessionId, PlaybackMethod, PlaybackPlan,
    PlaybackReport, ReportKind, StreamAuthorization,
};
use matinee_jellyfin::redact_freeform as redact_message;
use matinee_player::{PlaybackState, PlayerEvent, Snapshot, Track, TrackId, TrackKind};

use super::prepare::resume_start;
use super::{COMPLETED_TAIL, CONTROLS_IDLE, PROGRESS_INTERVAL, SEEK_THROTTLE};

/// How long a requested play or pause, or a seek target, outranks a snapshot
/// that has not caught up yet. Engine commands are applied asynchronously.
const COMMAND_SETTLE: Duration = Duration::from_millis(1500);

/// A snapshot this close to the seek target means the engine has arrived.
const SEEK_ARRIVED: Duration = Duration::from_millis(1500);

const LIBRARY_MISSING: &str = "Native playback needs libmpv. Packaging that library is not finished yet, so this build cannot play video. The rest of Matinee is still available.";
const LIBRARY_INCOMPATIBLE: &str = "This libmpv build cannot be used for playback. Packaging a compatible library is not finished yet.";

/// Fixture screens. They never open a socket or libmpv.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerPreview {
    Playing,
    Paused,
    Controls,
    Error,
    AudioMenu,
    SubtitleMenu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Closed,
    Resolving,
    Loading,
    Live,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailureKind {
    LibraryMissing,
    LibraryIncompatible,
    PlaybackInfo,
    NoCompatibleSource,
    Stream,
    Engine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlayerFailure {
    pub kind: FailureKind,
    pub message: String,
}

impl PlayerFailure {
    pub(crate) fn library_missing() -> Self {
        Self {
            kind: FailureKind::LibraryMissing,
            message: LIBRARY_MISSING.into(),
        }
    }

    pub(crate) fn library_incompatible() -> Self {
        Self {
            kind: FailureKind::LibraryIncompatible,
            message: LIBRARY_INCOMPATIBLE.into(),
        }
    }

    pub(crate) fn engine(message: impl Into<String>) -> Self {
        Self {
            kind: FailureKind::Engine,
            message: redact_message(&message.into()),
        }
    }

    pub(crate) fn stream(message: impl Into<String>) -> Self {
        Self {
            kind: FailureKind::Stream,
            message: redact_message(&message.into()),
        }
    }
}

/// Why a plan did not become a load. Mapped from the Jellyfin client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlanFailure {
    Info(String),
    Incompatible(String),
    Stream(String),
}

impl PlanFailure {
    pub(crate) fn info(message: impl Into<String>) -> Self {
        Self::Info(redact_message(&message.into()))
    }

    pub(crate) fn incompatible(message: impl Into<String>) -> Self {
        Self::Incompatible(redact_message(&message.into()))
    }

    pub(crate) fn stream(message: impl Into<String>) -> Self {
        Self::Stream(redact_message(&message.into()))
    }

    fn into_failure(self) -> PlayerFailure {
        match self {
            Self::Info(message) => PlayerFailure {
                kind: FailureKind::PlaybackInfo,
                message,
            },
            Self::Incompatible(message) => PlayerFailure {
                kind: FailureKind::NoCompatibleSource,
                message,
            },
            Self::Stream(message) => PlayerFailure {
                kind: FailureKind::Stream,
                message,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparedPlayback {
    pub title: String,
    pub context: Option<String>,
    pub runtime: Option<Duration>,
    pub plan: PlaybackPlan,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LoadIntent {
    pub url: String,
    pub start: Option<Duration>,
    pub authorization: StreamAuthorization,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Directive {
    Load(LoadIntent),
    Play,
    Pause,
    Stop,
    Seek(Duration),
    SeekByMs(i64),
    Volume(f32),
    Mute(bool),
    Audio(TrackId),
    Subtitle(TrackId),
    SubtitlesOff,
    Report(ReportKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuKind {
    Audio,
    Subtitles,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum UserCommand {
    TogglePlay,
    SeekByMs(i64),
    SetVolume(f32),
    NudgeVolume(f32),
    ToggleMute,
    SelectAudio(TrackId),
    SelectSubtitle(TrackId),
    SubtitlesOff,
    Scrub(Duration),
    Activity,
    OpenMenu(MenuKind),
    CloseMenu,
}

#[derive(Clone, Debug, PartialEq)]
struct PlanFacts {
    url: String,
    method: PlaybackMethod,
    source_id: Option<MediaSourceId>,
    play_session_id: Option<PlaySessionId>,
    authorization: StreamAuthorization,
    start: Duration,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
    /// Jellyfin indices of the audio streams, in container order.
    audio_indices: Vec<i32>,
    /// Jellyfin indices of the embedded subtitle streams, in container order.
    subtitle_indices: Vec<i32>,
}

#[derive(Clone, Debug, Default)]
struct Reports {
    started: bool,
    stopped: bool,
    sent_play: bool,
    pause_latched: bool,
    last_progress: Option<Instant>,
    corrected_completion: bool,
}

pub(crate) struct PlayerModel {
    stage: Stage,
    item_id: Option<ItemId>,
    title: String,
    context: Option<String>,
    plan: Option<PlanFacts>,
    failure: Option<PlayerFailure>,
    report_notice: Option<String>,
    snapshot: Snapshot,
    volume: f32,
    muted: bool,
    /// A seek target that outranks the snapshot until the engine arrives.
    scrub: Option<Duration>,
    /// Play or pause that was sent and not yet seen in a snapshot.
    expected: Option<(PlaybackState, Instant)>,
    pending_seek: bool,
    last_seek_sent: Option<Instant>,
    last_scrub: Option<Instant>,
    seek_report_due: bool,
    menu: Option<MenuKind>,
    last_activity: Instant,
    hovering_controls: bool,
    controls_pinned: bool,
    reports: Reports,
}

impl PlayerModel {
    pub(crate) fn new() -> Self {
        Self {
            stage: Stage::Closed,
            item_id: None,
            title: String::new(),
            context: None,
            plan: None,
            failure: None,
            report_notice: None,
            snapshot: Snapshot::default(),
            volume: 1.0,
            muted: false,
            scrub: None,
            expected: None,
            pending_seek: false,
            last_seek_sent: None,
            last_scrub: None,
            seek_report_due: false,
            menu: None,
            last_activity: Instant::now(),
            hovering_controls: false,
            controls_pinned: false,
            reports: Reports::default(),
        }
    }

    pub(crate) fn preview(scene: PlayerPreview) -> Self {
        let mut model = Self::new();
        model.controls_pinned = true;
        model.title = match scene {
            PlayerPreview::Controls => "Harbor Lights".into(),
            PlayerPreview::Error => "Northwind".into(),
            _ => "Northwind".into(),
        };
        model.context = match scene {
            PlayerPreview::Controls | PlayerPreview::SubtitleMenu => {
                Some("Harbor Lights · S2 E5".into())
            }
            _ => None,
        };
        match scene {
            PlayerPreview::Error => {
                model.stage = Stage::Failed;
                model.failure = Some(PlayerFailure::library_missing());
            }
            PlayerPreview::Playing
            | PlayerPreview::Paused
            | PlayerPreview::Controls
            | PlayerPreview::AudioMenu
            | PlayerPreview::SubtitleMenu => {
                model.stage = Stage::Live;
                model.snapshot = preview_snapshot(scene);
                model.volume = model.snapshot.volume;
                model.muted = model.snapshot.muted;
                model.menu = match scene {
                    PlayerPreview::AudioMenu => Some(MenuKind::Audio),
                    PlayerPreview::SubtitleMenu => Some(MenuKind::Subtitles),
                    _ => None,
                };
            }
        }
        model
    }

    pub(crate) fn stage(&self) -> Stage {
        self.stage
    }

    pub(crate) fn item_id(&self) -> Option<&ItemId> {
        self.item_id.as_ref()
    }

    pub(crate) fn failure(&self) -> Option<&PlayerFailure> {
        self.failure.as_ref()
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    pub(crate) fn menu(&self) -> Option<MenuKind> {
        self.menu
    }

    pub(crate) fn report_notice(&self) -> Option<&str> {
        self.report_notice.as_deref()
    }

    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub(crate) fn volume(&self) -> f32 {
        self.volume
    }

    pub(crate) fn muted(&self) -> bool {
        self.muted
    }

    pub(crate) fn method_label(&self) -> Option<&'static str> {
        self.plan.as_ref().map(|plan| match plan.method {
            PlaybackMethod::DirectPlay => "Direct play",
            PlaybackMethod::DirectStream => "Remux",
            PlaybackMethod::Transcode => "Transcode",
        })
    }

    pub(crate) fn playback_state(&self) -> Option<PlaybackState> {
        match self.stage {
            Stage::Live => Some(self.snapshot.state),
            Stage::Loading => Some(PlaybackState::Loading),
            Stage::Failed => Some(PlaybackState::Error),
            Stage::Resolving => Some(PlaybackState::Loading),
            Stage::Closed => None,
        }
    }

    pub(crate) fn transport_enabled(&self) -> bool {
        self.stage == Stage::Live
            && !matches!(
                self.snapshot.state,
                PlaybackState::Idle | PlaybackState::Error | PlaybackState::Loading
            )
    }

    pub(crate) fn timeline(&self) -> Timeline {
        let position = self.scrub.unwrap_or(self.snapshot.position);
        timeline(position, self.snapshot.duration)
    }

    pub(crate) fn controls_visible(&self, now: Instant) -> bool {
        if self.controls_pinned || self.menu.is_some() || self.hovering_controls {
            return true;
        }
        match self.stage {
            Stage::Closed => false,
            Stage::Resolving | Stage::Loading | Stage::Failed => true,
            Stage::Live => match self.snapshot.state {
                PlaybackState::Playing => {
                    now.saturating_duration_since(self.last_activity) < CONTROLS_IDLE
                }
                _ => true,
            },
        }
    }

    /// Everything the screen paints from the model. Equal keys need no repaint.
    pub(crate) fn view_key(&self, now: Instant) -> ViewKey {
        ViewKey {
            stage: self.stage,
            snapshot: self.snapshot.clone(),
            timeline: self.timeline(),
            controls: self.controls_visible(now),
            menu: self.menu,
            volume: self.volume,
            muted: self.muted,
            notice: self.report_notice.clone(),
            failure: self.failure.clone(),
        }
    }

    pub(crate) fn set_controls_hovered(&mut self, hovered: bool) {
        self.hovering_controls = hovered;
    }

    pub(crate) fn begin(&mut self, item_id: ItemId) {
        *self = Self::new();
        self.stage = Stage::Resolving;
        self.item_id = Some(item_id);
        self.title = "Opening".into();
    }

    pub(crate) fn accept_plan(&mut self, prepared: PreparedPlayback) -> Vec<Directive> {
        if self.stage != Stage::Resolving {
            return Vec::new();
        }
        let start = resume_start(prepared.plan.start_position, prepared.runtime);
        // `matinee-jellyfin` already removed credential query pairs. The load
        // authorizes with the session header.
        let url = prepared.plan.url.clone();
        let (audio_indices, subtitle_indices) = stream_indices(&prepared.plan.streams);
        self.title = prepared.title;
        self.context = prepared.context;
        self.plan = Some(PlanFacts {
            url: url.clone(),
            method: prepared.plan.method,
            source_id: prepared.plan.source_id.clone(),
            play_session_id: prepared.plan.play_session_id.clone(),
            authorization: prepared.plan.authorization,
            start: start.unwrap_or(Duration::ZERO),
            audio_stream_index: prepared.plan.selected_audio,
            subtitle_stream_index: prepared.plan.selected_subtitle,
            audio_indices,
            subtitle_indices,
        });
        self.stage = Stage::Loading;
        vec![Directive::Load(LoadIntent {
            url,
            start,
            authorization: prepared.plan.authorization,
        })]
    }

    pub(crate) fn reject_plan(&mut self, failure: PlanFailure) {
        if self.stage != Stage::Resolving {
            return;
        }
        self.fail(failure.into_failure());
    }

    pub(crate) fn reject_engine(&mut self, failure: PlayerFailure) {
        if matches!(self.stage, Stage::Closed | Stage::Failed) {
            return;
        }
        self.fail(failure);
    }

    pub(crate) fn fail_playback(&mut self, message: impl Into<String>) {
        if self.stage == Stage::Closed {
            return;
        }
        let kind = if self.reports.started {
            FailureKind::Stream
        } else {
            FailureKind::Engine
        };
        self.fail(PlayerFailure {
            kind,
            message: redact_message(&message.into()),
        });
    }

    pub(crate) fn ingest(&mut self, snapshot: Snapshot, now: Instant) -> Vec<Directive> {
        if !matches!(self.stage, Stage::Loading | Stage::Live) {
            return Vec::new();
        }
        self.snapshot = snapshot;
        self.volume = self.snapshot.volume;
        self.muted = self.snapshot.muted;
        self.hold_expected_state(now);
        self.release_seek_target(now);
        if self.stage == Stage::Loading
            && !matches!(
                self.snapshot.state,
                PlaybackState::Idle | PlaybackState::Loading
            )
        {
            self.stage = Stage::Live;
        }
        let mut directives = Vec::new();
        if let Some(duration) = self.snapshot.duration
            && let Some(seek) = self.completion_seek(duration)
        {
            directives.push(seek);
        }
        if self.snapshot.state == PlaybackState::Error
            && let Some(message) = self.snapshot.error.clone()
        {
            self.fail_playback(message);
        }
        directives.extend(self.state_reports(now));
        directives
    }

    pub(crate) fn ingest_event(&mut self, event: PlayerEvent, now: Instant) -> Vec<Directive> {
        if !matches!(self.stage, Stage::Loading | Stage::Live) {
            return Vec::new();
        }
        match event {
            PlayerEvent::Loaded => {
                self.stage = Stage::Live;
                let mut directives = Vec::new();
                if !self.reports.sent_play {
                    self.reports.sent_play = true;
                    directives.push(Directive::Play);
                }
                if !self.reports.started {
                    self.reports.started = true;
                    self.reports.last_progress = Some(now);
                    directives.push(Directive::Report(ReportKind::Start));
                }
                directives
            }
            PlayerEvent::Ended => {
                self.snapshot.state = PlaybackState::Ended;
                self.stage = Stage::Live;
                self.stop_report()
            }
            PlayerEvent::Error(message) => {
                self.fail_playback(message);
                Vec::new()
            }
            PlayerEvent::TracksChanged | PlayerEvent::ResolutionChanged { .. } => Vec::new(),
        }
    }

    pub(crate) fn command(&mut self, command: UserCommand, now: Instant) -> Vec<Directive> {
        if self.stage != Stage::Live {
            if matches!(command, UserCommand::Activity | UserCommand::CloseMenu) {
                self.note_activity(now);
            }
            if matches!(command, UserCommand::CloseMenu) {
                self.menu = None;
            }
            return Vec::new();
        }
        self.note_activity(now);
        match command {
            UserCommand::Activity => Vec::new(),
            UserCommand::OpenMenu(kind) => {
                self.menu = Some(kind);
                Vec::new()
            }
            UserCommand::CloseMenu => {
                self.menu = None;
                Vec::new()
            }
            UserCommand::TogglePlay => self.toggle_play(now),
            UserCommand::SeekByMs(delta) => self.seek_by(delta, now),
            UserCommand::SetVolume(volume) => self.set_volume(volume),
            UserCommand::NudgeVolume(delta) => self.set_volume(self.volume + delta),
            UserCommand::ToggleMute => {
                self.muted = !self.muted;
                vec![Directive::Mute(self.muted)]
            }
            UserCommand::SelectAudio(id) => {
                if self
                    .snapshot
                    .audio_tracks
                    .iter()
                    .any(|track| track.id == id)
                {
                    if let Some(plan) = self.plan.as_mut() {
                        plan.audio_stream_index =
                            stream_index(&plan.audio_indices, &self.snapshot.audio_tracks, id);
                    }
                    vec![Directive::Audio(id)]
                } else {
                    Vec::new()
                }
            }
            UserCommand::SelectSubtitle(id) => {
                if self
                    .snapshot
                    .subtitle_tracks
                    .iter()
                    .any(|track| track.id == id)
                {
                    if let Some(plan) = self.plan.as_mut() {
                        plan.subtitle_stream_index = stream_index(
                            &plan.subtitle_indices,
                            &self.snapshot.subtitle_tracks,
                            id,
                        );
                    }
                    vec![Directive::Subtitle(id)]
                } else {
                    Vec::new()
                }
            }
            UserCommand::SubtitlesOff => {
                if let Some(plan) = self.plan.as_mut() {
                    plan.subtitle_stream_index = Some(-1);
                }
                vec![Directive::SubtitlesOff]
            }
            UserCommand::Scrub(position) => self.scrub(position, now),
        }
    }

    pub(crate) fn flush(&mut self, now: Instant) -> Vec<Directive> {
        let mut directives = Vec::new();
        if self.pending_seek && self.seek_due(now) {
            self.pending_seek = false;
            if let Some(position) = self.scrub {
                self.last_seek_sent = Some(now);
                self.seek_report_due = true;
                directives.push(Directive::Seek(position));
            }
        }
        let settled = self
            .last_scrub
            .is_some_and(|scrubbed| now.duration_since(scrubbed) >= SEEK_THROTTLE);
        if self.seek_report_due && self.pending_seek {
            // Still dragging. The report waits until the pointer settles.
        } else if self.seek_report_due && settled {
            self.seek_report_due = false;
            directives.push(self.mark_progress(now));
        }
        directives
    }

    pub(crate) fn progress_tick(&mut self, now: Instant) -> Vec<Directive> {
        if self.stage != Stage::Live || self.snapshot.state != PlaybackState::Playing {
            return Vec::new();
        }
        if !self.reports.started || self.reports.stopped {
            return Vec::new();
        }
        let due = self
            .reports
            .last_progress
            .is_none_or(|last| now.duration_since(last) >= PROGRESS_INTERVAL);
        if !due {
            return Vec::new();
        }
        vec![self.mark_progress(now)]
    }

    pub(crate) fn note_report(&mut self, result: Result<(), String>) {
        self.report_notice = result.err().map(|message| redact_message(&message));
    }

    /// Stop reporting and release the engine. A second call is empty.
    ///
    /// The stop report body is captured here, before close resets anything,
    /// so it carries the last position, seek target, and track choices.
    /// `report` is present only when this close owes Jellyfin a stop.
    pub(crate) fn close(&mut self) -> Closing {
        if self.stage == Stage::Closed {
            return Closing::default();
        }
        let mut directives = self.stop_report();
        let report = if directives.is_empty() {
            None
        } else {
            self.report_body()
        };
        directives.push(Directive::Stop);
        self.stage = Stage::Closed;
        self.menu = None;
        self.scrub = None;
        Closing { directives, report }
    }

    /// Take position, volume, and mute from the engine's last snapshot just
    /// before close. An idle, failed, or empty snapshot is ignored so teardown
    /// never reports position zero in place of the last real one. Play state
    /// stays as the model holds it, including a pause that is still settling.
    pub(crate) fn note_final_snapshot(&mut self, snapshot: &Snapshot) {
        if self.stage != Stage::Live
            || !matches!(
                snapshot.state,
                PlaybackState::Playing
                    | PlaybackState::Paused
                    | PlaybackState::Buffering
                    | PlaybackState::Ended
            )
        {
            return;
        }
        self.snapshot.position = snapshot.position;
        self.volume = snapshot.volume;
        self.muted = snapshot.muted;
    }

    pub(crate) fn report_body(&self) -> Option<PlaybackReport> {
        let plan = self.plan.as_ref()?;
        let item_id = self.item_id.clone()?;
        let paused = self.snapshot.state != PlaybackState::Playing;
        Some(PlaybackReport {
            item_id,
            media_source_id: plan.source_id.clone(),
            play_session_id: plan.play_session_id.clone(),
            position: self.timeline().position,
            paused,
            muted: self.muted,
            volume: self.volume,
            audio_stream_index: plan.audio_stream_index,
            subtitle_stream_index: plan.subtitle_stream_index,
            method: plan.method,
        })
    }

    /// Text the player is allowed to paint. URLs and tokens are absent.
    #[cfg(test)]
    pub(crate) fn visible_lines(&self) -> Vec<String> {
        let mut lines = vec![self.title.clone()];
        if let Some(context) = &self.context {
            lines.push(context.clone());
        }
        if let Some(method) = self.method_label() {
            lines.push(method.into());
        }
        if let Some(failure) = &self.failure {
            lines.push(failure.message.clone());
        }
        if let Some(notice) = &self.report_notice {
            lines.push(notice.clone());
        }
        match self.playback_state() {
            Some(PlaybackState::Loading) => lines.push("Opening video…".into()),
            Some(PlaybackState::Buffering) => lines.push("Buffering…".into()),
            Some(PlaybackState::Ended) => lines.push("Playback finished".into()),
            _ => {}
        }
        lines.push(format_clock(self.timeline().position));
        lines.push(match self.snapshot.duration {
            Some(duration) if !duration.is_zero() => format_clock(duration),
            _ => "—".into(),
        });
        for (index, track) in self.snapshot.audio_tracks.iter().enumerate() {
            lines.push(track_label("Audio", index, track));
        }
        lines.push("Off".into());
        for (index, track) in self.snapshot.subtitle_tracks.iter().enumerate() {
            lines.push(track_label("Subtitle", index, track));
        }
        lines.push(if self.snapshot.state == PlaybackState::Playing {
            "Pause".into()
        } else {
            "Play".into()
        });
        lines.push("Back".into());
        lines
    }

    fn fail(&mut self, failure: PlayerFailure) {
        self.failure = Some(failure);
        self.stage = Stage::Failed;
        self.menu = None;
    }

    fn toggle_play(&mut self, now: Instant) -> Vec<Directive> {
        match self.snapshot.state {
            PlaybackState::Playing | PlaybackState::Buffering => {
                self.snapshot.state = PlaybackState::Paused;
                self.expected = Some((PlaybackState::Paused, now));
                self.reports.pause_latched = true;
                vec![Directive::Pause, self.mark_progress(now)]
            }
            PlaybackState::Paused | PlaybackState::Ended => {
                if self.snapshot.state == PlaybackState::Ended {
                    // The engine rewinds to the start when play follows the end.
                    self.hold_seek_target(Duration::ZERO, now);
                }
                let resume = self.reports.pause_latched;
                self.snapshot.state = PlaybackState::Playing;
                self.expected = Some((PlaybackState::Playing, now));
                self.reports.pause_latched = false;
                let mut directives = vec![Directive::Play];
                if self.reports.stopped {
                    // Stop was already reported. Watching again is a new session.
                    self.reports.stopped = false;
                    self.reports.started = true;
                    self.reports.last_progress = Some(now);
                    directives.push(Directive::Report(ReportKind::Start));
                } else if resume {
                    directives.push(self.mark_progress(now));
                }
                directives
            }
            _ => Vec::new(),
        }
    }

    /// Relative seeks become absolute targets so the clock and the report
    /// show where playback is going, not where it was.
    fn seek_by(&mut self, delta_ms: i64, now: Instant) -> Vec<Directive> {
        let Timeline {
            position,
            duration: Some(duration),
            enabled: true,
            ..
        } = self.timeline()
        else {
            return vec![Directive::SeekByMs(delta_ms)];
        };
        let step = Duration::from_millis(delta_ms.unsigned_abs());
        let target = if delta_ms < 0 {
            position.saturating_sub(step)
        } else {
            (position + step).min(duration)
        };
        self.hold_seek_target(target, now);
        self.last_seek_sent = Some(now);
        self.seek_report_due = true;
        vec![Directive::Seek(target)]
    }

    fn hold_seek_target(&mut self, target: Duration, now: Instant) {
        self.scrub = Some(target);
        self.last_scrub = Some(now);
    }

    /// Drop the seek target once the engine reaches it, or after it has had
    /// time to. A drag in progress or an unsent report keeps it.
    fn release_seek_target(&mut self, now: Instant) {
        let Some(target) = self.scrub else {
            return;
        };
        if self.pending_seek || self.seek_report_due {
            return;
        }
        let arrived = self.snapshot.position.abs_diff(target) <= SEEK_ARRIVED;
        let expired = self
            .last_scrub
            .is_none_or(|since| now.saturating_duration_since(since) >= COMMAND_SETTLE);
        if arrived || expired {
            self.scrub = None;
        }
    }

    /// Keep a requested play or pause until a snapshot agrees, so a stale
    /// snapshot does not flip the button or send a contradicting report.
    fn hold_expected_state(&mut self, now: Instant) {
        let Some((expected, since)) = self.expected else {
            return;
        };
        let settled = now.saturating_duration_since(since) >= COMMAND_SETTLE;
        let stale = matches!(
            self.snapshot.state,
            PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended
        ) && self.snapshot.state != expected;
        if stale && !settled {
            self.snapshot.state = expected;
        } else {
            self.expected = None;
        }
    }

    fn set_volume(&mut self, volume: f32) -> Vec<Directive> {
        if !volume.is_finite() {
            return Vec::new();
        }
        self.volume = volume.clamp(0.0, 1.0);
        self.muted = false;
        vec![Directive::Volume(self.volume), Directive::Mute(false)]
    }

    fn scrub(&mut self, position: Duration, now: Instant) -> Vec<Directive> {
        let Timeline {
            position, enabled, ..
        } = timeline(position, self.snapshot.duration);
        if !enabled {
            return Vec::new();
        }
        self.scrub = Some(position);
        self.last_scrub = Some(now);
        if self.seek_due(now) {
            self.pending_seek = false;
            self.last_seek_sent = Some(now);
            self.seek_report_due = true;
            vec![Directive::Seek(position)]
        } else {
            self.pending_seek = true;
            self.seek_report_due = true;
            Vec::new()
        }
    }

    fn seek_due(&self, now: Instant) -> bool {
        self.last_seek_sent
            .is_none_or(|sent| now.duration_since(sent) >= SEEK_THROTTLE)
    }

    fn completion_seek(&mut self, duration: Duration) -> Option<Directive> {
        let plan = self.plan.as_ref()?;
        if self.reports.corrected_completion || plan.start.is_zero() {
            return None;
        }
        if plan.start + COMPLETED_TAIL < duration {
            return None;
        }
        self.reports.corrected_completion = true;
        if let Some(plan) = self.plan.as_mut() {
            plan.start = Duration::ZERO;
        }
        Some(Directive::Seek(Duration::ZERO))
    }

    fn state_reports(&mut self, now: Instant) -> Vec<Directive> {
        if self.stage != Stage::Live || !self.reports.started || self.reports.stopped {
            return Vec::new();
        }
        match self.snapshot.state {
            PlaybackState::Paused if !self.reports.pause_latched => {
                self.reports.pause_latched = true;
                vec![self.mark_progress(now)]
            }
            PlaybackState::Playing if self.reports.pause_latched => {
                self.reports.pause_latched = false;
                vec![self.mark_progress(now)]
            }
            PlaybackState::Ended => self.stop_report(),
            _ => Vec::new(),
        }
    }

    fn stop_report(&mut self) -> Vec<Directive> {
        if !self.reports.started || self.reports.stopped {
            return Vec::new();
        }
        self.reports.stopped = true;
        vec![Directive::Report(ReportKind::Stopped)]
    }

    fn mark_progress(&mut self, now: Instant) -> Directive {
        self.reports.last_progress = Some(now);
        Directive::Report(ReportKind::Progress)
    }

    fn note_activity(&mut self, now: Instant) {
        self.last_activity = now;
    }
}

impl fmt::Debug for PlayerModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlayerModel")
            .field("stage", &self.stage)
            .field("title", &self.title)
            .field("context", &self.context)
            .field("method", &self.method_label())
            .field("failure", &self.failure)
            .field("report_notice", &self.report_notice)
            .field("state", &self.snapshot.state)
            .finish()
    }
}

/// What closing the Player owes the engine and Jellyfin.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Closing {
    pub directives: Vec<Directive>,
    /// The stop report body, captured before close reset the model.
    pub report: Option<PlaybackReport>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViewKey {
    stage: Stage,
    snapshot: Snapshot,
    timeline: Timeline,
    controls: bool,
    menu: Option<MenuKind>,
    volume: f32,
    muted: bool,
    notice: Option<String>,
    failure: Option<PlayerFailure>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Timeline {
    pub position: Duration,
    pub duration: Option<Duration>,
    pub ratio: f32,
    pub enabled: bool,
}

pub(crate) fn timeline(position: Duration, duration: Option<Duration>) -> Timeline {
    let Some(duration) = duration.filter(|duration| !duration.is_zero()) else {
        return Timeline {
            position: Duration::ZERO,
            duration: None,
            ratio: 0.0,
            enabled: false,
        };
    };
    let position = if position > duration {
        duration
    } else {
        position
    };
    let ratio = (position.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0);
    Timeline {
        position,
        duration: Some(duration),
        ratio,
        enabled: true,
    }
}

/// Jellyfin indices of the audio and embedded subtitle streams, in container
/// order. External subtitles are not in the file the engine opened.
fn stream_indices(streams: &[MediaStream]) -> (Vec<i32>, Vec<i32>) {
    let mut audio = Vec::new();
    let mut subtitles = Vec::new();
    for stream in streams {
        match stream {
            MediaStream::Audio(audio_stream) => audio.push(audio_stream.index),
            MediaStream::Subtitle(subtitle) if !subtitle.external => subtitles.push(subtitle.index),
            _ => {}
        }
    }
    audio.sort_unstable();
    subtitles.sort_unstable();
    (audio, subtitles)
}

/// The Jellyfin index for an engine track.
///
/// Tracks are matched by position, and only when the engine lists as many
/// tracks as Jellyfin does. A transcode can carry fewer tracks than the
/// source, so an uncertain match reports no index instead of a wrong one.
fn stream_index(indices: &[i32], tracks: &[Track], id: TrackId) -> Option<i32> {
    if indices.len() != tracks.len() {
        return None;
    }
    let position = tracks.iter().position(|track| track.id == id)?;
    indices.get(position).copied()
}

pub(crate) fn format_clock(duration: Duration) -> String {
    let whole = duration.as_secs();
    let hours = whole / 3600;
    let minutes = (whole % 3600) / 60;
    let seconds = whole % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

pub(crate) fn track_label(kind: &str, index: usize, track: &Track) -> String {
    let title = track
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let language = track
        .language
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (title, language) {
        (Some(title), Some(language)) => format!("{title} · {language}"),
        (Some(title), None) => title.to_string(),
        (None, Some(language)) => format!("{kind} · {language}"),
        (None, None) => format!("{kind} {}", index + 1),
    }
}

fn preview_snapshot(scene: PlayerPreview) -> Snapshot {
    let mut snapshot = Snapshot {
        state: if scene == PlayerPreview::Paused {
            PlaybackState::Paused
        } else {
            PlaybackState::Playing
        },
        position: Duration::from_secs(12 * 60 + 4),
        duration: Some(Duration::from_secs(102 * 60)),
        volume: 0.8,
        muted: false,
        audio_tracks: vec![
            sample_track(1, TrackKind::Audio, Some("Dialogue"), Some("eng")),
            sample_track(2, TrackKind::Audio, Some("Commentary"), Some("eng")),
        ],
        subtitle_tracks: vec![
            sample_track(3, TrackKind::Subtitle, None, Some("eng")),
            sample_track(4, TrackKind::Subtitle, Some("Full"), Some("spa")),
        ],
        selected_audio: Some(TrackId::from_raw(1)),
        selected_subtitle: if scene == PlayerPreview::SubtitleMenu {
            Some(TrackId::from_raw(3))
        } else {
            None
        },
        frame_size: Some((640, 360)),
        source_size: Some((640, 360)),
        error: None,
    };
    if scene == PlayerPreview::Controls {
        snapshot.position = Duration::from_secs(48 * 60);
    }
    snapshot
}

fn sample_track(id: u64, kind: TrackKind, title: Option<&str>, language: Option<&str>) -> Track {
    Track {
        id: TrackId::from_raw(id),
        kind,
        form: None,
        language: language.map(str::to_string),
        title: title.map(str::to_string),
        codec: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matinee_core::{ItemId, PlaybackMethod, StreamAuthorization};

    const TOKEN: &str = "token-value-not-for-logs";
    const SEEK_STEP: i64 = 10_000;

    fn item() -> ItemId {
        ItemId::parse("item-1").unwrap()
    }

    fn plan(method: PlaybackMethod, start: Duration) -> PreparedPlayback {
        PreparedPlayback {
            title: "Northwind".into(),
            context: None,
            runtime: Some(Duration::from_secs(3600)),
            plan: PlaybackPlan {
                // `matinee-jellyfin` strips credential query pairs before a plan exists.
                url: "https://jellyfin.local/Videos/item-1/stream?Static=true".into(),
                source_id: None,
                play_session_id: None,
                method,
                streams: Vec::new(),
                selected_audio: Some(1),
                selected_subtitle: None,
                start_position: start,
                authorization: StreamAuthorization::Session,
            },
        }
    }

    fn playing(position: Duration) -> Snapshot {
        Snapshot {
            state: PlaybackState::Playing,
            position,
            duration: Some(Duration::from_secs(3600)),
            volume: 1.0,
            muted: false,
            ..Snapshot::default()
        }
    }

    fn open_live() -> (PlayerModel, Instant) {
        let now = Instant::now();
        let mut model = PlayerModel::new();
        model.begin(item());
        let directives = model.accept_plan(plan(PlaybackMethod::DirectPlay, Duration::ZERO));
        assert!(matches!(directives[0], Directive::Load(_)));
        let loaded = model.ingest_event(PlayerEvent::Loaded, now);
        assert!(loaded.contains(&Directive::Play));
        assert!(loaded.contains(&Directive::Report(ReportKind::Start)));
        model.ingest(playing(Duration::from_secs(4)), now);
        (model, now)
    }

    #[test]
    fn states_follow_the_snapshot() {
        let mut model = PlayerModel::new();
        assert_eq!(model.stage(), Stage::Closed);
        model.begin(item());
        assert_eq!(model.playback_state(), Some(PlaybackState::Loading));
        model.accept_plan(plan(PlaybackMethod::DirectPlay, Duration::ZERO));
        assert_eq!(model.stage(), Stage::Loading);
        let now = Instant::now();
        model.ingest_event(PlayerEvent::Loaded, now);
        model.ingest(playing(Duration::from_secs(1)), now);
        assert_eq!(model.playback_state(), Some(PlaybackState::Playing));
        model.ingest(
            Snapshot {
                state: PlaybackState::Paused,
                ..playing(Duration::from_secs(2))
            },
            now,
        );
        assert_eq!(model.playback_state(), Some(PlaybackState::Paused));
        model.ingest(
            Snapshot {
                state: PlaybackState::Buffering,
                ..playing(Duration::from_secs(2))
            },
            now,
        );
        assert_eq!(model.playback_state(), Some(PlaybackState::Buffering));
        model.ingest_event(PlayerEvent::Ended, now);
        assert_eq!(model.playback_state(), Some(PlaybackState::Ended));
        model.fail_playback("The connection to Jellyfin was interrupted.");
        assert_eq!(model.stage(), Stage::Failed);
        assert_eq!(model.playback_state(), Some(PlaybackState::Error));
        model.close();
        assert_eq!(model.stage(), Stage::Closed);
        assert_eq!(model.close(), Closing::default());
    }

    #[test]
    fn commands_map_to_engine_directives() {
        let (mut model, now) = open_live();
        assert_eq!(
            model.command(UserCommand::TogglePlay, now),
            vec![Directive::Pause, Directive::Report(ReportKind::Progress)]
        );
        assert_eq!(
            model.command(UserCommand::TogglePlay, now),
            vec![Directive::Play, Directive::Report(ReportKind::Progress)]
        );
        assert_eq!(
            model.command(UserCommand::SeekByMs(-10_000), now),
            vec![Directive::Seek(Duration::ZERO)],
            "a relative seek becomes a clamped absolute target"
        );
        assert_eq!(
            model.command(UserCommand::SetVolume(0.25), now),
            vec![Directive::Volume(0.25), Directive::Mute(false)]
        );
        assert_eq!(
            model.command(UserCommand::NudgeVolume(2.0), now)[0],
            Directive::Volume(1.0)
        );
        assert_eq!(
            model.command(UserCommand::ToggleMute, now),
            vec![Directive::Mute(true)]
        );
        let audio = TrackId::from_raw(7);
        let subtitle = TrackId::from_raw(8);
        model.snapshot.audio_tracks = vec![sample_track(7, TrackKind::Audio, None, Some("eng"))];
        model.snapshot.subtitle_tracks =
            vec![sample_track(8, TrackKind::Subtitle, None, Some("eng"))];
        assert_eq!(
            model.command(UserCommand::SelectAudio(audio), now),
            vec![Directive::Audio(audio)]
        );
        assert_eq!(
            model.command(UserCommand::SelectSubtitle(subtitle), now),
            vec![Directive::Subtitle(subtitle)]
        );
        assert_eq!(
            model.command(UserCommand::SubtitlesOff, now),
            vec![Directive::SubtitlesOff]
        );
        assert!(
            model
                .command(UserCommand::SelectAudio(TrackId::from_raw(99)), now)
                .is_empty()
        );
        let labels = model.visible_lines().join("\n");
        assert!(labels.contains("Audio · eng"));
        assert!(!labels.contains("mpv"));
    }

    #[test]
    fn each_plan_method_loads_without_a_fallback_url() {
        for method in [
            PlaybackMethod::DirectPlay,
            PlaybackMethod::DirectStream,
            PlaybackMethod::Transcode,
        ] {
            let mut model = PlayerModel::new();
            model.begin(item());
            let directives = model.accept_plan(plan(method, Duration::from_secs(12)));
            let Directive::Load(intent) = &directives[0] else {
                panic!("expected a load");
            };
            assert_eq!(intent.authorization, StreamAuthorization::Session);
            assert_eq!(intent.start, Some(Duration::from_secs(12)));
            assert!(!intent.url.contains("api_key"));
            assert!(!intent.url.contains(TOKEN));
            assert!(intent.url.contains("/Videos/item-1/stream"));
            assert!(model.method_label().is_some());
        }
        let mut model = PlayerModel::new();
        model.begin(item());
        model.reject_plan(PlanFailure::incompatible(
            "Jellyfin could not create a compatible video stream.",
        ));
        assert_eq!(
            model.failure().unwrap().kind,
            FailureKind::NoCompatibleSource
        );
        assert_eq!(model.stage(), Stage::Failed);
        let rendered = format!("{model:?}\n{}", model.visible_lines().join("\n"));
        assert!(!rendered.contains(TOKEN));
        assert!(!rendered.contains("api_key"));
    }

    #[test]
    fn resume_and_completion_use_the_plan_position() {
        let mut unwatched = PlayerModel::new();
        unwatched.begin(item());
        let directives = unwatched.accept_plan(plan(PlaybackMethod::DirectPlay, Duration::ZERO));
        let Directive::Load(intent) = &directives[0] else {
            panic!("load");
        };
        assert_eq!(intent.start, None);

        let mut partial = PlayerModel::new();
        partial.begin(item());
        let directives =
            partial.accept_plan(plan(PlaybackMethod::DirectPlay, Duration::from_secs(90)));
        let Directive::Load(intent) = &directives[0] else {
            panic!("load");
        };
        assert_eq!(intent.start, Some(Duration::from_secs(90)));

        let mut finished = PlayerModel::new();
        finished.begin(item());
        let mut prepared = plan(PlaybackMethod::DirectPlay, Duration::from_secs(3590));
        prepared.runtime = Some(Duration::from_secs(3600));
        let directives = finished.accept_plan(prepared);
        let Directive::Load(intent) = &directives[0] else {
            panic!("load");
        };
        assert_eq!(
            intent.start, None,
            "a resume in the last 30 seconds starts over"
        );

        let mut unknown = PlayerModel::new();
        unknown.begin(item());
        let mut prepared = plan(PlaybackMethod::DirectPlay, Duration::from_secs(110));
        prepared.runtime = None;
        unknown.accept_plan(prepared);
        let now = Instant::now();
        unknown.ingest_event(PlayerEvent::Loaded, now);
        let directives = unknown.ingest(
            Snapshot {
                duration: Some(Duration::from_secs(120)),
                state: PlaybackState::Playing,
                ..Snapshot::default()
            },
            now,
        );
        assert!(directives.contains(&Directive::Seek(Duration::ZERO)));
    }

    #[test]
    fn timeline_edges_and_scrub_throttle() {
        assert!(!timeline(Duration::from_secs(3), Some(Duration::ZERO)).enabled);
        assert!(!timeline(Duration::from_secs(3), None).enabled);
        let end = timeline(Duration::from_secs(10), Some(Duration::from_secs(10)));
        assert!((end.ratio - 1.0).abs() < f32::EPSILON);
        let clamped = timeline(Duration::from_secs(40), Some(Duration::from_secs(10)));
        assert_eq!(clamped.position, Duration::from_secs(10));

        let (mut model, now) = open_live();
        let first = model.command(UserCommand::Scrub(Duration::from_secs(10)), now);
        assert_eq!(first, vec![Directive::Seek(Duration::from_secs(10))]);
        let second = model.command(
            UserCommand::Scrub(Duration::from_secs(11)),
            now + Duration::from_millis(40),
        );
        assert!(second.is_empty(), "a drag does not seek on every move");
        let flushed = model.flush(now + SEEK_THROTTLE + Duration::from_millis(40));
        assert!(flushed.contains(&Directive::Seek(Duration::from_secs(11))));
        assert!(flushed.contains(&Directive::Report(ReportKind::Progress)));
    }

    #[test]
    fn reporting_is_immediate_for_edges_and_quiet_while_paused() {
        let (mut model, now) = open_live();
        assert!(model.progress_tick(now + Duration::from_secs(3)).is_empty());
        let periodic = model.progress_tick(now + PROGRESS_INTERVAL);
        assert_eq!(periodic, vec![Directive::Report(ReportKind::Progress)]);
        model.command(UserCommand::TogglePlay, now + PROGRESS_INTERVAL);
        assert_eq!(model.snapshot.state, PlaybackState::Paused);
        assert!(
            model
                .progress_tick(now + PROGRESS_INTERVAL + PROGRESS_INTERVAL)
                .is_empty(),
            "paused playback does not keep reporting"
        );
        model.note_report(Err(format!("HTTP 500 Token=\"{TOKEN}\"")));
        assert_eq!(model.playback_state(), Some(PlaybackState::Paused));
        assert!(model.report_notice().unwrap().contains("redacted"));
        let rendered = format!("{model:?}\n{}", model.visible_lines().join("\n"));
        assert!(!rendered.contains(TOKEN));
        model.note_report(Ok(()));
        assert_eq!(
            model.report_notice(),
            None,
            "a later successful report clears the notice"
        );
        let closing = model.close().directives;
        assert!(closing.contains(&Directive::Report(ReportKind::Stopped)));
        assert!(closing.contains(&Directive::Stop));
        assert!(
            model
                .progress_tick(now + Duration::from_secs(100))
                .is_empty()
        );
    }

    fn paused(position: Duration) -> Snapshot {
        Snapshot {
            state: PlaybackState::Paused,
            ..playing(position)
        }
    }

    fn reports(directives: &[Directive]) -> Vec<ReportKind> {
        directives
            .iter()
            .filter_map(|directive| match directive {
                Directive::Report(kind) => Some(*kind),
                _ => None,
            })
            .collect()
    }

    fn minutes(value: u64) -> Duration {
        Duration::from_secs(value * 60)
    }

    #[test]
    fn the_clock_follows_playback_after_a_scrub() {
        let (mut model, now) = open_live();
        let at = |ms: u64| now + Duration::from_millis(ms);
        assert_eq!(
            model.command(UserCommand::Scrub(minutes(30)), now),
            vec![Directive::Seek(minutes(30))]
        );
        // The engine has not moved yet. The clock and the report show the target.
        model.ingest(playing(Duration::from_secs(4)), at(100));
        assert_eq!(model.timeline().position, minutes(30));
        let settled = model.flush(at(250));
        assert_eq!(reports(&settled), vec![ReportKind::Progress]);
        assert_eq!(model.report_body().unwrap().position, minutes(30));
        // The engine arrives, then keeps playing. The clock must keep moving.
        model.ingest(playing(minutes(30) + Duration::from_secs(1)), at(500));
        model.ingest(playing(minutes(31)), at(60_000));
        assert_eq!(model.timeline().position, minutes(31));
        assert_eq!(model.report_body().unwrap().position, minutes(31));
    }

    #[test]
    fn a_seek_target_expires_if_the_engine_lands_elsewhere() {
        let (mut model, now) = open_live();
        model.command(UserCommand::Scrub(minutes(30)), now);
        model.flush(now + SEEK_THROTTLE);
        // A keyframe seek can land several seconds away from the target.
        let landed = minutes(30) - Duration::from_secs(6);
        model.ingest(playing(landed), now + Duration::from_millis(500));
        assert_eq!(model.timeline().position, minutes(30));
        model.ingest(playing(landed), now + COMMAND_SETTLE + SEEK_THROTTLE);
        assert_eq!(model.timeline().position, landed);
    }

    #[test]
    fn keyboard_seeks_report_where_playback_is_going() {
        let (mut model, now) = open_live();
        let at = |ms: u64| now + Duration::from_millis(ms);
        model.ingest(playing(minutes(10)), now);
        assert_eq!(
            model.command(UserCommand::SeekByMs(SEEK_STEP), at(10)),
            vec![Directive::Seek(minutes(10) + Duration::from_secs(10))]
        );
        // A second press before the engine moves builds on the first target.
        assert_eq!(
            model.command(UserCommand::SeekByMs(SEEK_STEP), at(110)),
            vec![Directive::Seek(minutes(10) + Duration::from_secs(20))]
        );
        model.ingest(playing(minutes(10)), at(150));
        assert!(
            reports(&model.flush(at(200))).is_empty(),
            "presses in a row send one report"
        );
        assert_eq!(reports(&model.flush(at(320))), vec![ReportKind::Progress]);
        assert_eq!(
            model.report_body().unwrap().position,
            minutes(10) + Duration::from_secs(20)
        );
        assert_eq!(
            model.command(UserCommand::SeekByMs(SEEK_STEP), at(400)),
            vec![Directive::Seek(minutes(10) + Duration::from_secs(30))]
        );
        model.flush(at(1_000));
        let near_end = Duration::from_secs(3595);
        model.ingest(playing(near_end), at(5_000));
        assert_eq!(
            model.command(UserCommand::SeekByMs(SEEK_STEP), at(5_000)),
            vec![Directive::Seek(Duration::from_secs(3600))],
            "a forward seek stops at the end"
        );
    }

    #[test]
    fn pause_survives_a_snapshot_that_has_not_caught_up() {
        let (mut model, now) = open_live();
        let at = |ms: u64| now + Duration::from_millis(ms);
        let pause = model.command(UserCommand::TogglePlay, at(10));
        assert_eq!(reports(&pause), vec![ReportKind::Progress]);
        // The engine applies the pause asynchronously.
        let stale = model.ingest(playing(Duration::from_secs(5)), at(260));
        assert!(reports(&stale).is_empty(), "no contradicting report");
        assert_eq!(model.playback_state(), Some(PlaybackState::Paused));
        let caught_up = model.ingest(paused(Duration::from_secs(5)), at(510));
        assert!(reports(&caught_up).is_empty(), "no duplicate report");
        assert_eq!(model.playback_state(), Some(PlaybackState::Paused));

        let play = model.command(UserCommand::TogglePlay, at(1_000));
        assert_eq!(reports(&play), vec![ReportKind::Progress]);
        let stale = model.ingest(paused(Duration::from_secs(5)), at(1_250));
        assert!(reports(&stale).is_empty());
        assert_eq!(model.playback_state(), Some(PlaybackState::Playing));
        model.ingest(playing(Duration::from_secs(5)), at(1_500));
        assert_eq!(model.playback_state(), Some(PlaybackState::Playing));
    }

    #[test]
    fn an_engine_that_never_pauses_wins_after_the_settle_time() {
        let (mut model, now) = open_live();
        model.command(UserCommand::TogglePlay, now);
        model.ingest(playing(Duration::from_secs(5)), now + COMMAND_SETTLE);
        assert_eq!(model.playback_state(), Some(PlaybackState::Playing));
    }

    #[test]
    fn progress_reports_every_ten_seconds_on_the_ui_tick() {
        let (mut model, now) = open_live();
        let mut sent = 0;
        let mut tick = Duration::ZERO;
        while tick <= Duration::from_secs(30) {
            sent += model.progress_tick(now + tick).len();
            tick += Duration::from_millis(250);
        }
        assert_eq!(sent, 3, "10, 20, and 30 seconds after Start");
    }

    #[test]
    fn play_after_the_end_starts_a_new_report_session() {
        let (mut model, now) = open_live();
        let at = |ms: u64| now + Duration::from_millis(ms);
        let end = Duration::from_secs(3600);
        model.ingest(playing(end - Duration::from_secs(1)), at(100));
        let ended = model.ingest_event(PlayerEvent::Ended, at(200));
        assert_eq!(reports(&ended), vec![ReportKind::Stopped]);
        let ended_snapshot = Snapshot {
            state: PlaybackState::Ended,
            ..playing(end)
        };
        model.ingest(ended_snapshot.clone(), at(250));

        let again = model.command(UserCommand::TogglePlay, at(5_000));
        assert!(again.contains(&Directive::Play));
        assert_eq!(reports(&again), vec![ReportKind::Start]);
        assert_eq!(model.report_body().unwrap().position, Duration::ZERO);
        // The engine has not rewound yet. That stale end is not a second stop.
        let stale = model.ingest(ended_snapshot, at(5_250));
        assert!(reports(&stale).is_empty());
        assert_eq!(model.timeline().position, Duration::ZERO);

        model.ingest(playing(Duration::from_millis(300)), at(5_500));
        assert_eq!(model.playback_state(), Some(PlaybackState::Playing));
        assert_eq!(
            reports(&model.progress_tick(at(15_000))),
            vec![ReportKind::Progress]
        );
        let ended = model.ingest_event(PlayerEvent::Ended, at(20_000));
        assert_eq!(reports(&ended), vec![ReportKind::Stopped]);
    }

    #[test]
    fn track_changes_update_the_reported_stream_indices() {
        use matinee_core::{AudioStream, MediaStream, SubtitleStream};

        let audio = |index| {
            MediaStream::Audio(AudioStream {
                index,
                codec: None,
                title: None,
                display_title: None,
                language: None,
                channels: None,
                channel_layout: None,
                sample_rate: None,
                bitrate: None,
                is_default: index == 1,
            })
        };
        let subtitle = |index, external| {
            MediaStream::Subtitle(SubtitleStream {
                index,
                codec: None,
                title: None,
                language: None,
                is_default: false,
                forced: false,
                external,
            })
        };
        let mut prepared = plan(PlaybackMethod::DirectPlay, Duration::ZERO);
        prepared.plan.streams = vec![audio(2), audio(1), subtitle(3, false), subtitle(4, true)];
        let now = Instant::now();
        let mut model = PlayerModel::new();
        model.begin(item());
        model.accept_plan(prepared);
        model.ingest_event(PlayerEvent::Loaded, now);
        let mut snapshot = playing(Duration::from_secs(4));
        snapshot.audio_tracks = vec![
            sample_track(7, TrackKind::Audio, Some("Dialogue"), None),
            sample_track(8, TrackKind::Audio, Some("Commentary"), None),
        ];
        snapshot.subtitle_tracks = vec![sample_track(9, TrackKind::Subtitle, None, None)];
        model.ingest(snapshot.clone(), now);

        model.command(UserCommand::SelectAudio(TrackId::from_raw(8)), now);
        assert_eq!(model.report_body().unwrap().audio_stream_index, Some(2));
        model.command(UserCommand::SubtitlesOff, now);
        assert_eq!(model.report_body().unwrap().subtitle_stream_index, Some(-1));
        model.command(UserCommand::SelectSubtitle(TrackId::from_raw(9)), now);
        assert_eq!(
            model.report_body().unwrap().subtitle_stream_index,
            Some(3),
            "external subtitles are not in the file the engine opened"
        );

        // A transcode that carries one audio track cannot be matched safely.
        snapshot.audio_tracks.truncate(1);
        model.ingest(snapshot, now);
        model.command(UserCommand::SelectAudio(TrackId::from_raw(7)), now);
        assert_eq!(model.report_body().unwrap().audio_stream_index, None);
    }

    fn stops(closing: &Closing) -> usize {
        reports(&closing.directives)
            .into_iter()
            .filter(|kind| *kind == ReportKind::Stopped)
            .count()
    }

    #[test]
    fn closing_from_playback_reports_one_stop_with_the_last_state() {
        use matinee_core::{MediaSourceId, PlaySessionId};

        let mut prepared = plan(PlaybackMethod::Transcode, Duration::ZERO);
        prepared.plan.source_id = Some(MediaSourceId::parse("source-1").unwrap());
        prepared.plan.play_session_id = Some(PlaySessionId::parse("play-1").unwrap());
        let now = Instant::now();
        let mut model = PlayerModel::new();
        model.begin(item());
        model.accept_plan(prepared);
        model.ingest_event(PlayerEvent::Loaded, now);
        model.ingest(playing(minutes(20)), now);
        model.command(UserCommand::TogglePlay, now);
        model.command(UserCommand::SubtitlesOff, now);
        // The engine paused at 20:03 with the volume lowered and muted.
        let last = Snapshot {
            volume: 0.4,
            muted: true,
            ..paused(minutes(20) + Duration::from_secs(3))
        };
        model.note_final_snapshot(&last);

        let closing = model.close();
        assert_eq!(stops(&closing), 1);
        assert!(closing.directives.contains(&Directive::Stop));
        let report = closing.report.expect("a stop is owed");
        assert_eq!(report.position, minutes(20) + Duration::from_secs(3));
        assert!(report.paused);
        assert!(report.muted);
        assert!((report.volume - 0.4).abs() < f32::EPSILON);
        assert_eq!(report.audio_stream_index, Some(1));
        assert_eq!(report.subtitle_stream_index, Some(-1));
        assert_eq!(report.media_source_id.unwrap().as_str(), "source-1");
        assert_eq!(report.play_session_id.unwrap().as_str(), "play-1");
        assert_eq!(report.method, PlaybackMethod::Transcode);
        assert_eq!(model.close(), Closing::default(), "teardown after close");
    }

    #[test]
    fn teardown_never_reports_position_zero_in_place_of_the_last_one() {
        let (mut model, now) = open_live();
        model.ingest(playing(minutes(42)), now);
        // The engine already went idle, so its snapshot says zero.
        model.note_final_snapshot(&Snapshot::default());
        let report = model.close().report.unwrap();
        assert_eq!(report.position, minutes(42));
    }

    #[test]
    fn closing_during_a_seek_reports_the_target() {
        let (mut model, now) = open_live();
        model.command(UserCommand::Scrub(minutes(50)), now);
        model.note_final_snapshot(&playing(Duration::from_secs(4)));
        let report = model.close().report.unwrap();
        assert_eq!(report.position, minutes(50));
    }

    #[test]
    fn teardown_after_natural_completion_sends_no_second_stop() {
        let (mut model, now) = open_live();
        let ended = model.ingest_event(PlayerEvent::Ended, now);
        assert_eq!(reports(&ended), vec![ReportKind::Stopped]);
        let closing = model.close();
        assert_eq!(stops(&closing), 0);
        assert_eq!(closing.report, None);
        assert_eq!(closing.directives, vec![Directive::Stop]);
    }

    #[test]
    fn closing_a_replay_sends_its_own_stop() {
        let (mut model, now) = open_live();
        model.ingest_event(PlayerEvent::Ended, now);
        let again = model.command(UserCommand::TogglePlay, now + Duration::from_secs(1));
        assert_eq!(reports(&again), vec![ReportKind::Start]);
        // Past the settle time, so the rewind target has been released.
        model.ingest(
            playing(Duration::from_secs(30)),
            now + Duration::from_secs(3),
        );
        let closing = model.close();
        assert_eq!(stops(&closing), 1);
        assert_eq!(closing.report.unwrap().position, Duration::from_secs(30));
    }

    #[test]
    fn playback_that_never_started_reports_no_stop() {
        let mut resolving = PlayerModel::new();
        resolving.begin(item());
        assert_eq!(resolving.close().report, None);

        let mut loading = PlayerModel::new();
        loading.begin(item());
        loading.accept_plan(plan(PlaybackMethod::DirectPlay, Duration::ZERO));
        loading.note_final_snapshot(&playing(Duration::from_secs(1)));
        let closing = loading.close();
        assert_eq!(stops(&closing), 0);
        assert_eq!(closing.report, None);

        let mut rejected = PlayerModel::new();
        rejected.begin(item());
        rejected.reject_plan(PlanFailure::incompatible("no stream"));
        assert_eq!(rejected.close().report, None);

        let mut missing = PlayerModel::new();
        missing.begin(item());
        missing.accept_plan(plan(PlaybackMethod::DirectPlay, Duration::ZERO));
        missing.reject_engine(PlayerFailure::library_missing());
        assert_eq!(missing.close().report, None);
    }

    #[test]
    fn natural_completion_reports_stop_once() {
        let (mut model, now) = open_live();
        let ended = model.ingest_event(PlayerEvent::Ended, now);
        assert_eq!(ended, vec![Directive::Report(ReportKind::Stopped)]);
        assert!(model.controls_visible(now + Duration::from_secs(30)));
        let again = model.close();
        assert_eq!(again.directives, vec![Directive::Stop]);
        assert_eq!(again.report, None, "the stop was already reported");
    }

    #[test]
    fn controls_hide_only_during_playback() {
        let (mut model, now) = open_live();
        model.command(UserCommand::Activity, now);
        assert!(model.controls_visible(now));
        assert!(
            !model.controls_visible(now + CONTROLS_IDLE),
            "playback hides the overlay after idle"
        );
        model.command(UserCommand::Activity, now + CONTROLS_IDLE);
        assert!(model.controls_visible(now + CONTROLS_IDLE));
        model.command(UserCommand::TogglePlay, now + CONTROLS_IDLE);
        assert!(model.controls_visible(now + CONTROLS_IDLE + CONTROLS_IDLE));
        model.command(UserCommand::TogglePlay, now);
        model.command(UserCommand::OpenMenu(MenuKind::Audio), now);
        assert!(model.controls_visible(now + Duration::from_secs(60)));
    }

    #[test]
    fn missing_library_is_an_error_state() {
        let mut model = PlayerModel::new();
        model.begin(item());
        model.reject_engine(PlayerFailure::library_missing());
        assert_eq!(model.failure().unwrap().kind, FailureKind::LibraryMissing);
        assert!(model.visible_lines().join("\n").contains("libmpv"));
        assert_eq!(model.playback_state(), Some(PlaybackState::Error));
    }

    #[test]
    fn secrets_stay_out_of_debug_and_visible_text() {
        let mut model = PlayerModel::new();
        model.begin(item());
        model.accept_plan(plan(PlaybackMethod::Transcode, Duration::from_secs(3)));
        model.fail_playback(format!("open https://host/stream?api_key={TOKEN}"));
        let rendered = format!("{model:?}\n{}", model.visible_lines().join("\n"));
        assert!(!rendered.contains(TOKEN));
        assert!(!rendered.contains("MediaBrowser"));
        assert!(!rendered.contains("Authorization"));
    }
}
