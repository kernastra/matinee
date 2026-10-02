//! Behavior tests for button-like controls, run on GPUI's headless test
//! platform. These cover interaction rules (activation, focusability), not
//! pixels; visual states are reviewed in the Gallery.

use std::{cell::RefCell, rc::Rc};

use atelier_ui::{Button, IconButton, IconName};
use gpui::{
    ClickEvent, Context, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers,
    ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Source {
    Pointer,
    Keyboard,
}

#[derive(Clone, Copy)]
enum Kind {
    Enabled,
    Disabled,
    Loading,
    Icon,
}

struct Harness {
    kind: Kind,
    activations: Rc<RefCell<Vec<Source>>>,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let log = self.activations.clone();
        let on_click = move |event: &ClickEvent, _: &mut Window, _: &mut gpui::App| {
            log.borrow_mut().push(match event {
                ClickEvent::Mouse(_) => Source::Pointer,
                ClickEvent::Keyboard(_) => Source::Keyboard,
            });
        };
        let control = match self.kind {
            Kind::Enabled => Button::new("subject", "Go")
                .on_click(on_click)
                .into_any_element(),
            Kind::Disabled => Button::new("subject", "Go")
                .disabled(true)
                .on_click(on_click)
                .into_any_element(),
            Kind::Loading => Button::new("subject", "Go")
                .loading(true)
                .on_click(on_click)
                .into_any_element(),
            Kind::Icon => IconButton::new("subject", IconName::Play, "Play")
                .on_click(on_click)
                .into_any_element(),
        };
        // The control sits at the window origin so pointer tests can target it.
        div().size_full().flex().items_start().child(control)
    }
}

fn mount(
    kind: Kind,
    cx: &mut TestAppContext,
) -> (Rc<RefCell<Vec<Source>>>, &mut VisualTestContext) {
    let activations = Rc::new(RefCell::new(Vec::new()));
    let log = activations.clone();
    let (_, window) = cx.add_window_view(move |_, _| Harness {
        kind,
        activations: log,
    });
    window.run_until_parked();
    (activations, window)
}

/// A full key press. `simulate_keystrokes` only sends key-down, but GPUI
/// activates focused controls on key-up, as platforms do.
fn press(cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
}

fn focus_first_tab_stop(cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| {
        window.focus_next();
        window.focused(cx).is_some()
    })
}

#[gpui::test]
fn enter_and_space_activate_a_focused_button(cx: &mut TestAppContext) {
    let (activations, cx) = mount(Kind::Enabled, cx);
    assert!(focus_first_tab_stop(cx), "button should be a tab stop");
    cx.run_until_parked();
    press(cx, "enter");
    press(cx, "space");
    assert_eq!(*activations.borrow(), [Source::Keyboard, Source::Keyboard]);
}

#[gpui::test]
fn icon_buttons_share_keyboard_behavior(cx: &mut TestAppContext) {
    let (activations, cx) = mount(Kind::Icon, cx);
    assert!(focus_first_tab_stop(cx));
    cx.run_until_parked();
    press(cx, "enter");
    assert_eq!(*activations.borrow(), [Source::Keyboard]);
}

#[gpui::test]
fn disabled_buttons_are_skipped_by_focus_and_ignore_input(cx: &mut TestAppContext) {
    let (activations, cx) = mount(Kind::Disabled, cx);
    assert!(
        !focus_first_tab_stop(cx),
        "disabled button must not take focus"
    );
    press(cx, "enter");
    cx.simulate_click(point(px(8.0), px(8.0)), Modifiers::none());
    assert!(activations.borrow().is_empty());
}

#[gpui::test]
fn loading_buttons_keep_focus_but_ignore_activation(cx: &mut TestAppContext) {
    let (activations, cx) = mount(Kind::Loading, cx);
    assert!(focus_first_tab_stop(cx), "loading button stays focusable");
    cx.run_until_parked();
    press(cx, "enter");
    cx.simulate_click(point(px(8.0), px(8.0)), Modifiers::none());
    assert!(activations.borrow().is_empty());
}

#[gpui::test]
fn pointer_activation_does_not_move_keyboard_focus(cx: &mut TestAppContext) {
    let (activations, cx) = mount(Kind::Enabled, cx);
    cx.simulate_click(point(px(8.0), px(8.0)), Modifiers::none());
    assert_eq!(*activations.borrow(), [Source::Pointer]);
    let focused = cx.update(|window, cx| window.focused(cx).is_some());
    assert!(!focused, "a pointer press must not focus the button");
}
