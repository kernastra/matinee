//! One owner thread for the libmpv handle.
//!
//! Commands arrive on a channel. The sender calls [`ffi::Wakeup::poke`], which
//! is `mpv_wakeup`, so the owner can block in `mpv_wait_event` without a timer.
//! The render thread never touches the client handle. Position is an atomic so
//! the snapshot lock is not rewritten on every tick.

mod ffi;
mod library;
mod render;

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::error::PlayerError;
use crate::frame::{CpuMailbox, Diagnostics};
use crate::state::{PlayerEvent, Session, Snapshot};
use crate::tracks::{RawTrack, TrackId, TrackKind, subtitle_form};

pub use render::measure_software_render;

pub struct EngineInfo {
    pub path: String,
    pub version: (u32, u32),
}

pub fn probe_engine() -> Result<EngineInfo, PlayerError> {
    let loaded = library::load()?;
    Ok(EngineInfo {
        path: loaded.path.display().to_string(),
        version: loaded.version,
    })
}

use ffi::{EngineEvent, Mpv, Wakeup};
use render::{FrameStats, RenderThread, pairs, playback_options};

pub struct Engine {
    commands: Sender<Request>,
    wakeup: Wakeup,
    shared: Arc<Shared>,
    frames: Arc<CpuMailbox>,
    stats: Arc<FrameStats>,
    join: Option<JoinHandle<()>>,
}

pub struct Shared {
    snapshot: RwLock<Snapshot>,
    position_ms: std::sync::atomic::AtomicU64,
    events: Mutex<VecDeque<PlayerEvent>>,
}

impl Shared {
    fn new() -> Self {
        Self {
            snapshot: RwLock::new(Snapshot::default()),
            position_ms: std::sync::atomic::AtomicU64::new(0),
            events: Mutex::new(VecDeque::new()),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self
            .snapshot
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        snapshot.position = Duration::from_millis(self.position_ms.load(Ordering::Relaxed));
        snapshot
    }

    pub fn poll_event(&self) -> Option<PlayerEvent> {
        self.events
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .pop_front()
    }

    fn publish(&self, session: &Session) {
        let snapshot = session.snapshot(Duration::from_millis(
            self.position_ms.load(Ordering::Relaxed),
        ));
        *self
            .snapshot
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = snapshot;
    }

    fn push_events(&self, events: Vec<PlayerEvent>) {
        if events.is_empty() {
            return;
        }
        let mut queue = self
            .events
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        for event in events {
            if queue.len() == 32 {
                queue.pop_front();
            }
            queue.push_back(event);
        }
    }

    fn set_position_ms(&self, millis: u64) {
        self.position_ms.store(millis, Ordering::Relaxed);
    }
}

/// A command the owner applies. Validation happens before it is sent.
#[derive(Clone, Debug)]
pub enum Request {
    Load(LoadRequest),
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
    AddSubtitle(String),
    Shutdown,
}

/// What to open, including an optional resume position.
#[derive(Clone, Debug)]
pub struct LoadRequest {
    pub url: String,
    pub start: Option<Duration>,
    pub headers: Vec<(String, String)>,
}

pub struct OpenOptions {
    pub audio: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self { audio: true }
    }
}

impl Engine {
    pub fn open(options: OpenOptions) -> Result<Self, PlayerError> {
        let (commands, incoming) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let shared = Arc::new(Shared::new());
        let frames = Arc::new(CpuMailbox::new());
        let stats = Arc::new(FrameStats::new());
        let shared_thread = shared.clone();
        let frames_thread = frames.clone();
        let stats_thread = stats.clone();
        let join = thread::Builder::new()
            .name("matinee-player".into())
            .spawn(move || {
                let result = serve(
                    options.audio,
                    incoming,
                    ready_tx,
                    shared_thread.clone(),
                    frames_thread,
                    stats_thread,
                );
                if let Err(error) = result {
                    let mut session = Session::default();
                    session.error = Some(error.to_string());
                    shared_thread.publish(&session);
                    shared_thread.push_events(vec![PlayerEvent::Error(error.to_string())]);
                }
            })
            .map_err(|error| PlayerError::Initialization(error.to_string()))?;
        let wakeup = match ready_rx.recv() {
            Ok(Ok(wakeup)) => wakeup,
            Ok(Err(error)) => {
                let _ = join.join();
                return Err(error);
            }
            Err(_) => {
                let _ = join.join();
                return Err(PlayerError::Initialization(
                    "playback owner exited before it was ready".into(),
                ));
            }
        };
        Ok(Self {
            commands,
            wakeup,
            shared,
            frames,
            stats,
            join: Some(join),
        })
    }

