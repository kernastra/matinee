//! GPUI + libmpv proof. mpv decodes and renders (software render API) into
//! BGRA buffers on a dedicated thread; each frame becomes a GPUI
//! `RenderImage` painted with `Window::paint_image`, and ordinary GPUI
//! elements (HUD, progress bar) are composited on top of the video.
//!
//! gpui-player <path-or-url> [--start SECS] [--hwdec MODE] [--ao DRIVER]
//!             [--max-size WxH] [--demo] [--demo-quit]
//!
//! Keys: space play/pause, left/right seek 10s, up/down volume, a audio track,
//! s subtitle track, m mute, f fullscreen, q quit.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{
    App, Application, Bounds, Context, Corners, FocusHandle, ImageId, KeyDownEvent, Pixels, Render,
    RenderImage, SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions, canvas, div,
    point, prelude::*, px, relative, rgb, rgba, size,
};
use image::{Frame, RgbaImage};
use native_playback_spike::frame::{Timing, fit_within, force_opaque_bgra};
use native_playback_spike::mpv::{Event, Mpv, SwRenderContext, Track, tracks};

struct Options {
    source: String,
    start: Option<String>,
    hwdec: String,
    ao: Option<String>,
    max_size: (u32, u32),
    demo: bool,
    demo_quit: bool,
}

fn parse_options() -> Result<Options> {
    let mut args = std::env::args().skip(1);
    let mut options = Options {
        source: String::new(),
        start: None,
        hwdec: "auto-copy-safe".into(),
        ao: None,
        max_size: (1920, 1080),
        demo: false,
        demo_quit: false,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--start" => options.start = Some(args.next().context("--start SECS")?),
            "--hwdec" => options.hwdec = args.next().context("--hwdec MODE")?,
            "--ao" => options.ao = Some(args.next().context("--ao DRIVER")?),
            "--max-size" => {
                let value = args.next().context("--max-size WxH")?;
                let (w, h) = value.split_once('x').context("--max-size WxH")?;
                options.max_size = (w.parse()?, h.parse()?);
            }
            "--demo" => options.demo = true,
            "--demo-quit" => options.demo_quit = true,
            _ => options.source = arg,
        }
    }
    anyhow::ensure!(
        !options.source.is_empty(),
        "usage: gpui-player <path-or-url> [options]"
    );
    Ok(options)
}

#[derive(Default)]
struct RenderStats {
    mpv_render: Timing,
    force_opaque: Timing,
    allocate_and_wrap: Timing,
    produced: u64,
    replaced_unshown: u64,
    size: (u32, u32),
}

/// Latest-frame mailbox: if the UI falls behind, older frames are discarded
/// before they ever reach GPUI's atlas, so memory and GPU uploads stay bounded.
type Mailbox = Arc<Mutex<Option<Arc<RenderImage>>>>;

fn spawn_render_thread(
    mpv: Arc<Mpv>,
    max_size: (u32, u32),
    mailbox: Mailbox,
    wake_ui: UnboundedSender<()>,
    stats: Arc<Mutex<RenderStats>>,
    stop: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>> {
    let mut renderer = SwRenderContext::new(mpv.clone())?;
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    renderer.set_update_callback(move || {
        let _ = wake_tx.send(());
    });
    Ok(thread::Builder::new()
        .name("mpv-render".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if wake_rx.recv_timeout(Duration::from_millis(100)).is_err()
                    || !renderer.needs_frame()
                {
                    continue;
                }
                let (Ok(width), Ok(height)) = (mpv.get_i64("dwidth"), mpv.get_i64("dheight"))
                else {
                    continue;
                };
                let (width, height) = fit_within((width as u32, height as u32), max_size);

                let began = Instant::now();
                let mut buffer = vec![0u8; width as usize * height as usize * 4];
                let allocated = began.elapsed();
                let began = Instant::now();
                if renderer
                    .render_bgr0(width, height, &mut buffer, false)
                    .is_err()
                {
                    continue;
                }
                let rendered = began.elapsed();
                let began = Instant::now();
                force_opaque_bgra(&mut buffer);
                let opaque = began.elapsed();
                let began = Instant::now();
                let Some(image) = RgbaImage::from_raw(width, height, buffer) else {
                    continue;
                };
                let frame = Arc::new(RenderImage::new(vec![Frame::new(image)]));
                let wrapped = allocated + began.elapsed();

                let replaced = mailbox.lock().unwrap().replace(frame).is_some();
                let mut stats = stats.lock().unwrap();
                stats.mpv_render.record(rendered);
                stats.force_opaque.record(opaque);
                stats.allocate_and_wrap.record(wrapped);
                stats.produced += 1;
                stats.replaced_unshown += u64::from(replaced);
                stats.size = (width, height);
                drop(stats);
                if wake_ui.unbounded_send(()).is_err() {
                    break;
                }
            }
        })?)
}

