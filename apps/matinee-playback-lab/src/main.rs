//! Playback Lab.
//!
//! A utilitarian harness: load a URL, transport, tracks, and the latest frame
//! on [`ExternalFrameSurface`]. It is not the Matinee Player screen.
//!
//! The engine thread signals this window when a frame is ready. A slow timer
//! refreshes the readout only. It does not pace frames.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use atelier_app::{
    AppInfo, AtelierApp, ChromeIntent, Platform, WindowSpec, on_fullscreen_escape, open_window,
    resolve_chrome, titlebar_leading, titlebar_spacer,
};
use atelier_ui::prelude::*;
use futures::StreamExt;
use futures::channel::mpsc::unbounded;
use matinee_player::{
    CpuFrame, Diagnostics, LoadRequest, PlaybackState, Player, Snapshot, TrackId,
};
use matinee_ui::matinee_theme;

const MIN_SIZE: (f32, f32) = (960.0, 640.0);

struct Launch {
    url: Option<String>,
    demo: bool,
    exit_after: Option<Duration>,
    probe_4k: bool,
}

struct Lab {
    focus: FocusHandle,
    surface: Entity<ExternalFrameSurface>,
    player: Option<Player>,
    url: SharedString,
    notice: SharedString,
    tracks_open: bool,
    fit: usize,
    demo: bool,
    phase: u8,
    exit_after: Option<Duration>,
    started: Instant,
    logged: u8,
}

impl Lab {
    fn new(window: &mut Window, cx: &mut Context<Self>, launch: Launch) -> Self {
        let surface = cx.new(|cx| ExternalFrameSurface::new("No frame", window, cx));
        let (player, notice) = match Player::open() {
            Ok(player) => {
                let mailbox_owner = surface.clone();
                let (tx, mut rx) = unbounded();
                player.set_frame_listener(move || {
                    let _ = tx.unbounded_send(());
                });
                cx.spawn_in(window, async move |this, cx| {
                    while rx.next().await.is_some() {
                        while rx.try_recv().is_ok() {}
                        let alive = this.update(cx, |lab, cx| {
                            if let Some(frame) = lab.player.as_ref().and_then(Player::take_frame) {
                                publish(frame, &mailbox_owner, cx);
                            }
                        });
                        if alive.is_err() {
                            break;
                        }
                    }
                })
                .detach();
                (Some(player), String::new())
            }
            Err(error) => (None, error.to_string()),
        };
        let url = launch.url.clone().unwrap_or_default();
        let lab = Self {
            focus: cx.focus_handle(),
            surface,
            player,
            url: url.into(),
            notice: notice.into(),
            tracks_open: false,
            fit: 0,
            demo: launch.demo,
            phase: 0,
            exit_after: launch.exit_after,
            started: Instant::now(),
            logged: 0,
        };
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if this
                    .update_in(cx, |lab, window, cx| lab.on_tick(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        lab
    }

    fn on_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.demo {
            let elapsed = self.started.elapsed();
            const STEPS: [Duration; 8] = [
                Duration::from_millis(400),
                Duration::from_millis(1600),
                Duration::from_millis(2200),
                Duration::from_millis(2800),
                Duration::from_millis(3400),
                Duration::from_millis(4000),
                Duration::from_millis(4800),
                Duration::from_millis(5600),
            ];
            while (self.phase as usize) < STEPS.len() && elapsed >= STEPS[self.phase as usize] {
                self.run_step(window, cx);
                self.phase += 1;
            }
        }
        let second = u8::try_from(self.started.elapsed().as_secs()).unwrap_or(u8::MAX);
        if second != self.logged {
            self.logged = second;
            self.log_line(cx);
        }
        if self
            .exit_after
            .is_some_and(|limit| self.started.elapsed() >= limit)
        {
            cx.quit();
        }
        cx.notify();
    }

    fn run_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.phase {
            0 => self.load(cx),
            1 => self.command(cx, Player::pause),
            2 => self.command(cx, |player| player.seek(Duration::from_millis(1500))),
            3 => self.command(cx, Player::play),
            4 => self.command(cx, |player| player.set_volume(0.35)),
            5 => window.zoom_window(),
            6 => window.toggle_fullscreen(),
            _ => {
                if window.is_fullscreen() {
                    window.toggle_fullscreen();
                }
            }
        }
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let url = self.url.trim().to_string();
        if url.is_empty() {
            self.notice = "Enter a file path or URL.".into();
            return;
        }
        let Some(player) = &self.player else {
            return;
        };
        match player.load(LoadRequest {
            url,
            start: None,
            headers: Vec::new(),
        }) {
            Ok(()) => self.notice = SharedString::default(),
            Err(error) => self.notice = error.to_string().into(),
        }
        cx.notify();
    }

