//! `atelier-app` — reusable desktop application infrastructure.
//!
//! Phase 0 scope is deliberately small: boot GPUI with framework globals,
//! open native-decorated windows, map commands to platform shortcuts and
//! menus, and coordinate theme/motion changes. Anything without a current
//! consumer is documented in `docs/architecture/platform-strategy.md`
//! instead of implemented.

mod app;
pub mod command;
pub mod platform;

pub use app::{
    AppInfo, AtelierApp, REDUCED_MOTION_ENV, WindowSpec, open_window, set_motion_preference,
    set_theme,
};
pub use command::{Command, Shortcut};
pub use platform::Platform;

/// Display name of the framework. Kept in one place so the working name
/// can be changed cheaply.
pub const FRAMEWORK_NAME: &str = "Atelier";
