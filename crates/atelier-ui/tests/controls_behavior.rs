use atelier_ui::{
    Checkbox, CheckboxState, ComponentKeymap, SearchField, Segment, SegmentedControl, Slider,
    Switch, install_component_keybindings, move_focus_forward,
};
use gpui::{
    Context, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement, Render,
    SharedString, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

fn install(cx: &mut TestAppContext) {
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
}

fn press(cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    cx.run_until_parked();
}

fn focus(cx: &mut VisualTestContext) {
    cx.update(move_focus_forward);
    cx.run_until_parked();
}

struct SwitchHarness {
    on: bool,
    disabled: bool,
}

impl Render for SwitchHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on = self.on;
        let disabled = self.disabled;
        let entity = cx.entity();
        div().size_full().flex().items_start().child(
            Switch::new("switch", on)
                .disabled(disabled)
                .on_change(move |next, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.on = next;
                        cx.notify();
                    });
                }),
        )
    }
}

#[gpui::test]
fn switch_pointer_and_space(cx: &mut TestAppContext) {
    install(cx);
    let (view, window) = cx.add_window_view(|_, _| SwitchHarness {
        on: false,
        disabled: false,
    });
    window.run_until_parked();
    window.simulate_click(point(px(8.0), px(8.0)), Modifiers::none());
    window.run_until_parked();
    assert!(view.read_with(window, |view, _| view.on));
    focus(window);
    press(window, "space");
    assert!(!view.read_with(window, |view, _| view.on));
}

#[gpui::test]
fn disabled_switch_ignores_pointer_and_keyboard(cx: &mut TestAppContext) {
    install(cx);
    let (view, window) = cx.add_window_view(|_, _| SwitchHarness {
        on: false,
        disabled: true,
    });
    window.run_until_parked();
    window.simulate_click(point(px(8.0), px(8.0)), Modifiers::none());
    let focused = window.update(|window, cx| {
        move_focus_forward(window, cx);
        window.focused(cx).is_some()
    });
    assert!(!focused);
    press(window, "space");
    assert!(!view.read_with(window, |view, _| view.on));
}

struct CheckHarness {
    state: CheckboxState,
}

impl Render for CheckHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state;
        let entity = cx.entity();
        div()
            .size_full()
            .child(Checkbox::new("box", state).on_change(move |next, _, cx| {
                entity.update(cx, |this, cx| {
                    this.state = next;
                    cx.notify();
                });
            }))
    }
}

#[gpui::test]
fn checkbox_keyboard_resolves_indeterminate(cx: &mut TestAppContext) {
    install(cx);
    let (view, window) = cx.add_window_view(|_, _| CheckHarness {
        state: CheckboxState::Indeterminate,
    });
    window.run_until_parked();
    focus(window);
    press(window, "space");
    assert_eq!(
        view.read_with(window, |view, _| view.state),
        CheckboxState::Checked
    );
    press(window, "space");
    assert_eq!(
        view.read_with(window, |view, _| view.state),
        CheckboxState::Unchecked
    );
}

struct SegmentHarness {
    selected: usize,
}

impl Render for SegmentHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected;
        let entity = cx.entity();
        div().size_full().child(
            SegmentedControl::new(
                "segments",
                vec![
                    Segment::new("A"),
                    Segment::new("B").disabled(true),
                    Segment::new("C"),
                ],
                selected,
            )
            .on_change(move |index, _, cx| {
                entity.update(cx, |this, cx| {
                    this.selected = index;
                    cx.notify();
                });
            }),
        )
    }
}

#[gpui::test]
fn segmented_arrows_skip_disabled_segments(cx: &mut TestAppContext) {
    install(cx);
    let (view, window) = cx.add_window_view(|_, _| SegmentHarness { selected: 0 });
    window.run_until_parked();
    focus(window);
    window.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse("right").unwrap(), cx);
    });
    window.run_until_parked();
    assert_eq!(view.read_with(window, |view, _| view.selected), 2);
}

struct SliderHarness {
    value: f32,
    disabled: bool,
}

impl Render for SliderHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.value;
        let disabled = self.disabled;
        let entity = cx.entity();
        div().size_full().child(
            Slider::new("slider", value)
                .range(0.0, 100.0)
                .step(5.0)
                .disabled(disabled)
                .on_change(move |next, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.value = next;
                        cx.notify();
                    });
                }),
        )
    }
}

#[gpui::test]
fn slider_keyboard_steps_and_clamps(cx: &mut TestAppContext) {
    install(cx);
    let (view, window) = cx.add_window_view(|_, _| SliderHarness {
        value: 0.0,
        disabled: false,
    });
    window.run_until_parked();
    focus(window);
    window.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse("right").unwrap(), cx);
        window.dispatch_keystroke(Keystroke::parse("left").unwrap(), cx);
        window.dispatch_keystroke(Keystroke::parse("left").unwrap(), cx);
    });
    window.run_until_parked();
    let value = view.read_with(window, |view, _| view.value);
    assert!((value - 0.0).abs() < f32::EPSILON, "{value}");
    window.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse("end").unwrap(), cx);
    });
    window.run_until_parked();
    let value = view.read_with(window, |view, _| view.value);
    assert!((value - 100.0).abs() < f32::EPSILON, "{value}");
}

#[gpui::test]
fn disabled_slider_does_not_take_focus(cx: &mut TestAppContext) {
    install(cx);
    let (view, window) = cx.add_window_view(|_, _| SliderHarness {
        value: 10.0,
        disabled: true,
    });
    window.run_until_parked();
    let focused = window.update(|window, cx| {
        move_focus_forward(window, cx);
        window.focused(cx).is_some()
    });
    assert!(!focused);
    window.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse("right").unwrap(), cx);
    });
    let value = view.read_with(window, |view, _| view.value);
    assert!((value - 10.0).abs() < f32::EPSILON);
}

struct SearchHarness {
    value: SharedString,
    dismissed: bool,
}

impl Render for SearchHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.value.clone();
        let entity = cx.entity();
        div().size_full().child(
            SearchField::new("search", value)
                .on_change({
                    let entity = entity.clone();
                    move |value, _, cx| {
                        entity.update(cx, |this, cx| {
                            this.value = value;
                            cx.notify();
                        });
                    }
                })
                .on_dismiss(move |_, cx| {
                    entity.update(cx, |this, cx| {
                        this.dismissed = true;
                        cx.notify();
                    });
                }),
        )
    }
}

#[gpui::test]
fn search_escape_clears_then_dismisses(cx: &mut TestAppContext) {
    install(cx);
    let (view, window) = cx.add_window_view(|_, _| SearchHarness {
        value: "query".into(),
        dismissed: false,
    });
    window.run_until_parked();
    focus(window);
    window.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse("escape").unwrap(), cx);
    });
    window.run_until_parked();
    assert_eq!(view.read_with(window, |view, _| view.value.to_string()), "");
    assert!(!view.read_with(window, |view, _| view.dismissed));
    window.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse("escape").unwrap(), cx);
    });
    window.run_until_parked();
    assert!(view.read_with(window, |view, _| view.dismissed));
}