    pub fn send(&self, request: Request) -> Result<(), PlayerError> {
        self.commands
            .send(request)
            .map_err(|_| PlayerError::EngineStopped)?;
        self.wakeup.poke();
        Ok(())
    }

    pub fn shared(&self) -> Arc<Shared> {
        self.shared.clone()
    }

    pub fn frames(&self) -> Arc<CpuMailbox> {
        self.frames.clone()
    }

    pub fn diagnostics(&self) -> Diagnostics {
        let count = self.stats.render_count.load(Ordering::Relaxed);
        let divisor = count.max(1);
        Diagnostics {
            produced: self.frames.produced(),
            replaced: self.frames.replaced(),
            mailbox_depth: self.frames.depth(),
            frame_width: self.stats.width.load(Ordering::Relaxed),
            frame_height: self.stats.height.load(Ordering::Relaxed),
            render_mean: Duration::from_nanos(
                self.stats.render_ns.load(Ordering::Relaxed) / divisor,
            ),
            opaque_mean: Duration::from_nanos(
                self.stats.opaque_ns.load(Ordering::Relaxed) / divisor,
            ),
        }
    }

    pub fn shutdown(&mut self) {
        let _ = self.commands.send(Request::Shutdown);
        self.wakeup.poke();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn serve(
    audio: bool,
    incoming: Receiver<Request>,
    ready: Sender<Result<Wakeup, PlayerError>>,
    shared: Arc<Shared>,
    frames: Arc<CpuMailbox>,
    stats: Arc<FrameStats>,
) -> Result<(), PlayerError> {
    let loaded = match library::load() {
        Ok(loaded) => loaded,
        Err(error) => {
            let _ = ready.send(Err(error.clone()));
            return Err(error);
        }
    };
    let options = playback_options(audio);
    let mut mpv = match Mpv::create(loaded.api, &pairs(&options)) {
        Ok(mpv) => mpv,
        Err(error) => {
            let _ = ready.send(Err(error.clone()));
            return Err(error);
        }
    };
    let render = match RenderThread::start(loaded.api, mpv.handle(), frames, stats) {
        Ok(render) => render,
        Err(error) => {
            let _ = ready.send(Err(error.clone()));
            return Err(error);
        }
    };
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    mpv.set_wakeup_callback(move || {
        let _ = wake_tx.send(());
    });
    mpv.arm_wakeup();
    if let Err(error) = observe(&mpv) {
        let _ = ready.send(Err(error.clone()));
        return Err(error);
    }
    let wakeup = mpv.wakeup_handle();
    if ready.send(Ok(wakeup)).is_err() {
        return Ok(());
    }
    let mut session = Session::default();
    shared.publish(&session);
    loop {
        if wake_rx.recv().is_err() {
            break;
        }
        let mut stop = false;
        while let Ok(request) = incoming.try_recv() {
            if matches!(request, Request::Shutdown) {
                stop = true;
                break;
            }
            apply(&mpv, &mut session, &shared, request);
        }
        loop {
            match mpv.wait_event(0.0) {
                EngineEvent::None => break,
                EngineEvent::Shutdown => {
                    stop = true;
                    break;
                }
                event => apply_event(&mpv, &render, &mut session, &shared, event),
            }
        }
        if stop {
            break;
        }
    }
    mpv.silence_wakeup();
    render.shutdown();
    drop(mpv);
    drop(loaded);
    Ok(())
}

fn observe(mpv: &Mpv) -> Result<(), PlayerError> {
    for (index, name) in [
        "pause",
        "paused-for-cache",
        "eof-reached",
        "idle-active",
        "duration",
        "time-pos",
        "volume",
        "mute",
        "dwidth",
        "dheight",
    ]
    .into_iter()
    .enumerate()
    {
        mpv.observe(name, index as u64)?;
    }
    Ok(())
}

fn apply(mpv: &Mpv, session: &mut Session, shared: &Shared, request: Request) {
    let result = match request {
        Request::Load(load) => load_source(mpv, session, shared, load),
        Request::Play => play(mpv, session),
        Request::Pause => {
            session.paused = true;
            mpv.set("pause", "yes")
        }
        Request::Stop => {
            let result = mpv.command(&["stop"]);
            session.note_stopped();
            result
        }
        Request::Seek(position) => mpv.command(&[
            "seek",
            &format!("{:.3}", position.as_secs_f64()),
            "absolute+exact",
        ]),
        Request::SeekByMs(delta) => mpv.command(&[
            "seek",
            &format!("{:.3}", delta as f64 / 1000.0),
            "relative+exact",
        ]),
        Request::Volume(volume) => {
            session.volume = volume;
            mpv.set("volume", &format!("{:.2}", volume * 100.0))
        }
        Request::Mute(muted) => {
            session.muted = muted;
            mpv.set("mute", if muted { "yes" } else { "no" })
        }
        Request::Audio(id) => set_track(mpv, session, id, "aid"),
        Request::Subtitle(id) => set_track(mpv, session, id, "sid"),
        Request::SubtitlesOff => mpv.set("sid", "no"),
        Request::AddSubtitle(source) => {
            let result = mpv.command(&["sub-add", &source, "select"]);
            if result.is_ok() {
                refresh_tracks(mpv, session, shared);
            }
            result
        }
        Request::Shutdown => Ok(()),
    };
    if let Err(error) = result {
        session.error = Some(error.to_string());
        shared.push_events(vec![PlayerEvent::Error(error.to_string())]);
    }
    shared.publish(session);
}

fn load_source(
    mpv: &Mpv,
    session: &mut Session,
    shared: &Shared,
    load: LoadRequest,
) -> Result<(), PlayerError> {
    let events = session.begin_load();
    shared.push_events(events);
    shared.set_position_ms(0);
    let _ = mpv.command(&["change-list", "http-header-fields", "clr"]);
    for (name, value) in &load.headers {
        mpv.command(&[
            "change-list",
            "http-header-fields",
            "append",
            &format!("{name}: {value}"),
        ])?;
    }
    match load.start {
        Some(start) => mpv.set("start", &format!("{:.3}", start.as_secs_f64()))?,
        None => mpv.set("start", "0")?,
    }
    mpv.command(&["loadfile", &load.url, "replace"])
}

fn play(mpv: &Mpv, session: &mut Session) -> Result<(), PlayerError> {
    if session.eof {
        mpv.command(&["seek", "0", "absolute+exact"])?;
        session.resume_from_end();
    }
    session.paused = false;
    session.idle = false;
    mpv.set("pause", "no")
}

fn set_track(mpv: &Mpv, session: &Session, id: TrackId, property: &str) -> Result<(), PlayerError> {
    let Some(engine_id) = session.tracks.engine_id(id) else {
        return Err(PlayerError::InvalidCommand(format!("unknown track {id}")));
    };
    mpv.set(property, &engine_id.to_string())
}

fn apply_event(
    mpv: &Mpv,
    render: &RenderThread,
    session: &mut Session,
    shared: &Shared,
    event: EngineEvent,
) {
    match event {
        EngineEvent::FileLoaded => {
            let events = session.note_file_loaded();
            shared.push_events(events);
            refresh_tracks(mpv, session, shared);
            refresh_size(mpv, render, session, shared);
        }
        EngineEvent::PlaybackRestart => {
            session.note_restart();
            refresh_size(mpv, render, session, shared);
        }
        EngineEvent::EndFile { reason, error } => {
            let events = session.note_end(reason, error);
            shared.push_events(events);
        }
        EngineEvent::VideoReconfig | EngineEvent::AudioReconfig => {
            refresh_tracks(mpv, session, shared);
            refresh_size(mpv, render, session, shared);
        }
        EngineEvent::Property(name) => apply_property(mpv, render, session, shared, &name),
        EngineEvent::Shutdown => session.note_stopped(),
        _ => {}
    }
    shared.publish(session);
}

fn apply_property(
    mpv: &Mpv,
    render: &RenderThread,
    session: &mut Session,
    shared: &Shared,
    name: &str,
) {
    match name {
        "pause" => session.paused = mpv.get_flag("pause").unwrap_or(session.paused),
        "paused-for-cache" => session.caching = mpv.get_flag("paused-for-cache").unwrap_or(false),
        "eof-reached" => session.eof = mpv.get_flag("eof-reached").unwrap_or(session.eof),
        "idle-active" => session.idle = mpv.get_flag("idle-active").unwrap_or(session.idle),
        "duration" => session.duration = mpv.get_f64("duration").ok().and_then(duration_from),
        "time-pos" => {
            if let Ok(seconds) = mpv.get_f64("time-pos")
                && seconds.is_finite()
                && seconds >= 0.0
            {
                shared.set_position_ms((seconds * 1000.0) as u64);
            }
        }
        "volume" => {
            if let Ok(volume) = mpv.get_f64("volume") {
                session.volume = (volume / 100.0).clamp(0.0, 1.0) as f32;
            }
        }
        "mute" => session.muted = mpv.get_flag("mute").unwrap_or(session.muted),
        "dwidth" | "dheight" => refresh_size(mpv, render, session, shared),
        _ => {}
    }
}

fn refresh_size(mpv: &Mpv, render: &RenderThread, session: &mut Session, shared: &Shared) {
    let width = mpv
        .get_i64("video-params/w")
        .ok()
        .or_else(|| mpv.get_i64("dwidth").ok())
        .unwrap_or(0);
    let height = mpv
        .get_i64("video-params/h")
        .ok()
        .or_else(|| mpv.get_i64("dheight").ok())
        .unwrap_or(0);
    if width <= 0 || height <= 0 {
        return;
    }
    let width = width as u32;
    let height = height as u32;
    let events = session.note_source_size(width, height);
    render.set_size(width, height);
    session.frame_size = Some(crate::frame::capped_render_size(width, height));
    shared.push_events(events);
}

fn refresh_tracks(mpv: &Mpv, session: &mut Session, shared: &Shared) {
    let Ok(count) = mpv.get_i64("track-list/count") else {
        return;
    };
    let mut raw = Vec::new();
    for index in 0..count {
        let kind = mpv
            .get_string(&format!("track-list/{index}/type"))
            .unwrap_or_default();
        let kind = match kind.as_str() {
            "audio" => TrackKind::Audio,
            "sub" => TrackKind::Subtitle,
            _ => continue,
        };
        let Ok(engine_id) = mpv.get_i64(&format!("track-list/{index}/id")) else {
            continue;
        };
        let codec = optional_string(mpv, &format!("track-list/{index}/codec"));
        raw.push(RawTrack {
            engine_id,
            kind,
            form: (kind == TrackKind::Subtitle).then(|| subtitle_form(codec.as_deref())),
            language: optional_string(mpv, &format!("track-list/{index}/lang")),
            title: optional_string(mpv, &format!("track-list/{index}/title")),
            codec,
            selected: mpv
                .get_flag(&format!("track-list/{index}/selected"))
                .unwrap_or(false),
        });
    }
    if session.tracks.sync(&raw) {
        shared.push_events(vec![PlayerEvent::TracksChanged]);
    }
}

fn optional_string(mpv: &Mpv, name: &str) -> Option<String> {
    mpv.get_string(name).ok().filter(|value| !value.is_empty())
}

fn duration_from(seconds: f64) -> Option<Duration> {
    if seconds.is_finite() && seconds >= 0.0 {
        Some(Duration::from_secs_f64(seconds))
    } else {
        None
    }
}