/// Drains mpv's event queue (it overflows otherwise) and records load errors.
fn spawn_event_thread(
    mpv: Arc<Mpv>,
    last_error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            if let Event::EndFile { error, reason } = mpv.wait_event(0.25)
                && error < 0
            {
                *last_error.lock().unwrap() =
                    Some(format!("end-file reason={reason} error={error}"));
            }
        }
    });
}

#[derive(Default)]
struct Hud {
    position: f64,
    duration: f64,
    paused: bool,
    muted: bool,
    volume: f64,
    audio: String,
    subtitle: String,
    hwdec: String,
    codec: String,
    displayed_fps: f64,
    produced_fps: f64,
}

struct Player {
    mpv: Arc<Mpv>,
    mailbox: Mailbox,
    current: Option<Arc<RenderImage>>,
    retired: Option<Arc<RenderImage>>,
    stats: Arc<Mutex<RenderStats>>,
    upload: Arc<Mutex<(Option<ImageId>, Timing)>>,
    last_error: Arc<Mutex<Option<String>>>,
    displayed: u64,
    fps_sample: (Instant, u64, u64),
    hud: Hud,
    action: SharedString,
    title: SharedString,
    focus: FocusHandle,
    stop: Arc<AtomicBool>,
}

impl Player {
    #[allow(clippy::too_many_arguments)]
    fn new(
        mpv: Arc<Mpv>,
        mailbox: Mailbox,
        mut frames_ready: futures::channel::mpsc::UnboundedReceiver<()>,
        stats: Arc<Mutex<RenderStats>>,
        last_error: Arc<Mutex<Option<String>>>,
        stop: Arc<AtomicBool>,
        title: String,
        demo: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);

