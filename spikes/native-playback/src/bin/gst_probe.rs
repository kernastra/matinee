//! Headless GStreamer (playbin3) capability probe mirroring `mpv-probe`.
//! Video goes to a BGRA appsink (the frame path a GPUI integration would
//! use); subtitles go to a text appsink so the selected cue can be checked.
//!
//! gst-probe <path-or-url> [--start SECS] [--frames-secs SECS] [--text-sink] [--playbin2]
//!
//! Without `--text-sink`, playbin3 blends subtitles into the video frames.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSinkCallbacks};
use gstreamer_video::{VideoCapsBuilder, VideoFormat, VideoInfo};
use native_playback_spike::frame::Timing;

#[derive(Default)]
struct Shared {
    frames: u64,
    first_frame: Option<Duration>,
    copy: Timing,
    size: (u32, u32),
    subtitle: String,
    elements: Vec<String>,
}

struct Report {
    failures: usize,
}

impl Report {
    fn check(&mut self, name: &str, ok: bool, detail: impl AsRef<str>) {
        if !ok {
            self.failures += 1;
        }
        println!(
            "{} {name}: {}",
            if ok { "PASS" } else { "FAIL" },
            detail.as_ref()
        );
    }

    fn info(&self, name: &str, detail: impl AsRef<str>) {
        println!("INFO {name}: {}", detail.as_ref());
    }
}

fn checkpoint(shared: &Mutex<Shared>, label: &str) {
    println!(
        "INFO frames-delivered@{label}: {}",
        shared.lock().unwrap().frames
    );
}

fn seconds(time: Option<gst::ClockTime>) -> f64 {
    time.map_or(-1.0, |time| time.nseconds() as f64 / 1e9)
}

fn clock(secs: f64) -> gst::ClockTime {
    gst::ClockTime::from_nseconds((secs * 1e9) as u64)
}

/// Pops bus messages until `matches` yields a value; bus errors fail fast.
fn wait_bus<T>(
    bus: &gst::Bus,
    timeout: Duration,
    mut matches: impl FnMut(&gst::Message) -> Option<T>,
) -> Result<(T, Duration)> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        let Some(message) = bus.timed_pop(gst::ClockTime::from_mseconds(50)) else {
            continue;
        };
        if let gst::MessageView::Error(error) = message.view() {
            bail!("{} ({:?})", error.error(), error.debug());
        }
        if let Some(value) = matches(&message) {
            return Ok((value, started.elapsed()));
        }
    }
    bail!("timed out after {timeout:?}")
}

fn is_async_done(message: &gst::Message) -> Option<()> {
    matches!(message.view(), gst::MessageView::AsyncDone(_)).then_some(())
}

