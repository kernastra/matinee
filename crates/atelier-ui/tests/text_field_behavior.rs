//! Headless text-field behavior. Printable input goes through
//! `Window::dispatch_keystroke`, which drives the input handler. Do not use
//! `simulate_input`: it splits on `""` and then fails to parse an empty key.

use atelier_ui::{ComponentKeymap, TextField, install_component_keybindings, move_focus_forward};
use gpui::{
    Context, Entity, IntoElement, Keystroke, ParentElement, Render, SharedString, Styled,
    TestAppContext, VisualTestContext, Window, div,
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
