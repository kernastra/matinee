//! Native window: startup, Login, the authenticated shell, and the Player.
//!
//! The view asks the service runtime to do vault and HTTP work, then applies
//! the result to [`crate::model::AppModel`]. It does not poll HTTP futures.

use std::sync::Arc;

use atelier_app::{
    ChromeIntent, Platform, WindowSpec, on_fullscreen_escape, open_window, resolve_chrome,
    titlebar_leading, titlebar_spacer,
};
use atelier_ui::prelude::*;
use matinee_jellyfin::{ReqwestTransport, authenticate};
use matinee_secrets::{KeyringStore, MemoryStore};
use matinee_ui::palette::FADED_TEAL;
use tokio::task::JoinHandle;

use crate::player::{KeyOutcome, LeavePlayer, PlayerScreen};
use crate::runtime::ServiceRuntime;
use crate::session::{accept_authentication, forget_session, restore_session};
use crate::store::SharedStore;

use crate::model::{
    AppModel, LOGIN_COPY, MIGRATION_NOTE, PLAYER_ENTRY, PLAYER_ENTRY_NOTE, Phase, ReviewScene,
};

#[derive(Clone)]
pub struct Services {
    runtime: Arc<ServiceRuntime>,
    store: SharedStore,
}

impl Services {
    pub fn production() -> std::io::Result<Self> {
        Ok(Self {
            runtime: Arc::new(ServiceRuntime::new()?),
            store: SharedStore::new(KeyringStore::new()),
        })
    }

    pub fn memory() -> std::io::Result<Self> {
        Ok(Self {
            runtime: Arc::new(ServiceRuntime::new()?),
            store: SharedStore::new(MemoryStore::new()),
        })
    }

    pub fn runtime(&self) -> Arc<ServiceRuntime> {
        Arc::clone(&self.runtime)
    }
}

pub struct MatineeRoot {
    services: Services,
    model: AppModel,
    focus: FocusHandle,
    task: Option<JoinHandle<()>>,
    focus_username: bool,
    /// After sign-in starts, move focus onto the loading button once the
    /// fields have left the tab order.
    park_focus: bool,
    player: Option<Entity<PlayerScreen>>,
    focus_player: bool,
}