        cx.spawn_in(window, async move |this, cx| {
            while frames_ready.next().await.is_some() {
                let alive = this.update_in(cx, |this, window, cx| this.take_frame(window, cx));
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();

        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this.update(cx, |this, cx| this.refresh_hud(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        if let Some(quit_after) = demo {
            cx.spawn_in(window, async move |this, cx| {
                for step in 0usize.. {
                    cx.background_executor()
                        .timer(Duration::from_millis(2600))
                        .await;
                    let more = this
                        .update_in(cx, |this, window, cx| {
                            this.demo_step(step, quit_after, window, cx)
                        })
                        .unwrap_or(false);
                    if !more {
                        break;
                    }
                }
            })
            .detach();
        }

        Self {
            mpv,
            mailbox,
            current: None,
            retired: None,
            stats,
            upload: Arc::new(Mutex::new((None, Timing::default()))),
            last_error,
            displayed: 0,
            fps_sample: (Instant::now(), 0, 0),
            hud: Hud::default(),
            action: "Loading…".into(),
            title: title.into(),
            focus,
            stop,
        }
    }

    fn take_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(frame) = self.mailbox.lock().unwrap().take() else {
            return;
        };
        // Evict the frame shown two updates ago: by now no in-flight GPU
        // frame can still sample its atlas texture.
        if let Some(stale) = self.retired.take() {
            let _ = window.drop_image(stale);
        }
        self.retired = self.current.replace(frame);
        self.displayed += 1;
        cx.notify();
    }

    fn refresh_hud(&mut self, cx: &mut Context<Self>) {
        let mpv = &self.mpv;
        let text = |name: &str| mpv.get_string(name).unwrap_or_else(|_| "-".into());
        let track = |kind: &str| {
            let id = text(&format!("current-tracks/{kind}/id"));
            if id == "-" {
                return "off".to_string();
            }
            let lang = text(&format!("current-tracks/{kind}/lang"));
            let codec = text(&format!("current-tracks/{kind}/codec"));
            format!("#{id} {lang} ({codec})")
        };
        let (started, displayed, produced) = self.fps_sample;
        let elapsed = started.elapsed().as_secs_f64().max(0.001);
        let produced_now = self.stats.lock().unwrap().produced;
        self.hud = Hud {
            position: mpv.get_f64("time-pos").unwrap_or(0.0),
            duration: mpv.get_f64("duration").unwrap_or(0.0),
            paused: mpv.get_flag("pause").unwrap_or(false),
            muted: mpv.get_flag("mute").unwrap_or(false),
            volume: mpv.get_f64("volume").unwrap_or(0.0),
            audio: track("audio"),
            subtitle: track("sub"),
            hwdec: text("hwdec-current"),
            codec: format!(
                "{} {}x{} {}",
                text("video-format"),
                text("video-params/w"),
                text("video-params/h"),
                text("video-params/pixelformat")
            ),
            displayed_fps: (self.displayed - displayed) as f64 / elapsed,
            produced_fps: (produced_now - produced) as f64 / elapsed,
        };
        if elapsed > 2.0 {
            self.fps_sample = (Instant::now(), self.displayed, produced_now);
        }
        if let Some(error) = self.last_error.lock().unwrap().take() {
            self.action = format!("Playback error: {error}").into();
        }
        cx.notify();
    }

    fn act(&mut self, label: impl Into<SharedString>, run: impl FnOnce(&Mpv) -> Result<()>) {
        let label = label.into();
        self.action = match run(&self.mpv) {
            Ok(()) => label,
            Err(error) => format!("{label} failed: {error:#}").into(),
        };
        println!("ACTION {}", self.action);
    }

    fn cycle_track(&mut self, kind: &str, property: &str) {
        let all: Vec<Track> = tracks(&self.mpv).unwrap_or_default();
        let candidates: Vec<&Track> = all.iter().filter(|track| track.kind == kind).collect();
        let current = candidates.iter().position(|track| track.selected);
        let next = match current {
            Some(index) if index + 1 < candidates.len() => Some(candidates[index + 1]),
            Some(_) if kind == "sub" => None,
            _ => candidates.first().copied(),
        };
        let (value, label) = match next {
            Some(track) => (
                track.id.to_string(),
                format!(
                    "{} → #{} {} {}",
                    if kind == "audio" {
                        "Audio"
                    } else {
                        "Subtitles"
                    },
                    track.id,
                    track.lang.as_deref().unwrap_or("und"),
                    track.title.as_deref().unwrap_or("")
                ),
            ),
            None => ("no".to_string(), "Subtitles → off".to_string()),
        };
        self.act(label, |mpv| mpv.set(property, &value));
    }

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "space" => self.act("Toggle pause", |mpv| mpv.command(&["cycle", "pause"])),
            "left" => self.act("Seek −10s", |mpv| {
                mpv.command(&["seek", "-10", "relative+exact"])
            }),
            "right" => self.act("Seek +10s", |mpv| {
                mpv.command(&["seek", "10", "relative+exact"])
            }),
            "up" => self.act("Volume +5", |mpv| mpv.command(&["add", "volume", "5"])),
            "down" => self.act("Volume −5", |mpv| mpv.command(&["add", "volume", "-5"])),
            "m" => self.act("Toggle mute", |mpv| mpv.command(&["cycle", "mute"])),
            "a" => self.cycle_track("audio", "aid"),
            "s" => self.cycle_track("sub", "sid"),
            "f" => {
                window.toggle_fullscreen();
                self.action = "Toggle fullscreen".into();
            }
            "q" | "escape" => self.quit(cx),
            _ => return,
        }
        cx.notify();
    }

    fn demo_step(
        &mut self,
        step: usize,
        quit_after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match step {
            0 => self.act("Demo: seek to 0:20 (exact)", |mpv| {
                mpv.command(&["seek", "20", "absolute+exact"])
            }),
            1 => self.cycle_track("audio", "aid"),
            2 => {
                let last_sub = tracks(&self.mpv)
                    .unwrap_or_default()
                    .into_iter()
                    .rfind(|track| track.kind == "sub");
                match last_sub {
                    Some(track) => self.act(
                        format!(
                            "Subtitles → #{} {}",
                            track.id,
                            track.lang.as_deref().unwrap_or("und")
                        ),
                        |mpv| mpv.set("sid", &track.id.to_string()),
                    ),
                    None => self.action = "Demo: no subtitle tracks".into(),
                }
            }
            3 => self.act("Demo: pause", |mpv| mpv.set("pause", "yes")),
            4 => self.act("Demo: play", |mpv| mpv.set("pause", "no")),
            5 => self.act("Demo: volume 40%", |mpv| mpv.set("volume", "40")),
            6 => self.act("Demo: seek −10s", |mpv| {
                mpv.command(&["seek", "-10", "relative+exact"])
            }),
            7 => self.act("Demo: subtitles off", |mpv| mpv.set("sid", "no")),
            8 => {
                window.toggle_fullscreen();
                self.action = "Demo: enter fullscreen".into();
            }
            9 => {
                window.toggle_fullscreen();
                self.action = "Demo: exit fullscreen".into();
            }
            _ => {
                if quit_after {
                    self.quit(cx);
                } else {
                    self.print_stats();
                }
                return false;
            }
        }
        cx.notify();
        true
    }

    fn print_stats(&self) {
        let stats = self.stats.lock().unwrap();
        let upload = self.upload.lock().unwrap();
        println!("STATS frame {}x{}", stats.size.0, stats.size.1);
        println!(
            "STATS produced={} replaced-unshown={} displayed={}",
            stats.produced, stats.replaced_unshown, self.displayed
        );
        println!("STATS mpv-sw-render {}", stats.mpv_render.summary());
        println!("STATS force-opaque {}", stats.force_opaque.summary());
        println!(
            "STATS alloc+RenderImage {}",
            stats.allocate_and_wrap.summary()
        );
        println!(
            "STATS gpui-paint_image(new frame: atlas alloc+upload) {}",
            upload.1.summary()
        );
    }

    fn quit(&mut self, cx: &mut Context<Self>) {
        self.print_stats();
        self.stop.store(true, Ordering::Relaxed);
        cx.quit();
    }
}

fn contain(bounds: Bounds<Pixels>, image: (f32, f32)) -> Bounds<Pixels> {
    let (bw, bh) = (bounds.size.width / px(1.0), bounds.size.height / px(1.0));
    let scale = (bw / image.0).min(bh / image.1);
    let (w, h) = (image.0 * scale, image.1 * scale);
    Bounds {
        origin: point(
            bounds.origin.x + px((bw - w) / 2.0),
            bounds.origin.y + px((bh - h) / 2.0),
        ),
        size: size(px(w), px(h)),
    }
}

fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

impl Render for Player {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let frame = self.current.clone();
        let upload = self.upload.clone();
        let video = canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let Some(frame) = frame else {
                    return;
                };
                let pixels = frame.size(0);
                let target = contain(bounds, (pixels.width.0 as f32, pixels.height.0 as f32));
                let is_new = upload.lock().unwrap().0 != Some(frame.id);
                let began = Instant::now();
                let id = frame.id;
                let _ = window.paint_image(target, Corners::default(), frame, 0, false);
                if is_new {
                    let mut upload = upload.lock().unwrap();
                    upload.0 = Some(id);
                    upload.1.record(began.elapsed());
                }
            },
        )
        .size_full();

        let hud = &self.hud;
        let stats = self.stats.lock().unwrap();
        let upload_ms = self.upload.lock().unwrap().1.mean_ms();
        let progress = if hud.duration > 0.0 {
            (hud.position / hud.duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let line = |label: &str, value: String| {
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .w(px(96.))
                        .text_color(rgb(0x9aa4b2))
                        .child(label.to_string()),
                )
                .child(div().text_color(rgb(0xf4f6f8)).child(value))
        };
        let panel = div()
            .absolute()
            .top(px(16.))
            .left(px(16.))
            .p_3()
            .rounded_lg()
            .bg(rgba(0x0b0f14cc))
            .border_1()
            .border_color(rgba(0xffffff22))
            .text_xs()
            .font_family("JetBrains Mono")
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xffffff))
                    .child(self.title.clone()),
            )
            .child(line(
                "pipeline",
                "libmpv sw-render → BGRA → GPUI RenderImage".into(),
            ))
            .child(line("video", hud.codec.clone()))
            .child(line("hwdec", hud.hwdec.clone()))
            .child(line("audio", hud.audio.clone()))
            .child(line("subtitles", hud.subtitle.clone()))
            .child(line(
                "volume",
                format!(
                    "{:.0}%{}",
                    hud.volume,
                    if hud.muted { " (muted)" } else { "" }
                ),
            ))
            .child(line(
                "frames",
                format!(
                    "{}x{} · produced {:.1} fps · shown {:.1} fps",
                    stats.size.0, stats.size.1, hud.produced_fps, hud.displayed_fps
                ),
            ))
            .child(line(
                "cost/frame",
                format!(
                    "render {:.2}ms · alpha {:.2}ms · wrap {:.2}ms · upload {:.2}ms",
                    stats.mpv_render.mean_ms(),
                    stats.force_opaque.mean_ms(),
                    stats.allocate_and_wrap.mean_ms(),
                    upload_ms
                ),
            ));
        drop(stats);

        let controls = div()
            .absolute()
            .bottom(px(16.))
            .left(px(16.))
            .right(px(16.))
            .p_3()
            .rounded_lg()
            .bg(rgba(0x0b0f14cc))
            .border_1()
            .border_color(rgba(0xffffff22))
            .flex()
            .flex_col()
            .gap_2()
            .text_color(rgb(0xf4f6f8))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_sm()
                    .child(format!(
                        "{}  {} / {}",
                        if hud.paused { "Paused" } else { "Playing" },
                        clock(hud.position),
                        clock(hud.duration)
                    ))
                    .child(div().text_color(rgb(0xe6a452)).child(self.action.clone())),
            )
            .child(
                div()
                    .h(px(4.))
                    .w_full()
                    .rounded_full()
                    .bg(rgba(0xffffff33))
                    .child(div().h_full().rounded_full().w(relative(progress as f32)).bg(rgb(0xe6a452))),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x9aa4b2))
                    .child("space play/pause · ←/→ seek 10s · ↑/↓ volume · a audio · s subtitles · m mute · f fullscreen · q quit"),
            );

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::handle_key))
            .size_full()
            .relative()
            .bg(rgb(0x000000))
            .font_family("Inter")
            .child(video)
            .child(panel)
            .child(controls)
    }
}

