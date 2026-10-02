//! `atelier-ui` — the reusable design-system layer (working name "Atelier").
//!
//! Layering rules (see `docs/architecture/ui-framework.md`):
//! - Product concepts never appear here. Applications supply a [`Theme`] that
//!   maps their brand onto the semantic tokens in [`tokens`].
//! - Components own behavior (states, focus, keyboard activation, motion),
//!   not just appearance.
//! - This crate is the compatibility boundary around GPUI. Applications
//!   import from [`prelude`] rather than depending on `gpui` directly.

mod bridge;
pub mod components;
mod editing;
mod focus;
mod fonts;
mod inspect;
pub mod motion;
mod styled_ext;
mod theme;
pub mod tokens;

pub use components::*;
pub use focus::{
    InputModality, focus_ring_for, focus_visible, input_modality, move_focus_backward,
    move_focus_forward, note_keyboard_navigation, note_pointer_interaction,
};
pub use fonts::add_fonts;
pub use inspect::{Inspection, clear_inspection, current_inspection, report_inspection};
pub use styled_ext::StyledExt;
pub use theme::{ActiveTheme, Appearance, Theme, ThemeIssue, UiPreferences};

/// The full GPUI crate, for code that genuinely needs an API the framework
/// does not wrap yet. Every use outside the framework crates is migration
/// debt to be replaced by a framework API.
pub use gpui;

/// Everything an application view needs: framework components and tokens
/// plus the curated subset of GPUI that apps are expected to touch.
pub mod prelude {
    pub use crate::components::*;
    pub use crate::focus::{
        InputModality, focus_visible, input_modality, move_focus_backward, move_focus_forward,
        note_keyboard_navigation, note_pointer_interaction,
    };
    pub use crate::styled_ext::StyledExt;
    pub use crate::theme::{ActiveTheme, Appearance, Theme, UiPreferences};
    pub use crate::tokens::{
        Color, Elevation, MotionDuration, MotionPreference, Radius, Space, Spring, TextRole,
    };

    pub use gpui::prelude::*;
    pub use gpui::{
        AnimationExt, AnyElement, App, ClickEvent, Context, Div, ElementId, Entity, FocusHandle,
        Focusable, IntoElement, MouseButton, Pixels, Render, RenderOnce, SharedString, Stateful,
        Window, div, px,
    };
}