    fn command(
        &mut self,
        cx: &mut Context<Self>,
        run: impl FnOnce(&Player) -> Result<(), matinee_player::PlayerError>,
    ) {
        let Some(player) = &self.player else {
            self.notice = "Playback engine is unavailable.".into();
            return;
        };
        if let Err(error) = run(player) {
            self.notice = error.to_string().into();
        }
        cx.notify();
    }

    fn log_line(&self, cx: &mut Context<Self>) {
        let snapshot = self.snapshot();
        let stats = self.stats();
        let presented = self.surface.read(cx).presented_count();
        let upload = self.surface.read(cx).last_upload();
        eprintln!(
            "playback-lab state={} pos_ms={} produced={} replaced={} depth={} frame={}x{} render_mean_us={} presented={} upload_us={}",
            state_label(snapshot.state),
            snapshot.position.as_millis(),
            stats.produced,
            stats.replaced,
            stats.mailbox_depth,
            stats.frame_width,
            stats.frame_height,
            stats.render_mean.as_micros(),
            presented,
            upload.as_micros(),
        );
    }

    fn snapshot(&self) -> Snapshot {
        self.player
            .as_ref()
            .map(Player::snapshot)
            .unwrap_or_default()
    }

    fn stats(&self) -> Diagnostics {
        self.player
            .as_ref()
            .map(Player::diagnostics)
            .unwrap_or_default()
    }
}

fn publish(frame: CpuFrame, surface: &Entity<ExternalFrameSurface>, cx: &mut App) {
    let generation = frame.generation();
    let width = frame.width();
    let height = frame.height();
    let stride = frame.stride();
    let pixels = frame.into_pixels();
    let Ok(bgra) = BgraFrame::new(width, height, stride, pixels, generation) else {
        return;
    };
    surface.update(cx, |surface, _| {
        let _ = surface.mailbox().publish(bgra);
    });
}

