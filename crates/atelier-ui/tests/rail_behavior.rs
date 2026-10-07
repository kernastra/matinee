//! Headless behavior tests for the horizontal rail.

use atelier_ui::{
    ComponentKeymap, FocusNext, FocusPrevious, Pressable, Rail, RailState,
    install_component_keybindings,
};
use gpui::{
    Context, FocusHandle, IntoElement, KeyBinding, KeyDownEvent, KeyUpEvent, Keystroke,
    ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window, div, px,
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

struct RailHarness {
    state: RailState,
    handles: Vec<FocusHandle>,
    focused: Vec<usize>,
}

impl Render for RailHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let mut rail = Rail::new("rail", &self.state)
            .w(px(200.0))
            .on_focus(move |index, _, cx| {
                view.update(cx, |this, _| this.focused.push(index));
            });
        for (index, handle) in self.handles.iter().enumerate() {
            rail = rail.item(
                handle.clone(),
                Pressable::new(("tile", index), format!("Tile {index}"))
                    .focus_handle(handle.clone())
                    .on_press(|_, _, _| {})
                    .w(px(90.0))
                    .h(px(60.0)),
            );
        }
        div().size(px(400.0)).child(rail)
    }
}

#[gpui::test]
fn arrows_move_between_items_and_reveal_them(cx: &mut TestAppContext) {
    cx.update(install_keys);
    let state = RailState::new();
    let (view, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, cx| RailHarness {
            state,
            handles: (0..8)
                .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
                .collect(),
            focused: Vec::new(),
        }
    });
    cx.run_until_parked();
    let handles = view.read_with(cx, |this, _| this.handles.clone());
    assert!(
        state.can_page_forward(),
        "eight 90px tiles overflow a 200px rail"
    );
    assert!(!state.can_page_back());

    cx.update(|window, cx| {
        atelier_ui::note_keyboard_navigation(cx);
        window.focus_next();
    });
    cx.run_until_parked();
    cx.update(|window, _| assert!(handles[0].is_focused(window)));
    press(cx, "left");
    cx.update(|window, _| assert!(handles[0].is_focused(window), "no wrap"));
    press(cx, "right");
    press(cx, "right");
    cx.update(|window, _| assert!(handles[2].is_focused(window)));
    press(cx, "end");
    cx.update(|window, _| assert!(handles[7].is_focused(window)));
    cx.run_until_parked();
    let offset = f32::from(state.scroll().offset().x);
    assert!(
        offset < -100.0,
        "the last tile is scrolled into view: {offset}"
    );
    press(cx, "home");
    cx.update(|window, _| assert!(handles[0].is_focused(window)));
    cx.run_until_parked();
    assert!(
        f32::from(state.scroll().offset().x) > -0.5,
        "back at the start"
    );
    assert_eq!(state.last_focused(), Some(0));
    let focused = view.read_with(cx, |this, _| this.focused.clone());
    assert_eq!(focused, vec![0, 1, 2, 7, 0], "each move is reported once");
}

#[gpui::test]
fn paging_moves_the_offset_without_focus(cx: &mut TestAppContext) {
    cx.update(install_keys);
    let state = RailState::new();
    let (_view, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, cx| RailHarness {
            state,
            handles: (0..8)
                .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
                .collect(),
            focused: Vec::new(),
        }
    });
    cx.run_until_parked();
    state.page(true);
    assert!(f32::from(state.scroll().offset().x) < -100.0);
    assert!(state.can_page_back());
    state.page(false);
    assert!(!state.can_page_back());
    assert_eq!(state.last_focused(), None);
}

/// A tab stop before the rail, so focus can leave and come back.
struct ReentryHarness {
    state: RailState,
    before: FocusHandle,
    handles: Vec<FocusHandle>,
}

impl Render for ReentryHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut rail = Rail::new("rail", &self.state).w(px(200.0));
        for (index, handle) in self.handles.iter().enumerate() {
            rail = rail.item(
                handle.clone(),
                Pressable::new(("tile", index), format!("Tile {index}"))
                    .focus_handle(handle.clone())
                    .on_press(|_, _, _| {})
                    .w(px(90.0))
                    .h(px(60.0)),
            );
        }
        div()
            .size(px(400.0))
            .child(
                Pressable::new("before", "Before")
                    .focus_handle(self.before.clone())
                    .on_press(|_, _, _| {})
                    .w(px(40.0))
                    .h(px(20.0)),
            )
            .child(rail)
    }
}

