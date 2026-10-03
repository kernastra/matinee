//! Headless libmpv capability probe: load a file or URL and exercise the
//! controls Matinee needs, printing PASS/FAIL lines plus timings.
//!
//! mpv-probe <path-or-url> [--start SECS] [--hwdec MODE] [--ao DRIVER] [--sw-render SECS]
//!           [--opt NAME=VALUE]...

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use native_playback_spike::frame::{Timing, fit_within, force_opaque_bgra};
use native_playback_spike::mpv::{self, Event, Mpv, SwRenderContext};

struct Args {
    source: String,
    start: Option<f64>,
    hwdec: String,
    ao: String,
    sw_render_secs: Option<f64>,
    extra: Vec<(String, String)>,
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Args {
        source: String::new(),
        start: None,
        hwdec: "auto-safe".into(),
        ao: "null".into(),
        sw_render_secs: None,
        extra: Vec::new(),
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--start" => parsed.start = Some(args.next().context("--start SECS")?.parse()?),
            "--hwdec" => parsed.hwdec = args.next().context("--hwdec MODE")?,
            "--ao" => parsed.ao = args.next().context("--ao DRIVER")?,
            "--sw-render" => {
                parsed.sw_render_secs = Some(args.next().context("--sw-render SECS")?.parse()?)
            }
            "--opt" => {
                let pair = args.next().context("--opt NAME=VALUE")?;
                let (name, value) = pair.split_once('=').context("--opt NAME=VALUE")?;
                parsed.extra.push((name.into(), value.into()));
            }
            _ => parsed.source = arg,
        }
    }
    anyhow::ensure!(
        !parsed.source.is_empty(),
        "usage: mpv-probe <path-or-url> [options]"
    );
    Ok(parsed)
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

fn ms(duration: Duration) -> String {
    format!("{:.0}ms", duration.as_secs_f64() * 1000.0)
}

