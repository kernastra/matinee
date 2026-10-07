//! Behavior tests for `Pressable` on GPUI's headless test platform.

use std::{cell::RefCell, rc::Rc};

use atelier_ui::Pressable;
use gpui::{
    ClickEvent, Context, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers,
    ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Source {
    Pointer,
    Keyboard,
}

struct Harness {
    disabled: bool,
    presses: Rc<RefCell<Vec<Source>>>,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let log = self.presses.clone();
        div().size_full().flex().items_start().child(
            Pressable::new("row", "Episode 1")
                .w(px(160.0))
                .h(px(48.0))
                .disabled(self.disabled)
                .on_press(move |event, _, _| {
                    log.borrow_mut().push(match event {
                        ClickEvent::Mouse(_) => Source::Pointer,
                        ClickEvent::Keyboard(_) => Source::Keyboard,
                    });
                })
                .child(div().child("Episode 1")),
        )
    }
}

fn mount(
    disabled: bool,
    cx: &mut TestAppContext,
) -> (Rc<RefCell<Vec<Source>>>, &mut VisualTestContext) {
    let presses = Rc::new(RefCell::new(Vec::new()));
    let log = presses.clone();
    let (_, window) = cx.add_window_view(move |_, _| Harness {
        disabled,
        presses: log,
    });
    window.run_until_parked();
    (presses, window)
}

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
fn a_pressable_is_a_tab_stop_that_enter_and_space_activate(cx: &mut TestAppContext) {
    let (presses, cx) = mount(false, cx);
    assert!(focus_first_tab_stop(cx));
    cx.run_until_parked();
    press(cx, "enter");
    press(cx, "space");
    assert_eq!(*presses.borrow(), [Source::Keyboard, Source::Keyboard]);
}

#[gpui::test]
fn a_pointer_press_activates_without_taking_focus(cx: &mut TestAppContext) {
    let (presses, cx) = mount(false, cx);
    cx.simulate_click(point(px(8.0), px(8.0)), Modifiers::none());
    assert_eq!(*presses.borrow(), [Source::Pointer]);
    assert!(!cx.update(|window, cx| window.focused(cx).is_some()));
}

#[gpui::test]
fn a_disabled_pressable_is_skipped_and_inert(cx: &mut TestAppContext) {
    let (presses, cx) = mount(true, cx);
    assert!(!focus_first_tab_stop(cx));
    press(cx, "enter");
    cx.simulate_click(point(px(8.0), px(8.0)), Modifiers::none());
    assert!(presses.borrow().is_empty());
}