fn main() -> Result<()> {
    let options = parse_options()?;
    let mut mpv_options = vec![
        ("vo", "libmpv"),
        ("hwdec", options.hwdec.as_str()),
        ("keep-open", "yes"),
        ("terminal", "no"),
        ("ytdl", "no"),
        // GPUI owns the chrome; mpv still renders subtitles into the frame,
        // lifted clear of the GPUI controls overlay.
        ("osd-level", "0"),
        ("sub-margin-y", "120"),
    ];
    if let Some(start) = &options.start {
        mpv_options.push(("start", start.as_str()));
    }
    if let Some(ao) = &options.ao {
        mpv_options.push(("ao", ao.as_str()));
    }
    let mpv = Mpv::new(&mpv_options)?;

    let mailbox: Mailbox = Arc::new(Mutex::new(None));
    let stats = Arc::new(Mutex::new(RenderStats::default()));
    let last_error = Arc::new(Mutex::new(None));
    let stop = Arc::new(AtomicBool::new(false));
    let (wake_ui, frames_ready) = unbounded();
    // The render context must exist before loadfile, or vo=libmpv has no target.
    let _render_thread = spawn_render_thread(
        mpv.clone(),
        options.max_size,
        mailbox.clone(),
        wake_ui,
        stats.clone(),
        stop.clone(),
    )?;
    spawn_event_thread(mpv.clone(), last_error.clone(), stop.clone());
    mpv.command(&["loadfile", &options.source])?;

    let title = format!(
        "Matinee native playback spike — {}",
        options
            .source
            .rsplit('/')
            .next()
            .unwrap_or(&options.source)
            .split('?')
            .next()
            .unwrap_or("")
    );
    let demo = options.demo.then_some(options.demo_quit);
    Application::new().run(move |cx: &mut App| {
        cx.on_window_closed(|cx| cx.quit()).detach();
        let bounds = Bounds::centered(None, size(px(1280.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Matinee native playback spike (GPUI + libmpv)".into()),
                    ..Default::default()
                }),
                focus: true,
                ..Default::default()
            },
            move |window, cx| {
                cx.new(|cx| {
                    Player::new(
                        mpv,
                        mailbox,
                        frames_ready,
                        stats,
                        last_error,
                        stop,
                        title,
                        demo,
                        window,
                        cx,
                    )
                })
            },
        )
        .expect("open window");
        cx.activate(true);
    });
    Ok(())
}
