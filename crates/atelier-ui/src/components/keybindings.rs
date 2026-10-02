//! Key bindings for text editing and adjustable controls.
//!
//! Bindings are scoped to a key context so a slider's arrow keys do not
//! move a text caret, and a text field's arrows do not change a slider.
//! `atelier-app` installs these once, using the platform's modifier keys.

use gpui::{App, KeyBinding, actions};

/// Modifier names in GPUI keystroke syntax. `atelier-app` fills this from
/// platform conventions. This crate does not branch on the operating system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentKeymap {
    /// `"cmd"` on macOS, `"ctrl"` elsewhere. Select-all, copy, cut, paste.
    pub primary: &'static str,
    /// `"alt"` on macOS, `"ctrl"` elsewhere. Word left/right.
    pub word: &'static str,
    /// macOS line editing: Ctrl-A/E and ⌘←/⌘→.
    pub emacs_line_keys: bool,
    /// macOS character palette (Ctrl-⌘-Space).
    pub character_palette: bool,
}

pub const TEXT_FIELD_CONTEXT: &str = "TextField";
pub const SEARCH_FIELD_CONTEXT: &str = "SearchField";
pub const SLIDER_CONTEXT: &str = "Slider";
pub const SEGMENTED_CONTEXT: &str = "SegmentedControl";

actions!(
    atelier_field,
    [
        Backspace,
        Delete,
        MoveLeft,
        MoveRight,
        SelectLeft,
        SelectRight,
        LineStart,
        LineEnd,
        SelectToStart,
        SelectToEnd,
        MoveWordLeft,
        MoveWordRight,
        SelectWordLeft,
        SelectWordRight,
        SelectAll,
        Paste,
        Cut,
        Copy,
        ShowCharacterPalette,
        ClearOrDismiss,
    ]
);

actions!(
    atelier_adjust,
    [
        NudgeLeft,
        NudgeRight,
        NudgeUp,
        NudgeDown,
        NudgeToStart,
        NudgeToEnd,
        NudgePageUp,
        NudgePageDown,
    ]
);

/// Installs text-field, search-field, slider, and segmented-control bindings.
pub fn install_component_keybindings(cx: &mut App, keys: &ComponentKeymap) {
    let text = Some(TEXT_FIELD_CONTEXT);
    let mut bindings = vec![
        KeyBinding::new("backspace", Backspace, text),
        KeyBinding::new("delete", Delete, text),
        KeyBinding::new("left", MoveLeft, text),
        KeyBinding::new("right", MoveRight, text),
        KeyBinding::new("shift-left", SelectLeft, text),
        KeyBinding::new("shift-right", SelectRight, text),
        KeyBinding::new("home", LineStart, text),
        KeyBinding::new("end", LineEnd, text),
        KeyBinding::new("shift-home", SelectToStart, text),
        KeyBinding::new("shift-end", SelectToEnd, text),
        KeyBinding::new(&format!("{}-left", keys.word), MoveWordLeft, text),
        KeyBinding::new(&format!("{}-right", keys.word), MoveWordRight, text),
        KeyBinding::new(&format!("{}-shift-left", keys.word), SelectWordLeft, text),
        KeyBinding::new(&format!("{}-shift-right", keys.word), SelectWordRight, text),
        KeyBinding::new(&format!("{}-a", keys.primary), SelectAll, text),
        KeyBinding::new(&format!("{}-c", keys.primary), Copy, text),
        KeyBinding::new(&format!("{}-x", keys.primary), Cut, text),
        KeyBinding::new(&format!("{}-v", keys.primary), Paste, text),
    ];
    if keys.emacs_line_keys {
        // macOS text fields: Ctrl-A/E and ⌘←/⌘→ move to the line edges.
        bindings.push(KeyBinding::new("ctrl-a", LineStart, text));
        bindings.push(KeyBinding::new("ctrl-e", LineEnd, text));
        bindings.push(KeyBinding::new(
            &format!("{}-left", keys.primary),
            LineStart,
            text,
        ));
        bindings.push(KeyBinding::new(
            &format!("{}-right", keys.primary),
            LineEnd,
            text,
        ));
        bindings.push(KeyBinding::new(
            &format!("{}-shift-left", keys.primary),
            SelectToStart,
            text,
        ));
        bindings.push(KeyBinding::new(
            &format!("{}-shift-right", keys.primary),
            SelectToEnd,
            text,
        ));
    }
    if keys.character_palette {
        bindings.push(KeyBinding::new(
            "ctrl-cmd-space",
            ShowCharacterPalette,
            text,
        ));
    }
    bindings.push(KeyBinding::new(
        "escape",
        ClearOrDismiss,
        Some(SEARCH_FIELD_CONTEXT),
    ));

    for context in [SLIDER_CONTEXT, SEGMENTED_CONTEXT] {
        let context = Some(context);
        bindings.extend([
            KeyBinding::new("left", NudgeLeft, context),
            KeyBinding::new("right", NudgeRight, context),
            KeyBinding::new("up", NudgeUp, context),
            KeyBinding::new("down", NudgeDown, context),
            KeyBinding::new("home", NudgeToStart, context),
            KeyBinding::new("end", NudgeToEnd, context),
        ]);
    }
    let slider = Some(SLIDER_CONTEXT);
    bindings.push(KeyBinding::new("pageup", NudgePageUp, slider));
    bindings.push(KeyBinding::new("pagedown", NudgePageDown, slider));
    cx.bind_keys(bindings);
}
