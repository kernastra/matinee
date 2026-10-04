//! Authoritative Player state.
//!
//! The screen applies [`Directive`]s. It does not decide when to seek, what
//! to report, or which source to invent. Snapshot state remains the playback
//! state once a file is open.

use std::fmt;
use std::time::{Duration, Instant};

use matinee_core::{
    ItemId, MediaSourceId, PlaySessionId, PlaybackMethod, PlaybackPlan, PlaybackReport, ReportKind,
    StreamAuthorization,
};
use matinee_player::{PlaybackState, PlayerEvent, Snapshot, Track, TrackId, TrackKind};

use super::prepare::{redact_message, resume_start, strip_credential_query};
use super::{COMPLETED_TAIL, CONTROLS_IDLE, PROGRESS_INTERVAL, SEEK_THROTTLE};

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
    scrub: Option<Duration>,
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
        let url = strip_credential_query(&prepared.plan.url);
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
            UserCommand::SeekByMs(delta) => {
                vec![Directive::SeekByMs(delta), self.mark_progress(now)]
            }
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
        if let Err(message) = result {
            self.report_notice = Some(redact_message(&message));
        }
    }

    /// Stop reporting and release the engine. A second call is empty.
    pub(crate) fn close(&mut self) -> Vec<Directive> {
        if self.stage == Stage::Closed {
            return Vec::new();
        }
        let mut directives = self.stop_report();
        directives.push(Directive::Stop);
        self.stage = Stage::Closed;
        self.menu = None;
        self.scrub = None;
        directives
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
                self.reports.pause_latched = true;
                vec![Directive::Pause, self.mark_progress(now)]
            }
            PlaybackState::Paused | PlaybackState::Ended => {
                let resume = self.reports.pause_latched;
                self.snapshot.state = PlaybackState::Playing;
                self.reports.pause_latched = false;
                let mut directives = vec![Directive::Play];
                if resume {
                    directives.push(self.mark_progress(now));
                }
                directives
            }
            _ => Vec::new(),
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

    fn item() -> ItemId {
        ItemId::parse("item-1").unwrap()
    }

    fn plan(method: PlaybackMethod, start: Duration) -> PreparedPlayback {
        PreparedPlayback {
            title: "Northwind".into(),
            context: None,
            runtime: Some(Duration::from_secs(3600)),
            plan: PlaybackPlan {
                url: format!(
                    "https://jellyfin.local/Videos/item-1/stream?Static=true&api_key={TOKEN}"
                ),
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
        assert!(model.close().is_empty());
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
            model.command(UserCommand::SeekByMs(-10_000), now)[0],
            Directive::SeekByMs(-10_000)
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
        let closing = model.close();
        assert!(closing.contains(&Directive::Report(ReportKind::Stopped)));
        assert!(closing.contains(&Directive::Stop));
        assert!(
            model
                .progress_tick(now + Duration::from_secs(100))
                .is_empty()
        );
    }

    #[test]
    fn natural_completion_reports_stop_once() {
        let (mut model, now) = open_live();
        let ended = model.ingest_event(PlayerEvent::Ended, now);
        assert_eq!(ended, vec![Directive::Report(ReportKind::Stopped)]);
        assert!(model.controls_visible(now + Duration::from_secs(30)));
        let again = model.close();
        assert_eq!(again, vec![Directive::Stop]);
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