fn is_restart(event: &Event) -> bool {
    *event == Event::PlaybackRestart
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let mut report = Report { failures: 0 };
    let (major, minor) = mpv::client_api_version();
    report.info("libmpv", format!("client API {major}.{minor}"));

    let start = args.start.map(|secs| format!("{secs}"));
    let mut options = vec![
        ("terminal", "no"),
        ("ytdl", "no"),
        ("ao", args.ao.as_str()),
        ("hwdec", args.hwdec.as_str()),
        (
            "vo",
            if args.sw_render_secs.is_some() {
                "libmpv"
            } else {
                "null"
            },
        ),
        // Jellyfin-friendly network defaults.
        ("cache", "yes"),
        ("demuxer-max-bytes", "150MiB"),
        ("network-timeout", "15"),
    ];
    if let Some(start) = &start {
        options.push(("start", start.as_str()));
    }
    options.extend(
        args.extra
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    );
    let player = Mpv::new(&options)?;

    let mut renderer = match args.sw_render_secs {
        Some(_) => Some(SwRenderContext::new(player.clone())?),
        None => None,
    };

    let opened = Instant::now();
    player.command(&["loadfile", &args.source])?;
    let loaded = player.wait_for(Duration::from_secs(30), |event| *event == Event::FileLoaded);
    report.check(
        "load",
        loaded.is_ok(),
        match &loaded {
            Ok(elapsed) => format!("FILE_LOADED after {}", ms(*elapsed)),
            Err(error) => format!("{error:#}"),
        },
    );
    loaded?;
    let first = player.wait_for(Duration::from_secs(30), is_restart)?;
    report.check(
        "first-frame",
        true,
        format!("PLAYBACK_RESTART {} after loadfile", ms(opened.elapsed())),
    );
    let _ = first;

    let prop = |name: &str| player.get_string(name).unwrap_or_else(|_| "-".into());
    report.info(
        "media",
        format!(
            "format={} video={} {}x{} {} audio={} {}ch duration={}s",
            prop("file-format"),
            prop("video-codec"),
            prop("video-params/w"),
            prop("video-params/h"),
            prop("video-params/pixelformat"),
            prop("audio-codec-name"),
            prop("audio-params/channel-count"),
            prop("duration"),
        ),
    );
    report.info(
        "hwdec",
        format!("requested={} current={}", args.hwdec, prop("hwdec-current")),
    );

    let tracks = mpv::tracks(&player)?;
    for track in &tracks {
        report.info(
            "track",
            format!(
                "{} id={} lang={} codec={} title={} selected={}",
                track.kind,
                track.id,
                track.lang.as_deref().unwrap_or("-"),
                track.codec.as_deref().unwrap_or("-"),
                track.title.as_deref().unwrap_or("-"),
                track.selected
            ),
        );
    }

    let duration = player.get_f64("duration").unwrap_or(0.0);
    let position = || player.get_f64("time-pos").unwrap_or(-1.0);

    if let Some(start) = args.start {
        let at = position();
        report.check(
            "resume",
            (at - start).abs() < 2.5,
            format!("start={start}s time-pos={at:.2}s"),
        );
    }

    thread::sleep(Duration::from_millis(1500));
    let before = position();
    thread::sleep(Duration::from_millis(1000));
    let advanced = position() - before;
    report.check(
        "playing",
        advanced > 0.5,
        format!("advanced {advanced:.2}s in 1.0s wall"),
    );

    player.set("pause", "yes")?;
    thread::sleep(Duration::from_millis(300));
    let paused_at = position();
    thread::sleep(Duration::from_millis(700));
    let drift = position() - paused_at;
    report.check(
        "pause",
        player.get_flag("pause")? && drift.abs() < 0.05,
        format!("drift while paused {drift:.3}s"),
    );
    player.set("pause", "no")?;
    report.check("unpause", !player.get_flag("pause")?, "pause=no");

    if duration > 20.0 {
        let target = (duration * 0.5).floor();
        let began = Instant::now();
        player.command(&["seek", &format!("{target}"), "absolute+exact"])?;
        player.wait_for(Duration::from_secs(20), is_restart)?;
        let landed = position();
        report.check(
            "seek-absolute",
            (landed - target).abs() < 1.0,
            format!(
                "target={target}s landed={landed:.2}s in {}",
                ms(began.elapsed())
            ),
        );
        let began = Instant::now();
        player.command(&["seek", "-10", "relative+exact"])?;
        player.wait_for(Duration::from_secs(20), is_restart)?;
        let landed = position();
        report.check(
            "seek-relative",
            (landed - (target - 10.0)).abs() < 1.0,
            format!("-10s exact landed={landed:.2}s in {}", ms(began.elapsed())),
        );
    } else {
        report.info("seek", "skipped: unknown or short duration (live?)");
    }

    player.set("volume", "35")?;
    let volume = player.get_f64("volume")?;
    player.set("mute", "yes")?;
    let muted = player.get_flag("mute")?;
    player.set("mute", "no")?;
    report.check(
        "volume",
        (volume - 35.0).abs() < 0.01 && muted,
        format!("volume={volume} mute toggled"),
    );

    for track in tracks.iter().filter(|track| track.kind == "audio") {
        player.set("aid", &track.id.to_string())?;
        let _ = player.wait_for(Duration::from_secs(5), |event| {
            *event == Event::AudioReconfig
        });
        let current = prop("current-tracks/audio/id");
        report.check(
            "audio-track",
            current == track.id.to_string(),
            format!(
                "aid={} -> current id={} lang={} codec={} ch={}",
                track.id,
                current,
                prop("current-tracks/audio/lang"),
                prop("current-tracks/audio/codec"),
                prop("audio-params/channel-count"),
            ),
        );
    }

    for track in tracks.iter().filter(|track| track.kind == "sub") {
        player.set("sid", &track.id.to_string())?;
        thread::sleep(Duration::from_millis(1200));
        let current = prop("current-tracks/sub/id");
        report.check(
            "subtitle-track",
            current == track.id.to_string(),
            format!(
                "sid={} lang={} sub-text={:?}",
                track.id,
                prop("current-tracks/sub/lang"),
                prop("sub-text")
            ),
        );
    }
    if tracks.iter().any(|track| track.kind == "sub") {
        player.set("sid", "no")?;
        report.check(
            "subtitle-off",
            prop("current-tracks/sub/id") == "-",
            "sid=no",
        );
    }

    if let (Some(renderer), Some(secs)) = (renderer.as_mut(), args.sw_render_secs) {
        let (tx, rx) = mpsc::channel::<()>();
        renderer.set_update_callback(move || {
            let _ = tx.send(());
        });
        let width = player.get_i64("dwidth")? as u32;
        let height = player.get_i64("dheight")? as u32;
        let (width, height) = fit_within((width, height), (3840, 2160));
        let mut buffer = vec![0u8; width as usize * height as usize * 4];
        let (mut render, mut opaque, mut alloc) =
            (Timing::default(), Timing::default(), Timing::default());
        let until = Instant::now() + Duration::from_secs_f64(secs);
        while Instant::now() < until {
            if rx.recv_timeout(Duration::from_millis(100)).is_err() || !renderer.needs_frame() {
                continue;
            }
            let began = Instant::now();
            let mut fresh = vec![0u8; buffer.len()];
            alloc.record(began.elapsed());
            std::mem::swap(&mut buffer, &mut fresh);
            let began = Instant::now();
            renderer.render_bgr0(width, height, &mut buffer, false)?;
            render.record(began.elapsed());
            let began = Instant::now();
            force_opaque_bgra(&mut buffer);
            opaque.record(began.elapsed());
        }
        let fps = render.count() as f64 / secs;
        report.check(
            "sw-render",
            render.count() > 0,
            format!("{width}x{height} {fps:.1} fps rendered"),
        );
        report.info("sw-render-cost", format!("render {}", render.summary()));
        report.info(
            "sw-alpha-cost",
            format!("force-opaque {}", opaque.summary()),
        );
        report.info("sw-alloc-cost", format!("fresh buffer {}", alloc.summary()));
    }

    report.info(
        "drops",
        format!(
            "frame-drop-count={} decoder-frame-drop-count={}",
            prop("frame-drop-count"),
            prop("decoder-frame-drop-count")
        ),
    );
    drop(renderer);
    player.command(&["stop"])?;
    println!("SUMMARY failures={}", report.failures);
    std::process::exit(if report.failures == 0 { 0 } else { 1 });
}