fn reentry(
    cx: &mut TestAppContext,
    count: usize,
) -> (
    RailState,
    FocusHandle,
    Vec<FocusHandle>,
    &mut VisualTestContext,
) {
    cx.update(install_keys);
    let state = RailState::new();
    let (view, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, cx| ReentryHarness {
            state,
            before: cx.focus_handle().tab_index(0).tab_stop(true),
            handles: (0..count)
                .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
                .collect(),
        }
    });
    cx.run_until_parked();
    let (before, handles) =
        view.read_with(cx, |this, _| (this.before.clone(), this.handles.clone()));
    (state, before, handles, cx)
}

fn focus(cx: &mut VisualTestContext, handle: &FocusHandle) {
    let handle = handle.clone();
    cx.update(move |window, cx| {
        atelier_ui::note_keyboard_navigation(cx);
        window.focus(&handle);
    });
    cx.run_until_parked();
}

fn focused(cx: &mut VisualTestContext, handles: &[FocusHandle]) -> Option<usize> {
    cx.update(|window, _| handles.iter().position(|handle| handle.is_focused(window)))
}

#[gpui::test]
fn ends_hold_and_home_end_jump(cx: &mut TestAppContext) {
    let (_state, _before, handles, cx) = reentry(cx, 5);
    focus(cx, &handles[0]);
    press(cx, "left");
    assert_eq!(focused(cx, &handles), Some(0), "first + Left stays first");
    press(cx, "end");
    assert_eq!(focused(cx, &handles), Some(4), "End selects last");
    press(cx, "right");
    assert_eq!(focused(cx, &handles), Some(4), "last + Right stays last");
    press(cx, "home");
    assert_eq!(focused(cx, &handles), Some(0), "Home selects first");
}

#[gpui::test]
fn a_one_item_rail_stays_put(cx: &mut TestAppContext) {
    let (state, _before, handles, cx) = reentry(cx, 1);
    assert!(!state.can_page_forward() && !state.can_page_back());
    focus(cx, &handles[0]);
    for key in ["left", "right", "home", "end"] {
        press(cx, key);
        assert_eq!(focused(cx, &handles), Some(0), "{key}");
    }
    assert_eq!(state.last_focused(), Some(0));
}

#[gpui::test]
fn an_empty_rail_does_nothing(cx: &mut TestAppContext) {
    let (state, before, handles, cx) = reentry(cx, 0);
    assert!(handles.is_empty());
    assert!(!state.can_page_forward() && !state.can_page_back());
    focus(cx, &before);
    for key in ["left", "right", "home", "end"] {
        press(cx, key);
    }
    cx.update(|window, _| assert!(before.is_focused(window), "focus stays outside"));
    assert_eq!(state.last_focused(), None);
    state.page(true);
    assert_eq!(f32::from(state.scroll().offset().x), 0.0);
}

#[gpui::test]
fn the_focused_item_is_remembered_and_tab_reenters_in_order(cx: &mut TestAppContext) {
    let (state, before, handles, cx) = reentry(cx, 6);
    focus(cx, &handles[0]);
    press(cx, "right");
    press(cx, "right");
    press(cx, "right");
    assert_eq!(focused(cx, &handles), Some(3));
    let revealed = f32::from(state.scroll().offset().x);
    assert!(revealed < 0.0, "item 3 was scrolled into view: {revealed}");

    // Focus leaves the rail: the rail remembers item 3 for its owner.
    focus(cx, &before);
    assert_eq!(focused(cx, &handles), None);
    assert_eq!(state.last_focused(), Some(3));
    assert_eq!(
        f32::from(state.scroll().offset().x),
        revealed,
        "leaving does not move the row"
    );

    // Tab re-enters in tab order: the first item, revealed. Owners that
    // want the remembered item (Home's Up and Down) focus it themselves.
    cx.update(|window, cx| {
        atelier_ui::note_keyboard_navigation(cx);
        window.focus_next();
    });
    cx.run_until_parked();
    assert_eq!(focused(cx, &handles), Some(0));
    assert_eq!(state.last_focused(), Some(0));
    assert_eq!(f32::from(state.scroll().offset().x), 0.0);

    // An owner moving focus straight to the remembered item works too.
    focus(cx, &handles[3]);
    assert_eq!(focused(cx, &handles), Some(3));
}
