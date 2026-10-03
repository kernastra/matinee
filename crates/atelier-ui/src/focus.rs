//! Keyboard focus versus pointer interaction.
//!
//! Focus rings communicate keyboard navigation. A control that was reached
//! by the pointer still focuses when it must (a text field has to, so it can
//! show a caret), but it does not wear the keyboard focus ring.
//!
//! The modality is a framework global. [`crate::move_focus_forward`] and
//! [`crate::move_focus_backward`] record keyboard navigation. Pointer
//! handlers call [`note_pointer_interaction`]. Components ask
//! [`focus_visible`] before drawing [`crate::FocusRing`].

use gpui::{App, Global, Window};

/// How the user most recently moved through the interface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputModality {
    /// The last interaction was a pointer event.
    #[default]
    Pointer,
    /// The last interaction moved focus from the keyboard.
    Keyboard,
}

#[derive(Clone, Copy, Debug, Default)]
struct InputModalityState {
    modality: InputModality,
}

impl Global for InputModalityState {}

/// The modality currently in effect. Defaults to pointer until a keyboard
/// focus move is recorded, so a window does not open with a focus ring.
pub fn input_modality(cx: &App) -> InputModality {
    cx.try_global::<InputModalityState>()
        .map(|state| state.modality)
        .unwrap_or_default()
}

/// Record that focus is moving because of the keyboard.
pub fn note_keyboard_navigation(cx: &mut App) {
    cx.set_global(InputModalityState {
        modality: InputModality::Keyboard,
    });
}

/// Record a pointer interaction. Keyboard focus rings hide until the next
/// keyboard focus move, even if focus itself does not change.
pub fn note_pointer_interaction(cx: &mut App) {
    cx.set_global(InputModalityState {
        modality: InputModality::Pointer,
    });
}

/// Whether a focused control should draw the keyboard focus ring.
pub fn focus_visible(focused: bool, cx: &App) -> bool {
    focus_ring_for(focused, input_modality(cx))
}

/// Pure form of [`focus_visible`], so the contract is unit-testable.
pub fn focus_ring_for(focused: bool, modality: InputModality) -> bool {
    focused && modality == InputModality::Keyboard
}

/// Move focus to the next tab stop and record keyboard modality.
pub fn move_focus_forward(window: &mut Window, cx: &mut App) {
    note_keyboard_navigation(cx);
    window.focus_next();
}

/// Move focus to the previous tab stop and record keyboard modality.
pub fn move_focus_backward(window: &mut Window, cx: &mut App) {
    note_keyboard_navigation(cx);
    window.focus_prev();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_ring_follows_keyboard_modality_only() {
        assert!(focus_ring_for(true, InputModality::Keyboard));
        assert!(!focus_ring_for(true, InputModality::Pointer));
        assert!(!focus_ring_for(false, InputModality::Keyboard));
        assert!(!focus_ring_for(false, InputModality::Pointer));
    }
}