impl MatineeRoot {
    fn new(
        services: Services,
        review: Option<ReviewScene>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let model = review
            .map(AppModel::review)
            .unwrap_or_else(AppModel::starting);
        let player = review.and_then(ReviewScene::player_preview).map(|scene| {
            cx.new(|cx| PlayerScreen::preview(Arc::clone(&services.runtime), scene, window, cx))
        });
        let mut root = Self {
            services,
            focus_username: review == Some(ReviewScene::Focus),
            focus_player: player.is_some(),
            model,
            focus: cx.focus_handle(),
            task: None,
            park_focus: false,
            player,
        };
        if review.is_none() {
            root.start_restore(cx);
        }
        if let Some(player) = root.player.clone() {
            root.watch_player(&player, cx);
        }
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            // Close the Player before the window goes, so its stop report does
            // not depend on drop order. The window still closes.
            weak.update(cx, |root, cx| root.release_player(cx)).ok();
            true
        });
        cx.on_app_quit(|root, cx| {
            root.prepare_exit(cx);
            async {}
        })
        .detach();
        root
    }

    fn replace_task(&mut self, task: JoinHandle<()>) {
        if let Some(previous) = self.task.replace(task) {
            previous.abort();
        }
    }

    fn start_restore(&mut self, cx: &mut Context<Self>) {
        let store = self.services.store.clone();
        let (task, rx) = self.services.runtime.spawn(async move {
            tokio::task::spawn_blocking(move || restore_session(&store))
                .await
                .unwrap_or(crate::session::Startup::VaultUnavailable)
        });
        self.replace_task(task);
        cx.spawn(async move |this, cx| {
            let startup = rx
                .await
                .unwrap_or(crate::session::Startup::VaultUnavailable);
            this.update(cx, |this, cx| {
                this.task = None;
                this.model.apply_startup(startup);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn start_sign_in(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.model.begin_sign_in() else {
            cx.notify();
            return;
        };
        let store = self.services.store.clone();
        let (server, username, password) = request.into_parts();
        let (task, rx) = self.services.runtime.spawn(async move {
            match ReqwestTransport::new() {
                Ok(transport) => {
                    let result = authenticate(&transport, &server, &username, password).await;
                    accept_authentication(&store, result)
                }
                Err(error) => Err(error.to_string()),
            }
        });
        self.replace_task(task);
        self.park_focus = true;
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err("The request was cancelled.".into()));
            this.update(cx, |this, cx| {
                this.task = None;
                this.model.apply_sign_in(outcome);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn start_sign_out(&mut self, cx: &mut Context<Self>) {
        if !self.model.begin_sign_out() {
            return;
        }
        self.release_player(cx);
        let store = self.services.store.clone();
        let (task, rx) = self.services.runtime.spawn(async move {
            tokio::task::spawn_blocking(move || forget_session(&store))
                .await
                .unwrap_or_else(|_| {
                    Err("The credential vault could not complete the request.".into())
                })
        });
        self.replace_task(task);
        cx.spawn(async move |this, cx| {
            let outcome = rx.await.unwrap_or_else(|_| {
                Err("The credential vault could not complete the request.".into())
            });
            this.update(cx, |this, cx| {
                this.task = None;
                this.model.apply_sign_out(outcome);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn open_player(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item_id) = self.model.begin_playback() else {
            cx.notify();
            return;
        };
        let Some(session) = self.model.session().cloned() else {
            return;
        };
        let player = cx.new(|cx| {
            PlayerScreen::open(
                Arc::clone(&self.services.runtime),
                session,
                item_id,
                window,
                cx,
            )
        });
        self.watch_player(&player, cx);
        window.focus(player.read(cx).focus_handle());
        self.player = Some(player);
        cx.notify();
    }

    fn watch_player(&self, player: &Entity<PlayerScreen>, cx: &mut Context<Self>) {
        cx.subscribe(player, |this, _, _: &LeavePlayer, cx| {
            this.release_player(cx);
            cx.notify();
        })
        .detach();
    }

    /// Player → shell, and the first step of window close and application
    /// exit. The final stop report is started here and not awaited: the
    /// service runtime stays alive. Only [`Self::prepare_exit`] waits for it.
    fn release_player(&mut self, cx: &mut Context<Self>) {
        if let Some(player) = self.player.take() {
            player.update(cx, |player, _| player.finish());
        }
    }

    /// Orderly application exit. GPUI calls this from its quit handlers,
    /// before it drops windows. The Player is finished explicitly, then the
    /// GPUI thread waits for final reports, bounded by
    /// [`crate::runtime::FINAL_WORK_BOUND`]. Exit continues either way.
    fn prepare_exit(&mut self, cx: &mut Context<Self>) {
        self.release_player(cx);
        self.services.runtime.drain_final();
    }
}

impl Drop for MatineeRoot {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Render for MatineeRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_username {
            self.focus_username = false;
            window.on_next_frame(|window, _| {
                window.focus_next();
                window.focus_next();
            });
        }
        if self.focus_player {
            self.focus_player = false;
            let menu_open = self
                .player
                .as_ref()
                .is_some_and(|player| player.read(cx).menu_open());
            if !menu_open && let Some(player) = self.player.clone() {
                let focus = player.read(cx).focus_handle().clone();
                window.on_next_frame(move |window, _| {
                    window.focus(&focus);
                });
            }
        }

        let theme = cx.theme().clone();
        let chrome = resolve_chrome(Platform::current(), ChromeIntent::PlatformDefault);
        let signing_in = self.model.fields_locked();
        let playing = self.player.is_some();
        let fullscreen = window.is_fullscreen();
        if signing_in && self.park_focus {
            self.park_focus = false;
            // This frame's tab order no longer includes the fields. A focus
            // id that just disappeared resolves to the first stop (the
            // window), and the next stop is the loading button.
            window.on_next_frame(|window, _| {
                window.focus_next();
                window.focus_next();
            });
        }

        div()
            .id("matinee-root")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.colors.surface.canvas)
            .font_family(theme.typography.families.interface)
            .text_color(theme.colors.text.primary)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event, window, cx| {
                if this.player.is_some() {
                    let outcome = this
                        .player
                        .as_ref()
                        .map(|player| {
                            player.update(cx, |player, cx| player.on_key(event, window, cx))
                        })
                        .unwrap_or(KeyOutcome::Ignored);
                    if matches!(outcome, KeyOutcome::Leave) {
                        this.release_player(cx);
                        cx.notify();
                    }
                    return;
                }
                if plain_enter(event) && this.model.shows_login() {
                    this.start_sign_in(cx);
                }
                on_fullscreen_escape(event, window, cx);
            }))
            .when(!playing, |root| root.child(atmosphere(&theme)))
            .child(
                v_stack(Space::S0)
                    .size_full()
                    .when(
                        chrome.band_height > 0.0 && !(playing && fullscreen),
                        |column| column.child(titlebar(&theme, chrome)),
                    )
                    .child(if playing {
                        div()
                            .id("matinee-player-slot")
                            .flex_1()
                            .w_full()
                            .min_h(px(0.0))
                            .overflow_hidden()
                            .child(self.player.clone().unwrap())
                            .into_any_element()
                    } else {
                        ScrollView::vertical("matinee-scroll")
                            .flex_1()
                            .w_full()
                            .min_h(px(0.0))
                            .child(self.body(window, chrome.band_height, signing_in, cx))
                            .into_any_element()
                    }),
            )
    }
}

impl MatineeRoot {
    fn body(
        &mut self,
        window: &mut Window,
        band: f32,
        signing_in: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let viewport = window.viewport_size();
        let width = f32::from(viewport.width);
        let height = (f32::from(viewport.height) - band).max(0.0);
        let card = card_width(width);

        div()
            .w_full()
            .min_h(px(height))
            .flex()
            .items_center()
            .justify_center()
            .p(Space::S6.px())
            .child(if self.model.phase() == Phase::Starting {
                starting_mark().into_any_element()
            } else if self.model.shows_shell() {
                self.shell(cx).into_any_element()
            } else {
                self.login(card, signing_in, cx).into_any_element()
            })
    }

    fn login(&mut self, card: f32, signing_in: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let server = self.model.server().to_string();
        let username = self.model.username().to_string();
        let password = self.model.password().to_string();
        let warning = self.model.http_warning();
        let notice = self.model.notice().map(str::to_string);
        let label = self.model.button_label();
        let entity = cx.entity();

        Surface::new(SurfaceLevel::Elevated)
            .padding(Space::S6)
            .w(px(card))
            .child(
                v_stack(Space::S4)
                    .w_full()
                    .child(
                        Text::new("Your library, reimagined")
                            .role(TextRole::Caption)
                            .color(theme.colors.control.accent),
                    )
                    .child(Text::new("Movie night starts here.").role(TextRole::Title))
                    .child(
                        Text::new(LOGIN_COPY)
                            .role(TextRole::Body)
                            .tone(TextTone::Muted),
                    )
                    .child(
                        v_stack(Space::S3)
                            .w_full()
                            .child(
                                TextField::new("server", server)
                                    .fill()
                                    .disabled(signing_in)
                                    .label("Jellyfin server")
                                    .placeholder("http://jellyfin.local:8096")
                                    .on_change({
                                        let entity = entity.clone();
                                        move |value, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.model.set_server(value.to_string());
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .when_some(warning, |stack, warning| {
                                stack.child(
                                    Text::new(warning)
                                        .role(TextRole::Caption)
                                        .color(theme.colors.control.accent),
                                )
                            })
                            .child(
                                TextField::new("username", username)
                                    .fill()
                                    .disabled(signing_in)
                                    .label("Username")
                                    .on_change({
                                        let entity = entity.clone();
                                        move |value, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.model.set_username(value.to_string());
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .child(
                                TextField::new("password", password)
                                    .fill()
                                    .masked(true)
                                    .disabled(signing_in)
                                    .label("Password")
                                    .on_change({
                                        let entity = entity.clone();
                                        move |value, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.model.set_password(value.to_string());
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .when_some(notice, |stack, notice| {
                                stack.child(
                                    Text::new(notice)
                                        .role(TextRole::Caption)
                                        .color(theme.colors.text.danger),
                                )
                            })
                            .child(
                                Button::new("enter-matinee", label)
                                    .variant(ButtonVariant::Primary)
                                    .size(ButtonSize::Large)
                                    .fill()
                                    .loading(signing_in)
                                    .show_label_while_loading(true)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.start_sign_in(cx);
                                    })),
                            ),
                    ),
            )
    }

    fn shell(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let identity = self.model.identity();
        let username = identity
            .as_ref()
            .map(|identity| format!("Connected as {}", identity.username))
            .unwrap_or_else(|| "Connected".into());
        let server = identity
            .as_ref()
            .map(|identity| identity.server.clone())
            .unwrap_or_default();
        let notice = self.model.notice().map(str::to_string);
        let signing_out = self.model.phase() == Phase::SigningOut;
        let label = self.model.button_label();
        let item_id = self.model.item_id().to_string();
        let entity = cx.entity();

        Surface::new(SurfaceLevel::Elevated)
            .padding(Space::S8)
            .w(px(460.0))
            .child(
                v_stack(Space::S4)
                    .w_full()
                    .child(Text::new("Matinee").role(TextRole::Title))
                    .child(Text::new(username).role(TextRole::Body))
                    .child(
                        Text::new(server)
                            .role(TextRole::Metadata)
                            .tone(TextTone::Muted),
                    )
                    .child(
                        Text::new(MIGRATION_NOTE)
                            .role(TextRole::Body)
                            .tone(TextTone::Muted),
                    )
                    .child(
                        v_stack(Space::S2)
                            .w_full()
                            .child(
                                Text::new(PLAYER_ENTRY)
                                    .role(TextRole::Label)
                                    .color(theme.colors.control.accent),
                            )
                            .child(
                                Text::new(PLAYER_ENTRY_NOTE)
                                    .role(TextRole::Caption)
                                    .tone(TextTone::Muted),
                            )
                            .child(
                                TextField::new("item-id", item_id)
                                    .fill()
                                    .disabled(signing_out)
                                    .label("Item ID")
                                    .on_change({
                                        let entity = entity.clone();
                                        move |value, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.model.set_item_id(value.to_string());
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("play-item", "Play")
                                    .variant(ButtonVariant::Primary)
                                    .disabled(signing_out)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_player(window, cx);
                                    })),
                            ),
                    )
                    .when_some(notice, |stack, notice| {
                        stack.child(
                            Text::new(notice)
                                .role(TextRole::Caption)
                                .color(theme.colors.text.danger),
                        )
                    })
                    .child(
                        Button::new("sign-out", label)
                            .variant(ButtonVariant::Secondary)
                            .loading(signing_out)
                            .show_label_while_loading(true)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.start_sign_out(cx);
                            })),
                    ),
            )
    }
}

fn starting_mark() -> impl IntoElement {
    v_stack(Space::S2).child(Text::new("Matinee").role(TextRole::Title))
}

fn atmosphere(theme: &Theme) -> impl IntoElement {
    let full = theme.radius.get(Radius::Full);
    div()
        .absolute()
        .inset_0()
        .child(
            div()
                .absolute()
                .top(px(-180.0))
                .right(px(-140.0))
                .size(px(520.0))
                .rounded(px(full))
                .bg(theme.colors.control.accent.with_alpha(0.14)),
        )
        .child(
            div()
                .absolute()
                .bottom(px(-160.0))
                .left(px(-120.0))
                .size(px(460.0))
                .rounded(px(full))
                .bg(FADED_TEAL.with_alpha(0.18)),
        )
        .child(
            div()
                .absolute()
                .bottom(px(48.0))
                .right(px(72.0))
                .size(px(240.0))
                .rounded(px(full))
                .bg(theme.colors.control.destructive.with_alpha(0.12)),
        )
}

fn titlebar(theme: &Theme, chrome: atelier_app::ResolvedChrome) -> impl IntoElement {
    let row_height = chrome.band_height;
    h_stack(Space::S3)
        .w_full()
        .h(px(row_height))
        .flex_none()
        .items_center()
        .px(Space::S4.px())
        .when(chrome.leading_inset > 0.0, |row| {
            row.child(titlebar_leading(chrome.leading_inset, chrome))
        })
        .child(Text::new("Matinee").role(TextRole::Heading))
        .child(titlebar_spacer(chrome))
        .child(div().flex_1())
        .text_color(theme.colors.text.primary)
}

fn card_width(viewport_width: f32) -> f32 {
    (viewport_width - 64.0).clamp(280.0, 420.0)
}

fn plain_enter(event: &atelier_ui::gpui::KeyDownEvent) -> bool {
    let key = &event.keystroke;
    key.key == "enter"
        && !key.modifiers.shift
        && !key.modifiers.control
        && !key.modifiers.alt
        && !key.modifiers.platform
}

pub fn open_matinee(
    cx: &mut App,
    services: Services,
    review: Option<ReviewScene>,
    size: (f32, f32),
) {
    open_window(
        cx,
        WindowSpec::new("Matinee", size)
            .min_size((960.0, 620.0))
            .restoration_key("matinee"),
        move |window, cx| {
            cx.new(|cx| {
                let root = MatineeRoot::new(services, review, window, cx);
                window.focus(&root.focus);
                root
            })
        },
    )
    .expect("failed to open window");
}
