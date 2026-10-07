//! Headless text-field behavior. Printable input goes through
//! `Window::dispatch_keystroke`, which drives the input handler. Do not use
//! `simulate_input`: it splits on `""` and then fails to parse an empty key.

use atelier_ui::{
    ComponentKeymap, SearchField, TextField, install_component_keybindings, move_focus_forward,
};
use gpui::{
    ClipboardItem, Context, Entity, FocusHandle, IntoElement, Keystroke, ParentElement, Render,
    SharedString, Styled, TestAppContext, VisualTestContext, Window, div,
};

struct Harness {
    value: SharedString,
    disabled: bool,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.value.clone();
        let disabled = self.disabled;
        let entity = cx.entity();
        div().size_full().flex().items_start().child(
            TextField::new("field", value)
                .disabled(disabled)
                .on_change(move |value, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.value = value;
                        cx.notify();
                    });
                }),
        )
    }
}

fn mount(disabled: bool, cx: &mut TestAppContext) -> (Entity<Harness>, &mut VisualTestContext) {
    cx.update(|cx| {
        install_component_keybindings(
            cx,
            &ComponentKeymap {
                primary: "ctrl",
                word: "ctrl",
                emacs_line_keys: false,
                character_palette: false,
            },
        );
    });
    let (view, window) = cx.add_window_view(move |_, _| Harness {
        value: SharedString::default(),
        disabled,
    });
    window.run_until_parked();
    (view, window)
}

fn focus(cx: &mut VisualTestContext) {
    cx.update(move_focus_forward);
    cx.run_until_parked();
}

fn type_chars(cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| {
        for ch in text.chars() {
            window.dispatch_keystroke(Keystroke::parse(&ch.to_string()).unwrap(), cx);
        }
    });
    cx.run_until_parked();
}

fn value(view: &Entity<Harness>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |this, _| this.value.to_string())
}

#[gpui::test]
fn typing_updates_the_controlled_value(cx: &mut TestAppContext) {
    let (view, cx) = mount(false, cx);
    focus(cx);
    type_chars(cx, "Hi");
    assert_eq!(value(&view, cx), "Hi");
}

#[gpui::test]
fn backspace_deletes_the_previous_character(cx: &mut TestAppContext) {
    let (view, cx) = mount(false, cx);
    focus(cx);
    type_chars(cx, "Hi");
    cx.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse("backspace").unwrap(), cx);
    });
    cx.run_until_parked();
    assert_eq!(value(&view, cx), "H");
}

#[gpui::test]
fn disabled_fields_are_not_tab_stops_and_ignore_typing(cx: &mut TestAppContext) {
    let (view, cx) = mount(true, cx);
    let focused = cx.update(|window, cx| {
        move_focus_forward(window, cx);
        window.focused(cx).is_some()
    });
    assert!(!focused, "disabled field must not take focus");
    type_chars(cx, "Hi");
    assert_eq!(value(&view, cx), "");
}

#[gpui::test]
fn the_field_is_focusable(cx: &mut TestAppContext) {
    let (_view, cx) = mount(false, cx);
    focus(cx);
    let focused = cx.update(|window, cx| window.focused(cx).is_some());
    assert!(focused);
}

struct SecretHarness {
    value: SharedString,
}

impl Render for SecretHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.value.clone();
        let entity = cx.entity();
        div().size_full().flex().items_start().child(
            TextField::new("secret", value)
                .masked(true)
                .on_change(move |value, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.value = value;
                        cx.notify();
                    });
                }),
        )
    }
}

#[gpui::test]
fn masked_entry_keeps_the_value_and_does_not_copy_it(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install_component_keybindings(
            cx,
            &ComponentKeymap {
                primary: "ctrl",
                word: "ctrl",
                emacs_line_keys: false,
                character_palette: false,
            },
        );
    });
    let (view, cx) = cx.add_window_view(|_, _| SecretHarness {
        value: SharedString::default(),
    });
    cx.run_until_parked();
    cx.update(move_focus_forward);
    cx.run_until_parked();
    type_chars(cx, "Hi");
    assert_eq!(view.read_with(cx, |this, _| this.value.to_string()), "Hi");
    cx.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse("ctrl-a").unwrap(), cx);
        window.dispatch_keystroke(Keystroke::parse("ctrl-c").unwrap(), cx);
    });
    cx.run_until_parked();
    let copied = cx.update(|_window, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
    });
    assert!(
        !copied.contains("Hi"),
        "masked copy must not place the value on the clipboard, got {copied:?}"
    );
    cx.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse("ctrl-x").unwrap(), cx);
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |this, _| this.value.to_string()),
        "Hi",
        "cut must not remove a masked value"
    );
    cx.update(|_window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("Yo".to_string()));
    });
    cx.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse("ctrl-v").unwrap(), cx);
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |this, _| this.value.to_string()),
        "Yo",
        "paste must insert into a masked field"
    );
}

/// An owner that holds the field's focus handle, as a search screen does.
struct Owned {
    focus: Option<FocusHandle>,
    value: SharedString,
}

impl Render for Owned {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let mut field =
            SearchField::new("owned-search", self.value.clone()).on_change(move |value, _, cx| {
                entity.update(cx, |this, cx| {
                    this.value = value;
                    cx.notify();
                });
            });
        if let Some(focus) = self.focus.clone() {
            field = field.focus_handle(focus);
        }
        div().size_full().flex().items_start().child(field)
    }
}

#[gpui::test]
fn an_owners_focus_handle_moves_focus_into_the_field(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install_component_keybindings(
            cx,
            &ComponentKeymap {
                primary: "ctrl",
                word: "ctrl",
                emacs_line_keys: false,
                character_palette: false,
            },
        );
    });
    let (view, window) = cx.add_window_view(|_, cx| Owned {
        focus: Some(cx.focus_handle()),
        value: SharedString::default(),
    });
    window.run_until_parked();
    let handle = view.read_with(window, |owned, _| owned.focus.clone().expect("handle"));
    window.update(|window, _| window.focus(&handle));
    window.run_until_parked();
    assert!(
        window.update(|window, _| handle.is_focused(window)),
        "the owner's handle is the field's focus"
    );
    type_chars(window, "ab");
    assert_eq!(
        view.read_with(window, |owned, _| owned.value.to_string()),
        "ab"
    );
}

#[gpui::test]
fn without_a_handle_the_field_keeps_its_own_focus(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install_component_keybindings(
            cx,
            &ComponentKeymap {
                primary: "ctrl",
                word: "ctrl",
                emacs_line_keys: false,
                character_palette: false,
            },
        );
    });
    let (view, window) = cx.add_window_view(|_, _| Owned {
        focus: None,
        value: SharedString::default(),
    });
    window.run_until_parked();
    focus(window);
    type_chars(window, "xy");
    assert_eq!(
        view.read_with(window, |owned, _| owned.value.to_string()),
        "xy"
    );
}
