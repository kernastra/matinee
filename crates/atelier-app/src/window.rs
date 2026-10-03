//! Opening a window from a [`WindowSpec`], and keeping its normal frame on disk.

use std::path::PathBuf;

use atelier_ui::gpui::{
    App, AppContext, BorrowAppContext, Bounds, Context, Entity, Global, InteractiveElement,
    KeyDownEvent, MouseButton, MouseDownEvent, Render, SharedString, Styled, TitlebarOptions,
    Window, WindowBounds, WindowControlArea, WindowDecorations, WindowHandle, WindowOptions, div,
    point, px, size,
};

use crate::chrome::{ChromeIntent, DecorationSource, ResolvedChrome, resolve_chrome};
use crate::geometry::{
    LogicalRect, PlacementRequest, WindowGeometry, decode_geometry, encode_geometry, geometry_path,
    next_saved_geometry, place,
};
use crate::platform::Platform;

/// Describes a top-level window in platform-neutral terms.
#[derive(Clone, Debug)]
pub struct WindowSpec {
    pub title: SharedString,
    pub size: (f32, f32),
    pub min_size: (f32, f32),
    /// Largest size applied when placing, restoring, and saving a normal frame.
    /// Not a live maximum. The user can resize past it, and the window is not
    /// snapped back.
    pub placement_max: Option<(f32, f32)>,
    pub resizable: bool,
    /// Stable name for this window's saved frame. `None` does not persist.
    pub restoration_key: Option<SharedString>,
    pub chrome: ChromeIntent,
}

impl WindowSpec {
    pub fn new(title: impl Into<SharedString>, size: (f32, f32)) -> Self {
        Self {
            title: title.into(),
            size,
            min_size: (320.0, 240.0),
            placement_max: None,
            resizable: true,
            restoration_key: None,
            chrome: ChromeIntent::PlatformDefault,
        }
    }

    pub fn min_size(mut self, size: (f32, f32)) -> Self {
        self.min_size = size;
        self
    }

    /// Constrains placement, restoration, and the saved normal frame.
    /// Does not install an operating-system maximum and does not snap a
    /// live resize back into range.
    pub fn placement_max(mut self, size: (f32, f32)) -> Self {
        self.placement_max = Some(size);
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    pub fn restoration_key(mut self, key: impl Into<SharedString>) -> Self {
        self.restoration_key = Some(key.into());
        self
    }

    pub fn chrome(mut self, chrome: ChromeIntent) -> Self {
        self.chrome = chrome;
        self
    }
}

/// Opens a window with the platform strategy for `spec.chrome`.
///
/// The returned handle is the caller's root view. A separate entity records
/// geometry; it is not the window root.
pub fn open_window<V: Render + 'static>(
    cx: &mut App,
    spec: WindowSpec,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> atelier_ui::gpui::Result<WindowHandle<V>> {
    let platform = Platform::current();
    let chrome = resolve_chrome(platform, spec.chrome);
    let app_id = cx
        .try_global::<crate::AppInfo>()
        .map(|info| info.app_id.to_string())
        .unwrap_or_default();
    let saved = spec
        .restoration_key
        .as_ref()
        .and_then(|key| load_geometry(&app_id, key));
    let displays = display_rects(cx);
    let placement = place(&PlacementRequest {
        saved,
        default_size: spec.size,
        min_size: spec.min_size,
        placement_max: spec.placement_max,
        displays: displays.clone(),
        restore_origin: platform.restores_window_origin(),
    });
    let normal = if saved.is_some() || displays.is_empty() {
        placement.frame
    } else {
        // No saved frame: let GPUI pick the display, then keep our clamped size.
        let centered = Bounds::centered(
            None,
            size(px(placement.frame.width), px(placement.frame.height)),
            cx,
        );
        LogicalRect {
            x: f32::from(centered.origin.x),
            y: f32::from(centered.origin.y),
            width: placement.frame.width,
            height: placement.frame.height,
        }
    };
    let bounds = Bounds {
        origin: point(px(normal.x), px(normal.y)),
        size: size(px(normal.width), px(normal.height)),
    };
    let window_bounds = if placement.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    };
    let decorations = match chrome.decorations {
        DecorationSource::Server => WindowDecorations::Server,
    };
    let min_size = spec.min_size;
    let placement_max = spec.placement_max;
    let restoration_key = spec.restoration_key.clone();
    let initial = WindowGeometry {
        x: normal.x,
        y: normal.y,
        width: normal.width,
        height: normal.height,
        maximized: placement.maximized,
    };

    cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
            titlebar: Some(TitlebarOptions {
                title: Some(spec.title),
                appears_transparent: chrome.transparent_titlebar,
                traffic_light_position: chrome.traffic_light.map(|(x, y)| point(px(x), px(y))),
            }),
            is_resizable: spec.resizable,
            app_id: (!app_id.is_empty()).then_some(app_id.clone()),
            window_min_size: Some(size(px(min_size.0), px(min_size.1))),
            window_decorations: Some(decorations),
            ..Default::default()
        },
        move |window, cx| {
            retain_session(
                cx,
                window,
                SessionConfig {
                    app_id,
                    restoration_key,
                    min_size,
                    placement_max,
                    saved: initial,
                },
            );
            build(window, cx)
        },
    )
}

/// Escape leaves generic window fullscreen when no focused control consumed it.
/// Attach it with `.on_key_down(on_fullscreen_escape)` on the root element.
/// Dialogs, menus, and search fields handle Escape as an action first, so
/// this does not dismiss them. `Window::on_key_event` cannot run from a
/// view's `render`: GPUI has not pushed a dispatch node yet.
pub fn on_fullscreen_escape(event: &KeyDownEvent, window: &mut Window, cx: &mut App) {
    if event.keystroke.key == "escape" && window.is_fullscreen() {
        window.toggle_fullscreen();
        cx.stop_propagation();
    }
}

/// Empty space beside titlebar content. Not a drag surface unless
/// `client_drag` is set, which GPUI 0.2.2 never sets for the macOS band.
/// Controls must not use this; they keep their own hit targets.
pub fn titlebar_spacer(chrome: ResolvedChrome) -> atelier_ui::gpui::Div {
    titlebar_region(div().flex_1().h_full().min_w(px(8.0)), chrome)
}

/// Leading clearance for native traffic lights. Zero width is omitted by the caller.
pub fn titlebar_leading(width: f32, chrome: ResolvedChrome) -> atelier_ui::gpui::Div {
    let region = div().w(px(width)).h_full().flex_none();
    if width <= 0.0 {
        region
    } else {
        titlebar_region(region, chrome)
    }
}

fn titlebar_region(
    mut region: atelier_ui::gpui::Div,
    chrome: ResolvedChrome,
) -> atelier_ui::gpui::Div {
    if chrome.client_drag {
        // Windows uses the hit-test area. X11 and Wayland use `start_window_move`.
        // macOS supports neither, so `client_drag` stays false there.
        let forward_double_click = chrome.forwards_titlebar_double_click;
        region = region
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    if event.click_count > 1 && forward_double_click {
                        window.titlebar_double_click();
                    } else {
                        window.start_window_move();
                    }
                    cx.stop_propagation();
                },
            );
        return region;
    }
    if chrome.forwards_titlebar_double_click {
        region = region.on_mouse_down(MouseButton::Left, |event: &MouseDownEvent, window, cx| {
            if event.click_count > 1 {
                window.titlebar_double_click();
                cx.stop_propagation();
            }
        });
    }
    region
}

struct SessionConfig {
    app_id: String,
    restoration_key: Option<SharedString>,
    min_size: (f32, f32),
    placement_max: Option<(f32, f32)>,
    saved: WindowGeometry,
}

