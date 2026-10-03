use std::borrow::Cow;

use std::rc::Rc;

use atelier_ui::{
    ActiveTheme, ComponentKeymap, Copy, Cut, FocusNext, FocusPrevious, Paste, SelectAll, Theme,
    UiAssets, UiPreferences,
    gpui::{
        self, App, Application, AssetSource, Global, KeyBinding, Menu, MenuItem, OsAction,
        SharedString, SystemMenuType, Window,
    },
    install_component_keybindings, note_keyboard_navigation,
    tokens::MotionPreference,
};

use crate::{
    command::{
        CloseWindow, Command, Hide, HideOthers, Minimize, OpenSettings, Quit, Redo,
        ToggleFullScreen, Undo,
    },
    platform::Platform,
};

/// Identity of the running application.
#[derive(Clone, Debug)]
pub struct AppInfo {
    /// User-visible name (menus, window titles).
    pub name: &'static str,
    /// Reverse-DNS identifier; used as the Wayland/X11 app id on Linux.
    pub app_id: &'static str,
}

impl Global for AppInfo {}

/// Environment override for reduced motion. `1` / `true` forces reduced,
/// `0` / `false` forces full. Unset leaves the OS signal in charge.
/// This is an override, not a substitute for detection.
pub const REDUCED_MOTION_ENV: &str = "ATELIER_REDUCED_MOTION";

/// Builder that boots GPUI with the framework's globals, assets, keymap,
/// menus, and platform lifecycle conventions installed.
pub struct AtelierApp {
    info: AppInfo,
    theme: Theme,
    assets: Vec<Box<dyn AssetSource>>,
}

impl std::fmt::Debug for AtelierApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AtelierApp")
            .field("info", &self.info)
            .field("theme", &self.theme.name)
            .field("assets", &self.assets.len())
            .finish()
    }
}

impl AtelierApp {
    pub fn new(info: AppInfo) -> Self {
        Self {
            info,
            theme: Theme::neutral_dark(),
            assets: vec![Box::new(UiAssets)],
        }
    }

    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Adds an application asset source, consulted after the framework's.
    pub fn assets(mut self, source: impl AssetSource) -> Self {
        self.assets.push(Box::new(source));
        self
    }

    pub fn run(self, on_launch: impl Fn(&mut App) + 'static) {
        let AtelierApp {
            info,
            theme,
            assets,
        } = self;
        let on_launch = Rc::new(on_launch);
        let application = Application::new().with_assets(AppAssets(assets));
        if Platform::current().reopens_when_activated_without_windows() {
            let reopen = Rc::clone(&on_launch);
            application.on_reopen(move |cx| {
                if cx.windows().is_empty() {
                    reopen(cx);
                }
            });
        }
        application.run(move |cx| {
            let platform = Platform::current();
            cx.set_global(theme);
            cx.set_global(UiPreferences {
                system_motion: platform.detect_reduced_motion().map(|reduced| {
                    if reduced {
                        MotionPreference::Reduced
                    } else {
                        MotionPreference::Full
                    }
                }),
                motion_override: motion_override_from_env(),
            });
            install_commands(cx, platform);
            if platform.has_global_menu_bar() {
                cx.set_menus(app_menus(info.name, platform));
            }
            if platform.quits_when_last_window_closes() {
                cx.on_window_closed(|cx| {
                    if cx.windows().is_empty() {
                        cx.quit();
                    }
                })
                .detach();
            }
            cx.set_global(info);
            on_launch(cx);
            cx.activate(true);
        });
    }
}

fn motion_override_from_env() -> Option<MotionPreference> {
    match std::env::var(REDUCED_MOTION_ENV).as_deref() {
        Ok("1") | Ok("true") => Some(MotionPreference::Reduced),
        Ok("0") | Ok("false") => Some(MotionPreference::Full),
        _ => None,
    }
}

