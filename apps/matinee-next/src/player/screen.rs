//! Player screen.
//!
//! This view paints [`super::model::PlayerModel`]. It does not decide source
//! selection, reporting, or playback state. Frames are taken only after the
//! engine asks the UI to wake up.

use std::sync::Arc;
use std::time::{Duration, Instant};

use atelier_ui::gpui::{EventEmitter, KeyDownEvent};
use atelier_ui::prelude::*;
use futures::StreamExt;
use futures::channel::mpsc::unbounded;
use matinee_core::{ItemId, PlaybackReport, ReportKind, StreamAuthorization};
use matinee_jellyfin::{JellyfinClient, ReqwestTransport, Session};
use matinee_player::{LoadRequest, Player, PlayerError};
use tokio::task::JoinHandle;

use crate::runtime::ServiceRuntime;

use super::frame::{bgra_from_cpu, fixture_frame};
use super::model::{
    Directive, MenuKind, PlanFailure, PlayerFailure, PlayerModel, PlayerPreview, UserCommand,
    format_clock, track_label,
};
use super::prepare::prepare_playback;
use super::{PROGRESS_INTERVAL, SEEK_STEP_MS, VOLUME_STEP};

pub(crate) enum KeyOutcome {
    Ignored,
    Handled,
    Leave,
}

pub(crate) struct PlayerScreen {
    runtime: Arc<ServiceRuntime>,
    session: Option<Session>,
    model: PlayerModel,
    surface: Entity<ExternalFrameSurface>,
    focus: FocusHandle,
    tasks: Vec<JoinHandle<()>>,
    player: Option<Player>,
}

impl PlayerScreen {
    pub(crate) fn open(
        runtime: Arc<ServiceRuntime>,
        session: Session,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let surface = cx.new(|cx| ExternalFrameSurface::new("Video", window, cx));
        let mut screen = Self {
            runtime,
            session: Some(session),
            model: PlayerModel::new(),
            surface,
            focus: cx.focus_handle(),
            tasks: Vec::new(),
            player: None,
        };
        screen.model.begin(item_id);
        screen.start_prepare(cx);
        screen.start_clocks(window, cx);
        screen
    }