struct WindowSession {
    handle: atelier_ui::gpui::AnyWindowHandle,
    app_id: String,
    restoration_key: Option<SharedString>,
    min_size: (f32, f32),
    placement_max: Option<(f32, f32)>,
    saved: WindowGeometry,
}

impl WindowSession {
    fn on_bounds(&mut self, window: &Window, displays: &[LogicalRect]) {
        let bounds = window.bounds();
        let observed = LogicalRect {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        };
        if observed.width < 1.0 || observed.height < 1.0 {
            return;
        }
        let next = next_saved_geometry(
            self.saved,
            observed,
            window.is_maximized(),
            window.is_fullscreen(),
            self.min_size,
            self.placement_max,
            displays,
        );
        if next == self.saved || window.is_fullscreen() {
            return;
        }
        self.saved = next;
        let Some(key) = self.restoration_key.as_deref() else {
            return;
        };
        if let Err(error) = store_geometry(&self.app_id, key, &self.saved) {
            eprintln!("failed to store window geometry: {error}");
        }
    }
}

struct LiveSessions {
    sessions: Vec<Entity<WindowSession>>,
}

impl Global for LiveSessions {}

struct SessionJanitor;

impl Global for SessionJanitor {}

fn retain_session(cx: &mut App, window: &mut Window, config: SessionConfig) {
    let handle = window.window_handle();
    let SessionConfig {
        app_id,
        restoration_key,
        min_size,
        placement_max,
        saved,
    } = config;
    let session = cx.new(|cx: &mut Context<WindowSession>| {
        cx.observe_window_bounds(window, |session, window, cx| {
            let displays = display_rects(cx);
            session.on_bounds(window, &displays);
        })
        .detach();
        WindowSession {
            handle,
            app_id,
            restoration_key,
            min_size,
            placement_max,
            saved,
        }
    });
    if cx.try_global::<SessionJanitor>().is_none() {
        cx.set_global(SessionJanitor);
        cx.on_window_closed(prune_sessions).detach();
    }
    if cx.try_global::<LiveSessions>().is_some() {
        cx.update_global(|live: &mut LiveSessions, _cx| {
            live.sessions.push(session);
        });
    } else {
        cx.set_global(LiveSessions {
            sessions: vec![session],
        });
    }
}

fn prune_sessions(cx: &mut App) {
    let Some(live) = cx.try_global::<LiveSessions>() else {
        return;
    };
    let handles: Vec<_> = live
        .sessions
        .iter()
        .map(|session| session.read(cx).handle)
        .collect();
    let alive: Vec<bool> = handles
        .iter()
        .map(|handle| handle.update(cx, |_, _, _| ()).is_ok())
        .collect();
    cx.update_global(|live: &mut LiveSessions, _cx| {
        let mut index = 0;
        live.sessions.retain(|_| {
            let keep = alive[index];
            index += 1;
            keep
        });
    });
}

fn display_rects(cx: &App) -> Vec<LogicalRect> {
    cx.displays()
        .into_iter()
        .map(|display| {
            let bounds = display.bounds();
            LogicalRect {
                x: f32::from(bounds.origin.x),
                y: f32::from(bounds.origin.y),
                width: f32::from(bounds.size.width),
                height: f32::from(bounds.size.height),
            }
        })
        .collect()
}

fn state_file(app_id: &str, key: &str) -> Option<PathBuf> {
    geometry_path(&crate::platform::window_state_root(), app_id, key)
}

fn load_geometry(app_id: &str, key: &str) -> Option<WindowGeometry> {
    let path = state_file(app_id, key)?;
    let text = std::fs::read_to_string(path).ok()?;
    decode_geometry(&text)
}

fn store_geometry(app_id: &str, key: &str, geometry: &WindowGeometry) -> std::io::Result<()> {
    let Some(path) = state_file(app_id, key) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, encode_geometry(geometry))?;
    std::fs::rename(temporary, path)
}