fn install_commands(cx: &mut App, platform: Platform) {
    let bindings: Vec<KeyBinding> = Command::ALL
        .iter()
        .filter_map(|command| {
            let keystroke = command.shortcut(platform)?.keystroke(platform);
            Some(key_binding(*command, &keystroke))
        })
        .collect();
    cx.bind_keys(bindings);
    cx.bind_keys([
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrevious, None),
    ]);
    install_component_keybindings(
        cx,
        &ComponentKeymap {
            primary: platform.primary_key(),
            word: platform.word_key(),
            emacs_line_keys: platform.uses_emacs_line_editing(),
            character_palette: platform.has_character_palette(),
        },
    );

    cx.on_action(|_: &FocusNext, cx| {
        note_keyboard_navigation(cx);
        with_active_window(cx, |window| window.focus_next());
    });
    cx.on_action(|_: &FocusPrevious, cx| {
        note_keyboard_navigation(cx);
        with_active_window(cx, |window| window.focus_prev());
    });
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &CloseWindow, cx| {
        with_active_window(cx, |window| window.remove_window());
    });
    cx.on_action(|_: &Minimize, cx| {
        with_active_window(cx, |window| window.minimize_window());
    });
    cx.on_action(|_: &ToggleFullScreen, cx| {
        with_active_window(cx, |window| window.toggle_fullscreen());
    });
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
}

fn key_binding(command: Command, keystroke: &str) -> KeyBinding {
    match command {
        Command::Quit => KeyBinding::new(keystroke, Quit, None),
        Command::CloseWindow => KeyBinding::new(keystroke, CloseWindow, None),
        Command::Minimize => KeyBinding::new(keystroke, Minimize, None),
        Command::ToggleFullScreen => KeyBinding::new(keystroke, ToggleFullScreen, None),
        Command::OpenSettings => KeyBinding::new(keystroke, OpenSettings, None),
        Command::Hide => KeyBinding::new(keystroke, Hide, None),
        Command::HideOthers => KeyBinding::new(keystroke, HideOthers, None),
    }
}

/// Runs `f` against the active window once the current dispatch finishes.
/// App-level action handlers run while GPUI still holds the window that
/// dispatched the keystroke, so updating it synchronously would fail.
fn with_active_window(cx: &mut App, f: impl FnOnce(&mut Window) + 'static) {
    if let Some(handle) = cx.active_window() {
        cx.defer(move |cx| {
            handle.update(cx, |_, window, _| f(window)).ok();
        });
    }
}

fn app_menus(app_name: &str, platform: Platform) -> Vec<Menu> {
    let item = |command: Command| MenuItem::Action {
        name: SharedString::from(command.title(platform, app_name)),
        action: command.action(),
        os_action: None,
    };
    vec![
        Menu {
            name: SharedString::from(app_name.to_string()),
            items: vec![
                item(Command::OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                item(Command::Hide),
                item(Command::HideOthers),
                MenuItem::separator(),
                item(Command::Quit),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Undo", Undo, OsAction::Undo),
                MenuItem::os_action("Redo", Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                item(Command::Minimize),
                item(Command::ToggleFullScreen),
                MenuItem::separator(),
                item(Command::CloseWindow),
            ],
        },
    ]
}

/// Swaps the active theme and repaints every window.
pub fn set_theme(cx: &mut App, theme: Theme) {
    cx.set_global(theme);
    cx.refresh_windows();
}

/// Sets the preview or in-app motion override. The system signal is kept.
pub fn set_motion_preference(cx: &mut App, motion: MotionPreference) {
    let mut preferences = cx.ui_preferences().clone();
    preferences.motion_override = Some(motion);
    cx.set_global(preferences);
    cx.refresh_windows();
}

/// Asset sources consulted in order; the first that knows a path wins.
struct AppAssets(Vec<Box<dyn AssetSource>>);

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        for source in &self.0 {
            if let Some(bytes) = source.load(path)? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut all = Vec::new();
        for source in &self.0 {
            all.extend(source.list(path)?);
        }
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_assets_fall_through_to_later_sources() {
        struct Fixed;
        impl AssetSource for Fixed {
            fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
                Ok((path == "app/logo.svg").then(|| Cow::Borrowed(&b"<svg/>"[..])))
            }
            fn list(&self, _: &str) -> gpui::Result<Vec<SharedString>> {
                Ok(vec!["app/logo.svg".into()])
            }
        }
        let assets = AppAssets(vec![Box::new(UiAssets), Box::new(Fixed)]);
        assert!(assets.load("app/logo.svg").unwrap().is_some());
        assert!(assets.load("atelier/icons/play.svg").unwrap().is_some());
        assert!(assets.load("missing").unwrap().is_none());
    }
}