fn language(stream: &gst::Stream) -> String {
    stream
        .tags()
        .and_then(|tags| {
            tags.get::<gst::tags::LanguageCode>()
                .map(|tag| tag.get().to_string())
        })
        .unwrap_or_else(|| "-".into())
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut source, mut start, mut frames_secs) = (String::new(), None::<f64>, 3.0);
    let (mut use_text_sink, mut playbin2) = (false, false);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--start" => start = Some(args.next().context("--start SECS")?.parse()?),
            "--text-sink" => use_text_sink = true,
            "--playbin2" => playbin2 = true,
            "--frames-secs" => frames_secs = args.next().context("--frames-secs SECS")?.parse()?,
            _ => source = arg,
        }
    }
    anyhow::ensure!(
        !source.is_empty(),
        "usage: gst-probe <path-or-url> [--start SECS]"
    );
    let uri = if source.contains("://") {
        source.clone()
    } else {
        gst::glib::filename_to_uri(std::fs::canonicalize(&source)?, None)?.to_string()
    };

    gst::init()?;
    let mut report = Report { failures: 0 };
    report.info("gstreamer", gst::version_string());

    let shared = Arc::new(Mutex::new(Shared::default()));
    let opened = Instant::now();

    let video_sink = AppSink::builder()
        .caps(&VideoCapsBuilder::new().format(VideoFormat::Bgra).build())
        .max_buffers(2)
        .drop(true)
        .sync(true)
        .build();
    let frames = shared.clone();
    video_sink.set_callbacks(
        AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                let info = sample
                    .caps()
                    .and_then(|caps| VideoInfo::from_caps(caps).ok())
                    .ok_or(gst::FlowError::NotNegotiated)?;
                let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                // The copy a GPUI RenderImage needs: GStreamer owns this memory.
                let began = Instant::now();
                let owned = map.as_slice().to_vec();
                let elapsed = began.elapsed();
                let mut shared = frames.lock().unwrap();
                shared.copy.record(elapsed);
                shared.frames += 1;
                shared.size = (info.width(), info.height());
                shared.first_frame.get_or_insert(opened.elapsed());
                drop(owned);
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    let text_sink = AppSink::builder()
        .caps(&gst::Caps::builder("text/x-raw").build())
        .sync(true)
        .build();
    // Subtitles are sparse; a prerolling text sink stalls the pipeline.
    text_sink.set_property("async", false);
    let texts = shared.clone();
    text_sink.set_callbacks(
        AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                if let Some(buffer) = sample.buffer()
                    && let Ok(map) = buffer.map_readable()
                {
                    texts.lock().unwrap().subtitle = String::from_utf8_lossy(map.as_slice()).into();
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    let audio_sink = gst::ElementFactory::make("fakesink")
        .property("sync", true)
        .build()?;
    let playbin = gst::ElementFactory::make(if playbin2 { "playbin" } else { "playbin3" })
        .property("uri", &uri)
        .property("video-sink", &video_sink)
        .property("audio-sink", &audio_sink)
        .build()?;
    if use_text_sink {
        playbin.set_property("text-sink", &text_sink);
    }
    let elements = shared.clone();
    playbin.connect("deep-element-added", false, move |values| {
        if let Ok(element) = values[2].get::<gst::Element>()
            && let Some(factory) = element.factory()
        {
            let name = factory.name().to_string();
            let klass = factory.metadata("klass").unwrap_or_default().to_string();
            if klass.contains("Decoder") || klass.contains("Demux") {
                elements.lock().unwrap().elements.push(name);
            }
        }
        None
    });
    let bus = playbin.bus().context("playbin bus")?;

    playbin.set_state(gst::State::Paused)?;
    let mut collection = None;
    let preroll = wait_bus(&bus, Duration::from_secs(30), |message| {
        match message.view() {
            gst::MessageView::StreamCollection(event) => {
                collection = Some(event.stream_collection());
                None
            }
            gst::MessageView::AsyncDone(_) => Some(()),
            _ => None,
        }
    });
    report.check(
        "preroll",
        preroll.is_ok(),
        match &preroll {
            Ok((_, elapsed)) => format!("PAUSED after {elapsed:?}"),
            Err(error) => format!("{error:#}"),
        },
    );
    preroll?;
    let streams: Vec<gst::Stream> = match (&collection, playbin2) {
        (Some(collection), _) => collection.iter().collect(),
        (None, true) => Vec::new(),
        (None, false) => bail!("no stream collection posted"),
    };
    for stream in &streams {
        report.info(
            "stream",
            format!(
                "{:?} id={} lang={} caps={}",
                stream.stream_type(),
                stream.stream_id().unwrap_or_default(),
                language(stream),
                stream
                    .caps()
                    .and_then(|caps| caps.structure(0).map(|s| s.name().to_string()))
                    .unwrap_or_default()
            ),
        );
    }
    let of_type = |kind: gst::StreamType| -> Vec<&gst::Stream> {
        streams
            .iter()
            .filter(|stream| stream.stream_type().contains(kind))
            .collect()
    };
    let (videos, audios, texts) = (
        of_type(gst::StreamType::VIDEO),
        of_type(gst::StreamType::AUDIO),
        of_type(gst::StreamType::TEXT),
    );

    if let Some(start) = start {
        playbin.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
            clock(start),
        )?;
        wait_bus(&bus, Duration::from_secs(20), is_async_done)?;
    }
    playbin.set_state(gst::State::Playing)?;
    let position = || seconds(playbin.query_position::<gst::ClockTime>());
    let duration = seconds(playbin.query_duration::<gst::ClockTime>());

    let deadline = Instant::now() + Duration::from_secs(20);
    while shared.lock().unwrap().first_frame.is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let first = shared.lock().unwrap().first_frame;
    report.check(
        "first-frame",
        first.is_some(),
        format!("first BGRA sample {first:?} after open"),
    );
    report.info("elements", shared.lock().unwrap().elements.join(", "));
    if let Some(start) = start {
        thread::sleep(Duration::from_millis(300));
        let at = position();
        report.check(
            "resume",
            (at - start).abs() < 2.5,
            format!("start={start}s position={at:.2}s"),
        );
    }

    thread::sleep(Duration::from_millis(1200));
    let before = position();
    thread::sleep(Duration::from_millis(1000));
    let advanced = position() - before;
    report.check(
        "playing",
        advanced > 0.5,
        format!("advanced {advanced:.2}s in 1.0s wall"),
    );

    playbin.set_state(gst::State::Paused)?;
    thread::sleep(Duration::from_millis(300));
    let paused_at = position();
    thread::sleep(Duration::from_millis(700));
    let drift = position() - paused_at;
    report.check(
        "pause",
        drift.abs() < 0.05,
        format!("drift while paused {drift:.3}s"),
    );
    playbin.set_state(gst::State::Playing)?;

    if duration > 20.0 {
        let target = (duration * 0.5).floor();
        let began = Instant::now();
        playbin.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
            clock(target),
        )?;
        wait_bus(&bus, Duration::from_secs(20), is_async_done)?;
        thread::sleep(Duration::from_millis(100));
        let landed = position();
        report.check(
            "seek-absolute",
            (landed - target).abs() < 1.0,
            format!(
                "target={target}s landed={landed:.2}s in {:?}",
                began.elapsed()
            ),
        );
    } else {
        report.info("seek", format!("skipped: duration={duration}"));
    }

    checkpoint(&shared, "after-seek");
    playbin.set_property("volume", 0.35f64);
    playbin.set_property("mute", true);
    let volume: f64 = playbin.property("volume");
    let muted: bool = playbin.property("mute");
    playbin.set_property("mute", false);
    report.check(
        "volume",
        (volume - 0.35).abs() < 1e-6 && muted,
        format!("volume={volume} mute={muted}"),
    );

    let select = |ids: Vec<&str>| -> Result<Vec<String>> {
        let event = gst::event::SelectStreams::builder(&ids).build();
        anyhow::ensure!(playbin.send_event(event), "select-streams rejected");
        let (selected, _) = wait_bus(&bus, Duration::from_secs(10), |message| {
            match message.view() {
                gst::MessageView::StreamsSelected(event) => Some(
                    event
                        .streams()
                        .iter()
                        .filter_map(|stream| stream.stream_id().map(|id| id.to_string()))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            }
        })?;
        Ok(selected)
    };
    checkpoint(&shared, "after-volume");
    if playbin2 {
        let tags = |signal: &str, index: i32| {
            playbin
                .emit_by_name::<Option<gst::TagList>>(signal, &[&index])
                .and_then(|tags| {
                    tags.get::<gst::tags::LanguageCode>()
                        .map(|tag| tag.get().to_string())
                })
                .unwrap_or_else(|| "-".into())
        };
        let n_audio: i32 = playbin.property("n-audio");
        for index in 0..n_audio {
            playbin.set_property("current-audio", index);
            thread::sleep(Duration::from_millis(800));
            let current: i32 = playbin.property("current-audio");
            report.check(
                "audio-track",
                current == index,
                format!(
                    "current-audio={current} lang={}",
                    tags("get-audio-tags", index)
                ),
            );
        }
        checkpoint(&shared, "after-audio-switch");
        let n_text: i32 = playbin.property("n-text");
        for index in 0..n_text {
            playbin.set_property("current-text", index);
            thread::sleep(Duration::from_millis(1500));
            let current: i32 = playbin.property("current-text");
            let cue = shared.lock().unwrap().subtitle.clone();
            report.check(
                "subtitle-track",
                current == index,
                format!(
                    "current-text={current} lang={} text-sink cue={cue:?}",
                    tags("get-text-tags", index)
                ),
            );
        }
    }
    let video_id = videos.first().and_then(|stream| stream.stream_id());
    for audio in audios.iter().filter(|_| !playbin2) {
        let audio_id = audio.stream_id().unwrap_or_default();
        let mut ids: Vec<&str> = video_id.iter().map(|id| id.as_str()).collect();
        ids.push(audio_id.as_str());
        match select(ids) {
            Ok(selected) => report.check(
                "audio-track",
                selected.iter().any(|id| id == audio_id.as_str()),
                format!("selected lang={} -> {selected:?}", language(audio)),
            ),
            Err(error) => report.check("audio-track", false, format!("{error:#}")),
        }
    }
    checkpoint(&shared, "after-audio-switch");
    for text in texts.iter().filter(|_| !playbin2) {
        let text_id = text.stream_id().unwrap_or_default();
        let audio_id = audios
            .first()
            .and_then(|stream| stream.stream_id())
            .unwrap_or_default();
        let mut ids: Vec<&str> = video_id.iter().map(|id| id.as_str()).collect();
        ids.extend([audio_id.as_str(), text_id.as_str()]);
        let selected = select(ids);
        thread::sleep(Duration::from_millis(1500));
        let cue = shared.lock().unwrap().subtitle.clone();
        report.check(
            "subtitle-track",
            selected
                .as_ref()
                .is_ok_and(|selected| selected.iter().any(|id| id == text_id.as_str())),
            format!("lang={} text-sink cue={cue:?}", language(text)),
        );
    }

    checkpoint(&shared, "after-subtitles");
    let frames_before = shared.lock().unwrap().frames;
    thread::sleep(Duration::from_secs_f64(frames_secs));
    let shared = shared.lock().unwrap();
    let fps = (shared.frames - frames_before) as f64 / frames_secs;
    report.check(
        "frames",
        fps > 1.0,
        format!(
            "{}x{} BGRA {fps:.1} fps into appsink",
            shared.size.0, shared.size.1
        ),
    );
    report.info(
        "frame-copy-cost",
        format!("map+to_vec {}", shared.copy.summary()),
    );
    drop(shared);

    playbin.set_state(gst::State::Null)?;
    println!("SUMMARY failures={}", report.failures);
    std::process::exit(if report.failures == 0 { 0 } else { 1 });
}
