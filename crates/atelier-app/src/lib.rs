//! `atelier-app` — reusable desktop application infrastructure.
//!
//! Boots GPUI with framework globals, opens windows from a platform-neutral
//! [`WindowSpec`], maps commands to platform shortcuts and menus, and
//! coordinates theme and motion changes. Window strategy is documented in
//! `docs/architecture/platform-strategy.md`.

mod app;
mod chrome;
pub mod command;
mod geometry;
pub mod platform;
mod window;

pub use app::{AppInfo, AtelierApp, REDUCED_MOTION_ENV, set_motion_preference, set_theme};
pub use chrome::{
    ChromeIntent, ClientRegionCapabilities, DecorationSource, ResolvedChrome,
    client_region_capabilities, resolve_chrome,
};
pub use command::{Command, Shortcut};
pub use platform::Platform;
pub use window::{
    WindowSpec, on_fullscreen_escape, open_window, titlebar_leading, titlebar_spacer,
};

/// Display name of the framework. Kept in one place so the working name
/// can be changed cheaply.
pub const FRAMEWORK_NAME: &str = "Atelier";