    /// Review scenes paint a generated frame. They do not open libmpv or a server.
    pub(crate) fn preview(
        runtime: Arc<ServiceRuntime>,
        scene: PlayerPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let surface = cx.new(|cx| ExternalFrameSurface::new("Video", window, cx));
        let frame = fixture_frame(scene);
        surface.update(cx, |surface, _| {
            let _ = surface.mailbox().publish(frame);
        });
        Self {
            runtime,
            session: None,
            model: PlayerModel::preview(scene),
            surface,
            focus: cx.focus_handle(),
            tasks: Vec::new(),
            player: None,
        }
    }

    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.model.menu().is_some()
    }

    pub(crate) fn on_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> KeyOutcome {
        if !plain_key(event) {
            return KeyOutcome::Ignored;
        }
        let key = event.keystroke.key.as_str();
        let surface_focused = self.focus.is_focused(window);
        let menu = self.menu_open();
        let outcome = match key {
            "escape" if menu => {
                self.activity(UserCommand::CloseMenu, cx);
                KeyOutcome::Handled
            }
            "escape" if window.is_fullscreen() => {
                window.toggle_fullscreen();
                KeyOutcome::Handled
            }
            "escape" => KeyOutcome::Leave,
            "f" if !menu => {
                window.toggle_fullscreen();
                self.activity(UserCommand::Activity, cx);
                KeyOutcome::Handled
            }
            "m" if !menu => {
                self.activity(UserCommand::ToggleMute, cx);
                KeyOutcome::Handled
            }
            "space" if surface_focused && !menu => {
                self.activity(UserCommand::TogglePlay, cx);
                KeyOutcome::Handled
            }
            "left" if surface_focused && !menu => {
                self.activity(UserCommand::SeekByMs(-SEEK_STEP_MS), cx);
                KeyOutcome::Handled
            }
            "right" if surface_focused && !menu => {
                self.activity(UserCommand::SeekByMs(SEEK_STEP_MS), cx);
                KeyOutcome::Handled
            }
            "up" if surface_focused && !menu => {
                self.activity(UserCommand::NudgeVolume(VOLUME_STEP), cx);
                KeyOutcome::Handled
            }
            "down" if surface_focused && !menu => {
                self.activity(UserCommand::NudgeVolume(-VOLUME_STEP), cx);
                KeyOutcome::Handled
            }
            _ => KeyOutcome::Ignored,
        };
        if matches!(outcome, KeyOutcome::Handled) {
            cx.stop_propagation();
        }
        outcome
    }

    fn start_prepare(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let Some(item_id) = self.model.item_id().cloned() else {
            return;
        };
        let (task, rx) = self
            .runtime
            .spawn(async move { prepare_playback(session, item_id).await });
        self.tasks.push(task);
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err(PlanFailure::stream("The request was cancelled.")));
            this.update(cx, |this, cx| {
                this.on_prepared(outcome, cx);
            })
            .ok();
        })
        .detach();
    }

    fn on_prepared(
        &mut self,
        result: Result<super::model::PreparedPlayback, PlanFailure>,
        cx: &mut Context<Self>,
    ) {
        if self.model.stage() != super::model::Stage::Resolving {
            return;
        }
        match result {
            Err(failure) => self.model.reject_plan(failure),
            Ok(prepared) => {
                if self.ensure_player(cx) {
                    let directives = self.model.accept_plan(prepared);
                    self.dispatch(directives, cx);
                    return;
                }
            }
        }
        cx.notify();
    }

    fn ensure_player(&mut self, cx: &mut Context<Self>) -> bool {
        if self.player.is_some() {
            return true;
        }
        match Player::open() {
            Ok(player) => {
                self.install_frames(&player, cx);
                self.player = Some(player);
                true
            }
            Err(PlayerError::LibraryMissing { .. }) => {
                self.model.reject_engine(PlayerFailure::library_missing());
                false
            }
            Err(PlayerError::LibraryIncompatible { .. }) => {
                self.model
                    .reject_engine(PlayerFailure::library_incompatible());
                false
            }
            Err(error) => {
                self.model
                    .reject_engine(PlayerFailure::engine(error.to_string()));
                false
            }
        }
    }

    fn install_frames(&self, player: &Player, cx: &mut Context<Self>) {
        let (tx, mut rx) = unbounded();
        player.set_frame_listener(move || {
            let _ = tx.unbounded_send(());
        });
        let surface = self.surface.clone();
        cx.spawn(async move |this, cx| {
            while rx.next().await.is_some() {
                while rx.try_recv().is_ok() {}
                let alive = this.update(cx, |this, cx| {
                    let Some(frame) = this.player.as_ref().and_then(Player::take_frame) else {
                        return;
                    };
                    let Ok(bgra) = bgra_from_cpu(frame) else {
                        return;
                    };
                    surface.update(cx, |surface, _| {
                        let _ = surface.mailbox().publish(bgra);
                    });
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn start_clocks(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this.update(cx, |this, cx| this.on_tick(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        let (task, mut rx) = self.runtime.interval(PROGRESS_INTERVAL);
        self.tasks.push(task);
        cx.spawn(async move |this, cx| {
            while rx.recv().await.is_some() {
                if this.update(cx, |this, cx| this.on_progress(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let mut directives = Vec::new();
        if let Some(player) = self.player.as_ref() {
            let snapshot = player.snapshot();
            directives.extend(self.model.ingest(snapshot, now));
            while let Some(event) = player.poll_event() {
                directives.extend(self.model.ingest_event(event, now));
            }
        }
        directives.extend(self.model.flush(now));
        if directives.is_empty() {
            cx.notify();
        } else {
            self.dispatch(directives, cx);
        }
    }

    fn on_progress(&mut self, cx: &mut Context<Self>) {
        let directives = self.model.progress_tick(Instant::now());
        if !directives.is_empty() {
            self.dispatch(directives, cx);
        }
    }

    fn activity(&mut self, command: UserCommand, cx: &mut Context<Self>) {
        let directives = self.model.command(command, Instant::now());
        if directives.is_empty() {
            cx.notify();
        } else {
            self.dispatch(directives, cx);
        }
    }

    fn dispatch(&mut self, directives: Vec<Directive>, cx: &mut Context<Self>) {
        for directive in directives {
            match directive {
                Directive::Report(kind) => self.spawn_report(kind, cx),
                Directive::Load(intent) => self.load(intent),
                other => self.apply_engine(&other),
            }
        }
        cx.notify();
    }

    fn load(&mut self, intent: super::model::LoadIntent) {
        let Some(player) = self.player.as_ref() else {
            self.model
                .reject_engine(PlayerFailure::engine("Playback could not start."));
            return;
        };
        let headers = match stream_headers(self.session.as_ref(), intent.authorization) {
            Ok(headers) => headers,
            Err(failure) => {
                self.model.reject_engine(failure);
                return;
            }
        };
        if let Err(error) = player.load(LoadRequest {
            url: intent.url,
            start: intent.start,
            headers,
        }) {
            self.model.fail_playback(error.to_string());
        }
    }

    fn apply_engine(&mut self, directive: &Directive) {
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let result = match directive {
            Directive::Play => player.play(),
            Directive::Pause => player.pause(),
            Directive::Stop => player.stop(),
            Directive::Seek(position) => player.seek(*position),
            Directive::SeekByMs(delta) => player.seek_by_ms(*delta),
            Directive::Volume(volume) => player.set_volume(*volume),
            Directive::Mute(muted) => player.set_muted(*muted),
            Directive::Audio(id) => player.select_audio(*id),
            Directive::Subtitle(id) => player.select_subtitle(*id),
            Directive::SubtitlesOff => player.disable_subtitles(),
            Directive::Load(_) | Directive::Report(_) => Ok(()),
        };
        if let Err(error) = result
            && self.model.stage() != super::model::Stage::Closed
        {
            self.model.fail_playback(error.to_string());
        }
    }

    fn spawn_report(&mut self, kind: ReportKind, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let Some(report) = self.model.report_body() else {
            return;
        };
        let (task, rx) = self
            .runtime
            .spawn(async move { send_report(session, kind, report).await });
        self.tasks.push(task);
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err("The request was cancelled.".into()));
            this.update(cx, |this, cx| {
                this.model.note_report(outcome);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn shutdown(&mut self) {
        let directives = self.model.close();
        for directive in &directives {
            if matches!(directive, Directive::Stop) {
                self.apply_engine(directive);
            }
        }
        if let (Some(session), Some(report)) = (self.session.clone(), self.model.report_body()) {
            for directive in directives {
                if let Directive::Report(kind) = directive {
                    let session = session.clone();
                    let report = report.clone();
                    let _ = self
                        .runtime
                        .spawn(async move { send_report(session, kind, report).await });
                }
            }
        }
        for task in self.tasks.drain(..) {
            task.abort();
        }
        self.player.take();
    }
}

impl Drop for PlayerScreen {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Render for PlayerScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let now = Instant::now();
        let controls = self.model.controls_visible(now);
        let menu = self.model.menu();
        let title = self.model.title().to_string();
        let context_line = self.model.context().map(str::to_string);
        let method = self.model.method_label().map(str::to_string);
        let status = status_line(&self.model);
        let timeline = self.model.timeline();
        let transport = self.model.transport_enabled();
        let playing = self.model.snapshot().state == matinee_player::PlaybackState::Playing;
        let muted = self.model.muted();
        let volume = self.model.volume();
        let notice = self.model.report_notice().map(str::to_string);
        let audio = self.model.snapshot().audio_tracks.clone();
        let subtitles = self.model.snapshot().subtitle_tracks.clone();
        let selected_audio = self.model.snapshot().selected_audio;
        let selected_subtitle = self.model.snapshot().selected_subtitle;
        let fullscreen = window.is_fullscreen();
        let entity = cx.entity();

        let click = entity.clone();
        let moved = entity.clone();
        let play = entity.clone();
        let back = entity.clone();
        let rewind = entity.clone();
        let forward = entity.clone();
        let scrub = entity.clone();
        let volume_entity = entity.clone();
        let mute = entity.clone();
        let audio_button = entity.clone();
        let subtitle_button = entity.clone();
        let fullscreen_button = entity.clone();
        let dismiss = entity.clone();
        let hover = entity.clone();

        div()
            .id("matinee-player")
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(theme.colors.surface.canvas)
            .track_focus(&self.focus)
            .on_mouse_move(move |_, _, cx| {
                moved.update(cx, |this, cx| this.activity(UserCommand::Activity, cx));
            })
            .child(self.surface.clone())
            .child(div().absolute().inset_0().on_mouse_down(
                MouseButton::Left,
                move |_, window, cx| {
                    click.update(cx, |this, cx| {
                        window.focus(&this.focus);
                        this.activity(UserCommand::TogglePlay, cx);
                    });
                },
            ))
            .children(status.map(|message| status_card(&theme, message)))
            .when(controls, |parent| {
                parent.child(control_bar(
                    &theme,
                    ControlBar {
                        title,
                        context_line,
                        method,
                        timeline,
                        transport,
                        playing,
                        muted,
                        volume,
                        notice,
                        audio,
                        subtitles,
                        selected_audio,
                        selected_subtitle,
                        menu,
                        fullscreen,
                        play,
                        back,
                        rewind,
                        forward,
                        scrub,
                        volume_entity,
                        mute,
                        audio_button,
                        subtitle_button,
                        fullscreen_button,
                        dismiss,
                        hover,
                    },
                ))
            })
    }
}

struct ControlBar {
    title: String,
    context_line: Option<String>,
    method: Option<String>,
    timeline: super::model::Timeline,
    transport: bool,
    playing: bool,
    muted: bool,
    volume: f32,
    notice: Option<String>,
    audio: Vec<matinee_player::Track>,
    subtitles: Vec<matinee_player::Track>,
    selected_audio: Option<matinee_player::TrackId>,
    selected_subtitle: Option<matinee_player::TrackId>,
    menu: Option<MenuKind>,
    fullscreen: bool,
    play: Entity<PlayerScreen>,
    back: Entity<PlayerScreen>,
    rewind: Entity<PlayerScreen>,
    forward: Entity<PlayerScreen>,
    scrub: Entity<PlayerScreen>,
    volume_entity: Entity<PlayerScreen>,
    mute: Entity<PlayerScreen>,
    audio_button: Entity<PlayerScreen>,
    subtitle_button: Entity<PlayerScreen>,
    fullscreen_button: Entity<PlayerScreen>,
    dismiss: Entity<PlayerScreen>,
    hover: Entity<PlayerScreen>,
}

fn control_bar(theme: &Theme, bar: ControlBar) -> impl IntoElement {
    let position = format_clock(bar.timeline.position);
    let duration = bar
        .timeline
        .duration
        .filter(|duration| !duration.is_zero())
        .map(format_clock)
        .unwrap_or_else(|| "—".into());
    let max_ms = bar
        .timeline
        .duration
        .map(|duration| duration.as_millis() as f32)
        .unwrap_or(1.0)
        .max(1.0);
    let value_ms = bar.timeline.position.as_millis() as f32;
    let play_icon = if bar.playing {
        IconName::Pause
    } else {
        IconName::Play
    };
    let play_label = if bar.playing { "Pause" } else { "Play" };
    let mute_label = if bar.muted { "Unmute" } else { "Mute" };
    let fullscreen_label = if bar.fullscreen {
        "Leave full screen"
    } else {
        "Full screen"
    };

    div()
        .id("player-controls")
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .on_hover({
            let hover = bar.hover.clone();
            move |hovered, _, cx| {
                hover.update(cx, |this, cx| {
                    this.model.set_controls_hovered(*hovered);
                    cx.notify();
                });
            }
        })
        .child(
            v_stack(Space::S3)
                .w_full()
                .px(Space::S5.px())
                .pt(Space::S5.px())
                .pb(Space::S4.px())
                .bg(theme.colors.surface.overlay)
                .child(
                    h_stack(Space::S3)
                        .w_full()
                        .items_end()
                        .justify_between()
                        .child(
                            v_stack(Space::S1)
                                .child(Text::new(bar.title).role(TextRole::Heading))
                                .when_some(bar.context_line, |stack, line| {
                                    stack.child(
                                        Text::new(line)
                                            .role(TextRole::Caption)
                                            .tone(TextTone::Muted),
                                    )
                                }),
                        )
                        .when_some(bar.method, |row, method| {
                            row.child(
                                Text::new(method)
                                    .role(TextRole::Caption)
                                    .tone(TextTone::Muted),
                            )
                        }),
                )
                .child(
                    Slider::new("player-timeline", value_ms.min(max_ms))
                        .range(0.0, max_ms.max(1.0))
                        .step(250.0)
                        .fill(true)
                        .disabled(!bar.timeline.enabled || !bar.transport)
                        .on_change({
                            let scrub = bar.scrub.clone();
                            move |next, _, cx| {
                                scrub.update(cx, |this, cx| {
                                    this.activity(
                                        UserCommand::Scrub(Duration::from_millis(
                                            next.max(0.0) as u64
                                        )),
                                        cx,
                                    );
                                });
                            }
                        }),
                )
                .child(
                    h_stack(Space::S3)
                        .w_full()
                        .items_center()
                        .justify_between()
                        .child(Text::new(position).role(TextRole::Metadata))
                        .child(
                            Text::new(duration)
                                .role(TextRole::Metadata)
                                .tone(TextTone::Muted),
                        ),
                )
                .when_some(bar.notice.clone(), |stack, notice| {
                    stack.child(
                        Text::new(notice)
                            .role(TextRole::Caption)
                            .color(theme.colors.text.danger),
                    )
                })
                .child(
                    h_stack(Space::S2)
                        .w_full()
                        .items_center()
                        .child(
                            Button::new("player-back", "Back")
                                .variant(ButtonVariant::Subtle)
                                .size(ButtonSize::Small)
                                .on_click({
                                    let back = bar.back.clone();
                                    move |_, _, cx| {
                                        back.update(cx, |this, cx| {
                                            this.activity(UserCommand::Activity, cx);
                                            cx.emit(LeavePlayer);
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("player-rewind", "−10s")
                                .variant(ButtonVariant::Subtle)
                                .size(ButtonSize::Small)
                                .disabled(!bar.transport || !bar.timeline.enabled)
                                .on_click({
                                    let rewind = bar.rewind.clone();
                                    move |_, _, cx| {
                                        rewind.update(cx, |this, cx| {
                                            this.activity(UserCommand::SeekByMs(-SEEK_STEP_MS), cx);
                                        });
                                    }
                                }),
                        )
                        .child(
                            IconButton::new("player-play", play_icon, play_label)
                                .size(ButtonSize::Medium)
                                .disabled(!bar.transport)
                                .on_click({
                                    let play = bar.play.clone();
                                    move |_, _, cx| {
                                        play.update(cx, |this, cx| {
                                            this.activity(UserCommand::TogglePlay, cx);
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("player-forward", "+10s")
                                .variant(ButtonVariant::Subtle)
                                .size(ButtonSize::Small)
                                .disabled(!bar.transport || !bar.timeline.enabled)
                                .on_click({
                                    let forward = bar.forward.clone();
                                    move |_, _, cx| {
                                        forward.update(cx, |this, cx| {
                                            this.activity(UserCommand::SeekByMs(SEEK_STEP_MS), cx);
                                        });
                                    }
                                }),
                        )
                        .child(div().flex_1())
                        .child(
                            div().w(px(160.0)).child(
                                Slider::new("player-volume", bar.volume)
                                    .range(0.0, 1.0)
                                    .step(VOLUME_STEP)
                                    .fill(true)
                                    .disabled(!bar.transport)
                                    .on_change({
                                        let volume_entity = bar.volume_entity.clone();
                                        move |next, _, cx| {
                                            volume_entity.update(cx, |this, cx| {
                                                this.activity(UserCommand::SetVolume(next), cx);
                                            });
                                        }
                                    }),
                            ),
                        )
                        .child(
                            Button::new("player-mute", mute_label)
                                .variant(ButtonVariant::Subtle)
                                .size(ButtonSize::Small)
                                .disabled(!bar.transport)
                                .on_click({
                                    let mute = bar.mute.clone();
                                    move |_, _, cx| {
                                        mute.update(cx, |this, cx| {
                                            this.activity(UserCommand::ToggleMute, cx);
                                        });
                                    }
                                }),
                        )
                        .child(track_menu(
                            "player-audio",
                            "Audio",
                            MenuKind::Audio,
                            bar.menu == Some(MenuKind::Audio),
                            audio_entries(&bar.audio, bar.selected_audio, bar.audio_button.clone()),
                            bar.audio_button.clone(),
                            bar.dismiss.clone(),
                        ))
                        .child(track_menu(
                            "player-subtitles",
                            "Subtitles",
                            MenuKind::Subtitles,
                            bar.menu == Some(MenuKind::Subtitles),
                            subtitle_entries(
                                &bar.subtitles,
                                bar.selected_subtitle,
                                bar.subtitle_button.clone(),
                            ),
                            bar.subtitle_button,
                            bar.dismiss,
                        ))
                        .child(
                            Button::new("player-fullscreen", fullscreen_label)
                                .variant(ButtonVariant::Subtle)
                                .size(ButtonSize::Small)
                                .on_click({
                                    let fullscreen_button = bar.fullscreen_button;
                                    move |_, window, cx| {
                                        window.toggle_fullscreen();
                                        fullscreen_button.update(cx, |this, cx| {
                                            this.activity(UserCommand::Activity, cx);
                                        });
                                    }
                                }),
                        ),
                ),
        )
}

fn track_menu(
    id: &'static str,
    label: &'static str,
    kind: MenuKind,
    open: bool,
    entries: Vec<MenuEntry>,
    toggle: Entity<PlayerScreen>,
    dismiss: Entity<PlayerScreen>,
) -> impl IntoElement {
    Popover::new(id)
        .open(open)
        .placement(Placement::Top)
        .on_dismiss(move |_, cx| {
            dismiss.update(cx, |this, cx| this.activity(UserCommand::CloseMenu, cx));
        })
        .trigger(
            Button::new(id, label)
                .variant(ButtonVariant::Subtle)
                .size(ButtonSize::Small)
                .on_click(move |_, _, cx| {
                    toggle.update(cx, |this, cx| {
                        let command = if this.model.menu() == Some(kind) {
                            UserCommand::CloseMenu
                        } else {
                            UserCommand::OpenMenu(kind)
                        };
                        this.activity(command, cx);
                    });
                }),
        )
        .menu(entries)
}

fn audio_entries(
    tracks: &[matinee_player::Track],
    selected: Option<matinee_player::TrackId>,
    screen: Entity<PlayerScreen>,
) -> Vec<MenuEntry> {
    tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let id = track.id;
            let screen = screen.clone();
            MenuEntry::Item(
                MenuItem::new(track_label("Audio", index, track))
                    .checked(selected == Some(id))
                    .on_activate(move |_, cx| {
                        screen.update(cx, |this, cx| {
                            this.activity(UserCommand::SelectAudio(id), cx);
                            this.activity(UserCommand::CloseMenu, cx);
                        });
                    }),
            )
        })
        .collect()
}

fn subtitle_entries(
    tracks: &[matinee_player::Track],
    selected: Option<matinee_player::TrackId>,
    screen: Entity<PlayerScreen>,
) -> Vec<MenuEntry> {
    let mut entries = vec![MenuEntry::Item({
        let screen = screen.clone();
        MenuItem::new("Off")
            .checked(selected.is_none())
            .on_activate(move |_, cx| {
                screen.update(cx, |this, cx| {
                    this.activity(UserCommand::SubtitlesOff, cx);
                    this.activity(UserCommand::CloseMenu, cx);
                });
            })
    })];
    entries.extend(tracks.iter().enumerate().map(|(index, track)| {
        let id = track.id;
        let screen = screen.clone();
        MenuEntry::Item(
            MenuItem::new(track_label("Subtitle", index, track))
                .checked(selected == Some(id))
                .on_activate(move |_, cx| {
                    screen.update(cx, |this, cx| {
                        this.activity(UserCommand::SelectSubtitle(id), cx);
                        this.activity(UserCommand::CloseMenu, cx);
                    });
                }),
        )
    }));
    entries
}

fn status_card(theme: &Theme, message: String) -> impl IntoElement {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .pt(px(72.0))
        .flex()
        .justify_center()
        .child(
            div()
                .max_w(px(440.0))
                .p(Space::S4.px())
                .bg(theme.colors.surface.overlay)
                .rounded(px(theme.radius.get(Radius::Medium)))
                .child(Text::new(message).role(TextRole::Body)),
        )
}

fn status_line(model: &PlayerModel) -> Option<String> {
    if let Some(failure) = model.failure() {
        return Some(failure.message.clone());
    }
    match model.playback_state() {
        Some(matinee_player::PlaybackState::Loading)
            if model.stage() == super::model::Stage::Resolving =>
        {
            Some("Checking the best playback format…".into())
        }
        Some(matinee_player::PlaybackState::Loading) => Some("Opening video…".into()),
        Some(matinee_player::PlaybackState::Buffering) => Some("Buffering…".into()),
        Some(matinee_player::PlaybackState::Ended) => Some("Playback finished".into()),
        _ => None,
    }
}

fn stream_headers(
    session: Option<&Session>,
    authorization: StreamAuthorization,
) -> Result<Vec<(String, String)>, PlayerFailure> {
    match authorization {
        StreamAuthorization::None => Ok(Vec::new()),
        StreamAuthorization::Session => {
            let session = session.ok_or_else(|| {
                PlayerFailure::stream("The Jellyfin session is no longer authorized.")
            })?;
            let value = session
                .authorization_header()
                .map_err(|error| PlayerFailure::stream(error.to_string()))?;
            Ok(vec![("Authorization".into(), value)])
        }
    }
}

async fn send_report(
    session: Session,
    kind: ReportKind,
    report: PlaybackReport,
) -> Result<(), String> {
    let transport = ReqwestTransport::new().map_err(|error| error.to_string())?;
    JellyfinClient::new(session, transport)
        .report_playback(kind, &report)
        .await
        .map_err(|error| error.to_string())
}

fn plain_key(event: &KeyDownEvent) -> bool {
    let key = &event.keystroke;
    !key.modifiers.shift && !key.modifiers.control && !key.modifiers.alt && !key.modifiers.platform
}

/// The shell listens for this and drops the player.
pub(crate) struct LeavePlayer;

impl EventEmitter<LeavePlayer> for PlayerScreen {}