impl Render for Lab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let chrome = resolve_chrome(Platform::current(), ChromeIntent::PlatformDefault);
        let row_height = if chrome.band_height > 0.0 {
            chrome.band_height
        } else {
            40.0
        };
        let snapshot = self.snapshot();
        let stats = self.stats();
        let presented = self.surface.read(cx).presented_count();
        let upload = self.surface.read(cx).last_upload();
        let frame = self.surface.read(cx).frame_size();
        let fullscreen = window.is_fullscreen();
        let maximized = window.is_maximized();
        let url = self.url.clone();
        let notice = if self.notice.is_empty() {
            snapshot.error.clone().unwrap_or_default().into()
        } else {
            self.notice.clone()
        };
        let fit = self.fit;
        let tracks_open = self.tracks_open;
        let audio = snapshot.audio_tracks.clone();
        let subtitles = snapshot.subtitle_tracks.clone();
        let selected_audio = snapshot.selected_audio;
        let selected_subtitle = snapshot.selected_subtitle;
        let duration_ms = snapshot
            .duration
            .map(|duration| duration.as_millis() as f32)
            .unwrap_or(0.0);
        let position_ms = snapshot.position.as_millis() as f32;

        let lab = cx.entity();
        let load = cx.entity();
        let play = cx.entity();
        let pause = cx.entity();
        let stop = cx.entity();
        let mute = cx.entity();
        let seek = cx.entity();
        let volume = cx.entity();
        let fit_lab = cx.entity();
        let tracks = cx.entity();
        let dismiss = cx.entity();

        v_stack(Space::S0)
            .id("playback-lab")
            .track_focus(&self.focus)
            .on_key_down(on_fullscreen_escape)
            .size_full()
            .bg(theme.colors.surface.canvas)
            .font_family(theme.typography.families.interface)
            .text_color(theme.colors.text.primary)
            .child(titlebar(
                &theme, row_height, chrome, fullscreen, maximized, fit, fit_lab,
            ))
            .child(
                v_stack(Space::S3)
                    .p(Space::S4.px())
                    .flex_none()
                    .child(
                        h_stack(Space::S2)
                            .items_center()
                            .child(
                                div().flex_1().child(
                                    TextField::new("source-url", url)
                                        .placeholder("File path or URL")
                                        .on_change(move |value, _, cx| {
                                            lab.update(cx, |this, cx| {
                                                this.url = value;
                                                cx.notify();
                                            });
                                        }),
                                ),
                            )
                            .child(
                                Button::new("load", "Load")
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| {
                                        load.update(cx, |this, cx| this.load(cx));
                                    }),
                            ),
                    )
                    .child(
                        h_stack(Space::S2)
                            .items_center()
                            .child(transport("play", "Play", play, Player::play))
                            .child(transport("pause", "Pause", pause, Player::pause))
                            .child(transport("stop", "Stop", stop, Player::stop))
                            .child(transport(
                                "mute",
                                if snapshot.muted { "Unmute" } else { "Mute" },
                                mute,
                                |player| player.set_muted(!player.snapshot().muted),
                            ))
                            .child(
                                div().w(px(160.0)).child(
                                    Slider::new("volume", snapshot.volume * 100.0)
                                        .range(0.0, 100.0)
                                        .step(1.0)
                                        .on_change(move |next, _, cx| {
                                            volume.update(cx, |this, cx| {
                                                this.command(cx, |player| {
                                                    player.set_volume(next / 100.0)
                                                });
                                            });
                                        }),
                                ),
                            )
                            .child(
                                div().flex_1().child(
                                    Slider::new("seek", position_ms.min(duration_ms))
                                        .range(0.0, duration_ms.max(1.0))
                                        .step(100.0)
                                        .disabled(duration_ms <= 0.0)
                                        .on_change(move |next, _, cx| {
                                            seek.update(cx, |this, cx| {
                                                this.command(cx, |player| {
                                                    player.seek(Duration::from_millis(next as u64))
                                                });
                                            });
                                        }),
                                ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .relative()
                    .child(self.surface.clone())
                    .child(
                        div()
                            .absolute()
                            .bottom(Space::S3.px())
                            .left(Space::S3.px())
                            .child(
                                Popover::new("track-menu")
                                    .open(tracks_open)
                                    .placement(Placement::Top)
                                    .on_dismiss(move |_, cx| {
                                        dismiss.update(cx, |this, cx| {
                                            this.tracks_open = false;
                                            cx.notify();
                                        });
                                    })
                                    .trigger(
                                        Button::new("tracks", "Tracks")
                                            .size(ButtonSize::Small)
                                            .on_click(move |_, _, cx| {
                                                tracks.update(cx, |this, cx| {
                                                    this.tracks_open = !this.tracks_open;
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .menu(track_entries(
                                        cx.entity(),
                                        &audio,
                                        &subtitles,
                                        selected_audio,
                                        selected_subtitle,
                                    )),
                            ),
                    ),
            )
            .child(readout(
                &theme, &snapshot, &stats, frame, presented, upload, &notice,
            ))
    }
}

fn transport(
    id: &'static str,
    label: &'static str,
    lab: Entity<Lab>,
    run: impl Fn(&Player) -> Result<(), matinee_player::PlayerError> + 'static,
) -> impl IntoElement {
    Button::new(id, label)
        .size(ButtonSize::Small)
        .on_click(move |_, _, cx| {
            lab.update(cx, |this, cx| this.command(cx, &run));
        })
}

fn titlebar(
    theme: &Theme,
    height: f32,
    chrome: atelier_app::ResolvedChrome,
    fullscreen: bool,
    maximized: bool,
    fit: usize,
    lab: Entity<Lab>,
) -> impl IntoElement {
    let fit_lab = lab.clone();
    h_stack(Space::S2)
        .w_full()
        .h(px(height))
        .flex_none()
        .items_center()
        .px(Space::S3.px())
        .bg(theme.colors.surface.panel)
        .border_b_1()
        .border_color(theme.colors.border.subtle)
        .when(chrome.leading_inset > 0.0, |row| {
            row.child(titlebar_leading(chrome.leading_inset, chrome))
        })
        .child(Text::new("Playback Lab").role(TextRole::Heading))
        .child(titlebar_spacer(chrome))
        .child(
            SegmentedControl::new(
                "fit-mode",
                vec![Segment::new("Fit"), Segment::new("Fill")],
                fit,
            )
            .on_change(move |index, _, cx| {
                fit_lab.update(cx, |this, cx| {
                    this.fit = index;
                    let mode = if index == 0 {
                        ImageFit::Fit
                    } else {
                        ImageFit::Fill
                    };
                    this.surface
                        .update(cx, |surface, cx| surface.set_fit(mode, cx));
                    cx.notify();
                });
            }),
        )
        .child(
            Button::new("maximize", if maximized { "Restore" } else { "Maximize" })
                .size(ButtonSize::Small)
                .on_click(|_, window, _| window.zoom_window()),
        )
        .child(
            Button::new(
                "fullscreen",
                if fullscreen {
                    "Exit full screen"
                } else {
                    "Full screen"
                },
            )
            .size(ButtonSize::Small)
            .on_click(|_, window, _| window.toggle_fullscreen()),
        )
        .child(titlebar_spacer(chrome))
}

fn track_entries(
    lab: Entity<Lab>,
    audio: &[matinee_player::Track],
    subtitles: &[matinee_player::Track],
    selected_audio: Option<TrackId>,
    selected_subtitle: Option<TrackId>,
) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    entries.push(MenuEntry::Item(MenuItem::new("Audio").disabled(true)));
    if audio.is_empty() {
        entries.push(MenuEntry::Item(MenuItem::new("None").disabled(true)));
    }
    for track in audio {
        let id = track.id;
        let owner = lab.clone();
        entries.push(MenuEntry::Item(
            MenuItem::new(track_label(track))
                .checked(selected_audio == Some(id))
                .on_activate(move |_, cx| {
                    owner.update(cx, |this, cx| {
                        this.command(cx, |player| player.select_audio(id));
                        this.tracks_open = false;
                    });
                }),
        ));
    }
    entries.push(MenuEntry::Separator(MenuSeparator));
    entries.push(MenuEntry::Item(MenuItem::new("Subtitles").disabled(true)));
    let off = lab.clone();
    entries.push(MenuEntry::Item(
        MenuItem::new("Off")
            .checked(selected_subtitle.is_none())
            .on_activate(move |_, cx| {
                off.update(cx, |this, cx| {
                    this.command(cx, Player::disable_subtitles);
                    this.tracks_open = false;
                });
            }),
    ));
    for track in subtitles {
        let id = track.id;
        let owner = lab.clone();
        entries.push(MenuEntry::Item(
            MenuItem::new(track_label(track))
                .checked(selected_subtitle == Some(id))
                .on_activate(move |_, cx| {
                    owner.update(cx, |this, cx| {
                        this.command(cx, |player| player.select_subtitle(id));
                        this.tracks_open = false;
                    });
                }),
        ));
    }
    entries
}

fn track_label(track: &matinee_player::Track) -> String {
    let language = track.language.as_deref().unwrap_or("und");
    let codec = track.codec.as_deref().unwrap_or("unknown");
    format!("{language} · {codec}")
}

fn readout(
    theme: &Theme,
    snapshot: &Snapshot,
    stats: &Diagnostics,
    frame: Option<(u32, u32)>,
    presented: u64,
    upload: Duration,
    notice: &SharedString,
) -> impl IntoElement {
    let picture = match frame {
        Some((width, height)) => format!("{width}×{height}"),
        None => "—".to_string(),
    };
    let source = match snapshot.source_size {
        Some((width, height)) => format!("{width}×{height}"),
        None => "—".to_string(),
    };
    v_stack(Space::S1)
        .w_full()
        .flex_none()
        .px(Space::S4.px())
        .py(Space::S2.px())
        .bg(theme.colors.surface.panel)
        .border_t_1()
        .border_color(theme.colors.border.subtle)
        .child(
            Text::new(format!(
                "{} · {} / {} · source {source} · frame {picture}",
                state_label(snapshot.state),
                clock(snapshot.position),
                snapshot
                    .duration
                    .map(clock)
                    .unwrap_or_else(|| "—".to_string()),
            ))
            .role(TextRole::Caption),
        )
        .child(
            Text::new(format!(
                "produced {} · replaced {} · depth {} · render {:.2} ms · presented {} · upload {:.2} ms",
                stats.produced,
                stats.replaced,
                stats.mailbox_depth,
                stats.render_mean.as_secs_f64() * 1000.0,
                presented,
                upload.as_secs_f64() * 1000.0,
            ))
            .role(TextRole::Caption)
            .tone(TextTone::Muted),
        )
        .child(
            Text::new(if notice.is_empty() {
                "Tracks stays above the frame.".to_string()
            } else {
                notice.to_string()
            })
            .role(TextRole::Caption)
            .tone(TextTone::Secondary),
        )
}

fn state_label(state: PlaybackState) -> &'static str {
    match state {
        PlaybackState::Idle => "Idle",
        PlaybackState::Loading => "Loading",
        PlaybackState::Playing => "Playing",
        PlaybackState::Paused => "Paused",
        PlaybackState::Buffering => "Buffering",
        PlaybackState::Ended => "Ended",
        PlaybackState::Error => "Error",
    }
}

fn clock(duration: Duration) -> String {
    let total = duration.as_secs();
    format!("{}:{:02}", total / 60, total % 60)
}

fn parse_args() -> Launch {
    let mut url = None;
    let mut demo = false;
    let mut exit_after = None;
    let mut probe_4k = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => demo = true,
            "--probe-4k" => probe_4k = true,
            "--exit-after" => {
                let seconds = args
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(8);
                exit_after = Some(Duration::from_secs(seconds));
            }
            "--source" => url = args.next(),
            other if !other.starts_with('-') && url.is_none() => url = Some(other.to_string()),
            other => eprintln!("unknown argument {other}"),
        }
    }
    if url.is_none() {
        url = std::env::var("MATINEE_PLAYBACK_SOURCE").ok();
    }
    Launch {
        url,
        demo,
        exit_after,
        probe_4k,
    }
}

fn ensure_demo_source(launch: &mut Launch) -> Option<PathBuf> {
    if launch.url.is_none() && (launch.demo || launch.probe_4k) {
        let path = std::env::temp_dir().join("matinee-playback-lab-1080p.mkv");
        if !generate_1080p(&path) {
            return None;
        }
        launch.url = Some(path.display().to_string());
    }
    launch.url.as_ref().map(PathBuf::from)
}

fn generate_1080p(path: &std::path::Path) -> bool {
    if path.exists() {
        return true;
    }
    let srt = path.with_extension("srt");
    if std::fs::write(
        &srt,
        "1\n00:00:00,500 --> 00:00:04,000\nPlayback Lab\n\n2\n00:00:04,000 --> 00:00:08,000\nNative frame\n",
    )
    .is_err()
    {
        return false;
    }
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1920x1080:rate=24:duration=8",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=8",
            "-i",
        ])
        .arg(&srt)
        .args([
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-c:s",
            "srt",
            "-shortest",
        ])
        .arg(path)
        .output();
    match output {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            false
        }
        Err(error) => {
            eprintln!("ffmpeg failed to start: {error}");
            false
        }
    }
}

