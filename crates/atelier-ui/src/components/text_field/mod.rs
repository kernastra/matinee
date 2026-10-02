//! Single-line text field.
//!
//! Editing goes through GPUI's input handler (`EntityInputHandler` +
//! `Window::handle_input`), so printable input and IME composition use the
//! platform path. Editing commands are key bindings in the `TextField`
//! context (see `keybindings`).
//!
//! The field is controlled: `value` is the source of truth whenever the
//! field is not composing. `on_change` must write the new value back or the
//! next frame restores the previous one.
//!
//! Component metrics: height 30 (aligned with medium buttons), default width
//! 280, 1px border, 1px caret. A focused field uses a 1px `focus.ring`
//! border and a caret. It does not draw [`crate::components::FocusRing`]; that ring is reserved
//! for keyboard focus on controls that have no caret.

mod input;
mod layout;
mod render;
mod state;

use std::rc::Rc;

use gpui::{App, ElementId, SharedString, Window};

use crate::components::{IconName, keybindings::TEXT_FIELD_CONTEXT};

use state::{ChangeHandler, FocusHandler, TrailingHandler};

/// A single-line text field.
#[derive(gpui::IntoElement)]
pub struct TextField {
    pub(in crate::components::text_field) id: ElementId,
    pub(in crate::components::text_field) value: SharedString,
    pub(in crate::components::text_field) placeholder: SharedString,
    pub(in crate::components::text_field) label: Option<SharedString>,
    pub(in crate::components::text_field) supporting_text: Option<SharedString>,
    pub(in crate::components::text_field) invalid: bool,
    pub(in crate::components::text_field) disabled: bool,
    pub(in crate::components::text_field) leading_icon: Option<IconName>,
    pub(in crate::components::text_field) trailing_icon: Option<IconName>,
    pub(in crate::components::text_field) trailing_label: Option<SharedString>,
    pub(in crate::components::text_field) trailing_action: Option<TrailingHandler>,
    pub(in crate::components::text_field) on_change: Option<ChangeHandler>,
    pub(in crate::components::text_field) on_focus_change: Option<FocusHandler>,
    pub(in crate::components::text_field) inspected: bool,
    pub(in crate::components::text_field) inspection_name: &'static str,
    pub(in crate::components::text_field) key_context: &'static str,
}

impl TextField {
    pub fn new(id: impl Into<ElementId>, value: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            value: value.into(),
            placeholder: SharedString::default(),
            label: None,
            supporting_text: None,
            invalid: false,
            disabled: false,
            leading_icon: None,
            trailing_icon: None,
            trailing_label: None,
            trailing_action: None,
            on_change: None,
            on_focus_change: None,
            inspected: false,
            inspection_name: "Text field",
            key_context: TEXT_FIELD_CONTEXT,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn supporting_text(mut self, text: impl Into<SharedString>) -> Self {
        self.supporting_text = Some(text.into());
        self
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn leading_icon(mut self, icon: IconName) -> Self {
        self.leading_icon = Some(icon);
        self
    }

    /// A pointer-only trailing icon. It is not a tab stop and does not take
    /// keyboard focus away from the field.
    pub fn trailing(
        mut self,
        icon: IconName,
        label: impl Into<SharedString>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.trailing_icon = Some(icon);
        self.trailing_label = Some(label.into());
        self.trailing_action = Some(Rc::new(action));
        self
    }

    pub fn on_change(
        mut self,
        handler: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    pub fn on_focus_change(mut self, handler: impl Fn(bool, &mut App) + 'static) -> Self {
        self.on_focus_change = Some(Rc::new(handler));
        self
    }

    /// Report this field to the Gallery inspector.
    pub fn inspected(mut self, inspected: bool) -> Self {
        self.inspected = inspected;
        self
    }

    pub fn inspection_name(mut self, name: &'static str) -> Self {
        self.inspection_name = name;
        self
    }

    pub(crate) fn key_context(mut self, context: &'static str) -> Self {
        self.key_context = context;
        self
    }
}
