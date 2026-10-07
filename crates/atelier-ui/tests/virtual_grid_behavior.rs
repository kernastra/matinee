//! Headless behavior tests for the virtualized grid.

use atelier_ui::{
    ComponentKeymap, FocusNext, FocusPrevious, GridSizing, VirtualGrid, VirtualGridState,
    install_component_keybindings,
};
use gpui::{
    Context, IntoElement, KeyBinding, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, ParentElement, Render, Styled, TestAppContext, VisualTestContext,
    Window, div, point, px,
};

fn install_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrevious, None),
    ]);
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

fn press(cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    cx.run_until_parked();
}

fn sizing() -> GridSizing {
    GridSizing {
        min_cell_width: 100.0,
        max_cell_width: 120.0,
        aspect: 1.5,
        extra_height: 20.0,
        column_gap: 10.0,
        row_gap: 10.0,
        inset_x: 20.0,
        inset_top: 10.0,
        inset_bottom: 10.0,
        max_columns: 12,
        overscan_rows: 1,
    }
}

struct GridHarness {
    state: VirtualGridState,
    count: usize,
    width: f32,
    built: Vec<usize>,
    activated: Vec<usize>,
    moved: Vec<usize>,
}

impl Render for GridHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.built.clear();
        let view = cx.entity().downgrade();
        let built = view.clone();
        let moved = view.clone();
        div().w(px(self.width)).h(px(620.0)).child(
            VirtualGrid::new("grid", &self.state, self.count, move |cell, _, cx| {
                built.update(cx, |this, _| this.built.push(cell.index)).ok();
                div().size_full().into_any_element()
            })
            .sizing(sizing())
            .fallback_viewport((self.width, 620.0))
            .on_activate(move |index, _, cx| {
                view.update(cx, |this, _| this.activated.push(index)).ok();
            })
            .on_focus(move |index, _, cx| {
                moved.update(cx, |this, _| this.moved.push(index)).ok();
            }),
        )
    }
}

fn open(
    cx: &mut TestAppContext,
    count: usize,
    width: f32,
) -> (gpui::Entity<GridHarness>, &mut VisualTestContext) {
    cx.update(install_keys);
    let (view, cx) = cx.add_window_view(move |_, cx| GridHarness {
        state: VirtualGridState::new(cx),
        count,
        width,
        built: Vec::new(),
        activated: Vec::new(),
        moved: Vec::new(),
    });
    cx.run_until_parked();
    (view, cx)
}

fn state(view: &gpui::Entity<GridHarness>, cx: &mut VisualTestContext) -> VirtualGridState {
    view.read_with(cx, |this, _| this.state.clone())
}

fn focus_grid(view: &gpui::Entity<GridHarness>, cx: &mut VisualTestContext) {
    let state = state(view, cx);
    cx.update(|window, cx| {
        atelier_ui::note_keyboard_navigation(cx);
        window.focus(state.focus_handle());
    });
    cx.run_until_parked();
}

#[gpui::test]
fn ten_thousand_items_build_tens_of_cells(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, 10_000, 640.0);
    let state = state(&view, cx);
    assert_eq!(state.measured(), Some((640.0, 620.0)));
    let built = view.read_with(cx, |this, _| this.built.clone());
    assert!(!built.is_empty());
    assert!(built.len() <= 35, "built {} cells", built.len());
    assert_eq!(state.rendered_cells(), built.len());
    assert_eq!(built[0], 0);

    // Far down the collection: still tens, and they are the right ones.
    state.scroll().set_offset(point(px(0.0), px(-150_000.0)));
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    let built = view.read_with(cx, |this, _| this.built.clone());
    assert!(built.len() <= 35, "built {} cells", built.len());
    let first = built[0];
    assert!(
        first > 3000,
        "cells come from deep in the collection: {first}"
    );
}

#[gpui::test]
fn keyboard_focus_moves_by_row_and_reveals_offscreen_cells(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, 10_000, 640.0);
    let state = state(&view, cx);
    focus_grid(&view, cx);
    assert_eq!(state.focused(), Some(0), "focus lands on the first cell");
    press(cx, "right");
    press(cx, "right");
    press(cx, "right");
    assert_eq!(state.focused(), Some(3));
    press(cx, "down");
    assert_eq!(state.focused(), Some(8), "five columns: one row down");
    // Walk well past the rendered window.
    for _ in 0..10 {
        press(cx, "down");
    }
    assert_eq!(state.focused(), Some(58));
    let built = view.read_with(cx, |this, _| this.built.clone());
    assert!(built.contains(&58), "the focused cell was materialized");
    assert!(
        f32::from(state.scroll().offset().y) < -1000.0,
        "the grid scrolled to it"
    );
    press(cx, "pagedown");
    assert!(state.focused().unwrap() > 58);
    press(cx, "end");
    assert_eq!(state.focused(), Some(9_999));
    let built = view.read_with(cx, |this, _| this.built.clone());
    assert!(built.contains(&9_999));
    press(cx, "home");
    assert_eq!(state.focused(), Some(0));
    assert!(
        f32::from(state.scroll().offset().y) > -0.5,
        "back at the top"
    );
    let moved = view.read_with(cx, |this, _| this.moved.clone());
    assert_eq!(&moved[..4], &[1, 2, 3, 8], "each move is reported");

    press(cx, "enter");
    press(cx, "space");
    let activated = view.read_with(cx, |this, _| this.activated.clone());
    assert_eq!(activated, vec![0, 0]);
}

#[gpui::test]
fn resizing_keeps_the_logical_focus_and_keeps_it_visible(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, 1_000, 760.0);
    let state = state(&view, cx);
    focus_grid(&view, cx);
    for _ in 0..20 {
        press(cx, "down");
    }
    let focused = state.focused().unwrap();
    assert_eq!(focused, 120, "six columns");
    view.update(cx, |this, cx| {
        this.width = 520.0;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        state.focused(),
        Some(120),
        "the same item, not the same slot"
    );
    let built = view.read_with(cx, |this, _| this.built.clone());
    assert!(built.contains(&120), "still built after the column change");
    press(cx, "down");
    assert_eq!(state.focused(), Some(124), "four columns now");
}

#[gpui::test]
fn a_click_focuses_and_activates_that_cell(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, 40, 640.0);
    let state = state(&view, cx);
    // Cell 7: column 2, row 1. Origin (20 + 2 * 122, 10 + 198).
    let position = point(px(20.0 + 2.0 * 122.0 + 30.0), px(208.0 + 30.0));
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    assert_eq!(state.focused(), Some(7));
    let activated = view.read_with(cx, |this, _| this.activated.clone());
    assert_eq!(activated, vec![7]);
    cx.update(|window, _| assert!(state.focus_handle().is_focused(window)));
}

#[gpui::test]
fn an_empty_grid_builds_nothing_and_ignores_keys(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, 0, 640.0);
    let state = state(&view, cx);
    focus_grid(&view, cx);
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(state.focused(), None);
    assert_eq!(state.rendered_cells(), 0);
    assert!(view.read_with(cx, |this, _| this.activated.is_empty()));
}