fn probe_4k(path: &std::path::Path) {
    match matinee_player::measure_software_render(path, 3840, 2160, 8) {
        Ok(stats) => {
            println!(
                "4k-cpu-probe frames={} render_mean_ms={:.2} opaque_mean_ms={:.2} size={}x{}",
                stats.produced,
                stats.render_mean.as_secs_f64() * 1000.0,
                stats.opaque_mean.as_secs_f64() * 1000.0,
                stats.frame_width,
                stats.frame_height,
            );
        }
        Err(error) => eprintln!("4k probe failed: {error}"),
    }
}

fn main() {
    let mut launch = parse_args();
    let source = ensure_demo_source(&mut launch);
    if launch.probe_4k {
        if let Some(path) = &source {
            probe_4k(path);
        } else {
            eprintln!("--probe-4k needs a source");
        }
        if !launch.demo && launch.exit_after.is_none() {
            return;
        }
    }

    let launch_for_window = Launch {
        url: launch.url.clone(),
        demo: launch.demo,
        exit_after: launch.exit_after,
        probe_4k: false,
    };
    AtelierApp::new(AppInfo {
        name: "Playback Lab",
        app_id: "dev.sean.matinee.playback-lab",
    })
    .theme(matinee_theme())
    .run(move |cx| {
        if let Err(error) = matinee_ui::load_bundled_fonts(cx) {
            eprintln!("failed to load Matinee fonts: {error}");
        }
        let launch = Launch {
            url: launch_for_window.url.clone(),
            demo: launch_for_window.demo,
            exit_after: launch_for_window.exit_after,
            probe_4k: false,
        };
        open_window(
            cx,
            WindowSpec::new("Playback Lab", (1280.0, 800.0))
                .min_size(MIN_SIZE)
                .placement_max((1600.0, 1000.0))
                .restoration_key("playback-lab"),
            move |window, cx| {
                cx.new(|cx| {
                    let lab = Lab::new(window, cx, launch);
                    window.focus(&lab.focus);
                    lab
                })
            },
        )
        .expect("failed to open playback lab");
    });
}
