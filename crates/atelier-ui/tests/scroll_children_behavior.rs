//! Headless tests for `ScrollView` children, `ScrollControl::reveal_child`,
//! and how a vertical page and a horizontal `Rail` share the mouse wheel.

use atelier_ui::{
    ComponentKeymap, Pressable, Rail, RailState, ScrollControl, ScrollView,
    install_component_keybindings, note_keyboard_navigation,
};
use gpui::{
    Context, FocusHandle, IntoElement, Modifiers, ParentElement, Render, ScrollDelta,
    ScrollWheelEvent, Styled, TestAppContext, TouchPhase, VisualTestContext, Window, div, point,
    px,
};

fn install_keys(cx: &mut gpui::App) {
    install_component_keybindings(
        cx,
        &ComponentKeymap {
            primary: "ctrl",
            word: "ctrl",
            emacs_line_keys: false,
            character_palette: false,
        },
    );
}

/// Five 100px rows in a 100px vertical view, each added with `.child`.
struct Rows {
    control: ScrollControl,
    focusable: bool,
}

impl Render for Rows {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut view = ScrollView::vertical("rows")
            .control(self.control.clone())
            .focusable(self.focusable)
            .w(px(100.0))
            .h(px(100.0))
            .flex()
            .flex_col();
        for _ in 0..5 {
            view = view.child(div().flex_none().w(px(100.0)).h(px(100.0)));
        }
        div().size(px(300.0)).child(view)
    }
}

fn redraw(view: &gpui::Entity<Rows>, cx: &mut VisualTestContext) {
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

fn rows(
    cx: &mut TestAppContext,
    focusable: bool,
) -> (ScrollControl, gpui::Entity<Rows>, &mut VisualTestContext) {
    cx.update(install_keys);
    let control = ScrollControl::new();
    let (view, cx) = cx.add_window_view({
        let control = control.clone();
        move |_, _| Rows { control, focusable }
    });
    cx.run_until_parked();
    (control, view, cx)
}

#[gpui::test]
fn every_child_is_kept_in_order_and_reveal_addresses_it(cx: &mut TestAppContext) {
    let (control, view, cx) = rows(cx, false);
    // Five children of 100px: 400px of overflow. "Last child wins" would
    // leave one child and no overflow.
    assert_eq!(f32::from(control.max_offset().height), 400.0);
    assert!(control.can_scroll_forward(atelier_ui::ScrollAxis::Vertical));
    assert!(!control.can_scroll_back(atelier_ui::ScrollAxis::Vertical));

    control.reveal_child(3);
    redraw(&view, cx);
    assert_eq!(
        f32::from(control.offset().y),
        -300.0,
        "child 3 is the 4th row"
    );
    control.reveal_child(4);
    redraw(&view, cx);
    assert_eq!(f32::from(control.offset().y), -400.0);
    control.reveal_child(1);
    redraw(&view, cx);
    assert_eq!(f32::from(control.offset().y), -100.0, "least movement back");
    // Revealing a visible child does not move.
    control.reveal_child(1);
    redraw(&view, cx);
    assert_eq!(f32::from(control.offset().y), -100.0);
    // Past the end: nothing happens.
    control.reveal_child(40);
    redraw(&view, cx);
    assert_eq!(f32::from(control.offset().y), -100.0);
}

#[gpui::test]
fn the_focus_ring_does_not_shift_child_indices(cx: &mut TestAppContext) {
    let (control, view, cx) = rows(cx, true);
    cx.update(|window, cx| {
        note_keyboard_navigation(cx);
        window.focus_next();
    });
    redraw(&view, cx);
    control.reveal_child(3);
    redraw(&view, cx);
    assert_eq!(f32::from(control.offset().y), -300.0);
    control.reveal_child(0);
    redraw(&view, cx);
    assert_eq!(f32::from(control.offset().y), 0.0);
}

/// A vertical page holding a rail of eight 90px tiles in a 200px row,
/// above filler that makes the page scroll.
struct Page {
    page: ScrollControl,
    rail: RailState,
    tiles: Vec<FocusHandle>,
}

impl Render for Page {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut rail = Rail::new("rail", &self.rail).w(px(200.0)).h(px(60.0));
        for (index, focus) in self.tiles.iter().enumerate() {
            rail = rail.item(
                focus.clone(),
                Pressable::new(("tile", index), format!("Tile {index}"))
                    .focus_handle(focus.clone())
                    .on_press(|_, _, _| {})
                    .w(px(90.0))
                    .h(px(60.0)),
            );
        }
        div().size(px(200.0)).child(
            ScrollView::vertical("page")
                .control(self.page.clone())
                .restrict_to_axis(true)
                .size_full()
                .flex()
                .flex_col()
                .child(rail)
                .child(div().flex_none().w(px(200.0)).h(px(600.0))),
        )
    }
}

fn wheel(cx: &mut VisualTestContext, dx: f32, dy: f32) {
    let position = point(px(50.0), px(30.0));
    cx.simulate_mouse_move(position, None, Modifiers::default());
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_vertical_wheel_over_a_rail_scrolls_the_page_not_the_rail(cx: &mut TestAppContext) {
    cx.update(install_keys);
    let page = ScrollControl::new();
    let rail = RailState::new();
    let (_view, cx) = cx.add_window_view({
        let (page, rail) = (page.clone(), rail.clone());
        move |_, cx| Page {
            page,
            rail,
            tiles: (0..8)
                .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
                .collect(),
        }
    });
    cx.run_until_parked();
    assert!(rail.can_page_forward());

    // The pointer is over the rail. A plain vertical wheel moves the page.
    wheel(cx, 0.0, -40.0);
    assert_eq!(f32::from(page.offset().y), -40.0, "the page scrolled");
    assert_eq!(f32::from(rail.scroll().offset().x), 0.0, "the rail did not");

    // Horizontal motion (a trackpad, or Shift-wheel turned sideways by the
    // platform) moves the rail and leaves the page alone.
    wheel(cx, 0.0, 40.0);
    assert_eq!(f32::from(page.offset().y), 0.0);
    cx.simulate_mouse_move(point(px(50.0), px(30.0)), None, Modifiers::default());
    wheel(cx, -70.0, 0.0);
    assert_eq!(
        f32::from(rail.scroll().offset().x),
        -70.0,
        "the rail scrolled"
    );
    assert_eq!(f32::from(page.offset().y), 0.0, "the page did not");
}
