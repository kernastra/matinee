use atelier_ui::{
    Button, InputModality, TextField, focus_visible, input_modality, move_focus_forward,
};
use gpui::{
    Context, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext, Window, div,
    point, px,
};

struct ButtonHarness;

impl Render for ButtonHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_start()
            .child(Button::new("go", "Go"))
    }
}

struct FieldHarness;

impl Render for FieldHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_start()
            .child(TextField::new("field", ""))
    }
}

#[gpui::test]
fn keyboard_navigation_shows_focus_treatment(cx: &mut TestAppContext) {
    let (_, window) = cx.add_window_view(|_, _| ButtonHarness);
    window.run_until_parked();
    window.update(move_focus_forward);
    window.run_until_parked();
    window.update(|window, cx| {
        assert!(window.focused(cx).is_some());
        assert_eq!(input_modality(cx), InputModality::Keyboard);
        assert!(focus_visible(true, cx));
    });
}

#[gpui::test]
fn pointer_interaction_hides_keyboard_focus_treatment(cx: &mut TestAppContext) {
    let (_, window) = cx.add_window_view(|_, _| ButtonHarness);
    window.run_until_parked();
    window.update(move_focus_forward);
    window.run_until_parked();
    window.simulate_click(point(px(8.0), px(8.0)), Modifiers::none());
    window.run_until_parked();
    window.update(|_window, cx| {
        assert_eq!(input_modality(cx), InputModality::Pointer);
        assert!(!focus_visible(true, cx));
    });
}

#[gpui::test]
fn pointer_focus_on_a_text_field_does_not_force_a_keyboard_ring(cx: &mut TestAppContext) {
    let (_, window) = cx.add_window_view(|_, _| FieldHarness);
    window.run_until_parked();
    window.simulate_click(point(px(20.0), px(14.0)), Modifiers::none());
    window.run_until_parked();
    window.update(|window, cx| {
        assert!(
            window.focused(cx).is_some(),
            "a text field focuses on click"
        );
        assert_eq!(input_modality(cx), InputModality::Pointer);
        assert!(!focus_visible(true, cx));
    });
}
