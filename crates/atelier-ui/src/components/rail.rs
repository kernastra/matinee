//! Horizontal rail: a clipped row of focusable items that scrolls sideways.
//!
//! Each item is a direct child of the scrolling element, with a focus
//! handle its owner supplies (usually passed on to a
//! [`Pressable`](super::Pressable)). The rail adds what a plain horizontal
//! [`ScrollView`](super::ScrollView) lacks for a row of tiles:
//!
//! - Left and Right move focus to the neighboring item; Home and End to the
//!   first and last. Movement does not wrap. Tab still walks every item.
//! - When keyboard focus lands on an item, the rail scrolls the least
//!   amount that shows it, so focus never sits on an item out of view.
//! - A vertical mouse wheel is left to the page around the rail. A
//!   trackpad's horizontal motion, or a horizontal wheel, scrolls the rail.
//!   Give the page [`ScrollView::restrict_to_axis`](super::ScrollView::restrict_to_axis)
//!   too, or GPUI also turns that sideways motion into page scrolling.
//!
//! [`RailState`] lives with the owner, so the scroll offset and the last
//! focused index survive the rail not being drawn for a while.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Div, ElementId, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, StatefulInteractiveElement, StyleRefinement, Styled, Window, div,
};

use crate::{
    components::{
        ScrollAxis, ScrollControl,
        keybindings::{NudgeLeft, NudgeRight, NudgeToEnd, NudgeToStart, RAIL_CONTEXT},
    },
    focus::{self, InputModality},
    navigation::{NavStep, move_enabled},
    tokens::Space,
};

type FocusHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// Scroll offset and focus memory for one [`Rail`]. Clone is cheap; clones
/// share the same state.
#[derive(Clone, Debug, Default)]
pub struct RailState {
    scroll: ScrollControl,
    /// The item that had focus when the rail was last drawn.
    inside: Rc<Cell<Option<usize>>>,
    /// The last item that had focus, kept after focus leaves the rail.
    remembered: Rc<Cell<Option<usize>>>,
}

impl RailState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scroll(&self) -> &ScrollControl {
        &self.scroll
    }

    /// The index of the item that last held focus, if any did.
    pub fn last_focused(&self) -> Option<usize> {
        self.remembered.get()
    }

    /// Move about one rail width toward the end (`forward`) or the start.
    pub fn page(&self, forward: bool) {
        self.scroll.page(ScrollAxis::Horizontal, forward);
    }

    pub fn can_page_back(&self) -> bool {
        self.scroll.can_scroll_back(ScrollAxis::Horizontal)
    }

    pub fn can_page_forward(&self) -> bool {
        self.scroll.can_scroll_forward(ScrollAxis::Horizontal)
    }

    /// Record which item is focused now. Returns the index when focus moved
    /// onto a different item since the last call.
    fn observe(&self, focused: Option<usize>) -> Option<usize> {
        let previous = self.inside.replace(focused);
        if focused.is_some() {
            self.remembered.set(focused);
        }
        focused.filter(|_| focused != previous)
    }
}

/// A horizontal row of focusable items. See the module notes.
#[derive(IntoElement)]
pub struct Rail {
    id: ElementId,
    state: RailState,
    gap: Space,
    items: Vec<(FocusHandle, AnyElement)>,
    on_focus: Option<FocusHandler>,
    base: Div,
}

impl Rail {
    pub fn new(id: impl Into<ElementId>, state: &RailState) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            gap: Space::S4,
            items: Vec::new(),
            on_focus: None,
            base: div(),
        }
    }

    pub fn gap(mut self, gap: Space) -> Self {
        self.gap = gap;
        self
    }

    /// Add an item. `focus` must be the handle the item tracks.
    pub fn item(mut self, focus: FocusHandle, element: impl IntoElement) -> Self {
        self.items.push((focus, element.into_any_element()));
        self
    }

    /// Called when focus lands on a different item, after the rail has
    /// asked to reveal it. Owners use it to reveal the rail on the page.
    pub fn on_focus(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_focus = Some(Rc::new(handler));
        self
    }
}

impl Styled for Rail {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

/// Move focus inside the rail. Returns the index that now has focus.
fn step(handles: &[FocusHandle], step: NavStep, window: &mut Window, cx: &mut App) {
    let current = handles.iter().position(|handle| handle.is_focused(window));
    let enabled = vec![false; handles.len()];
    let Some(next) = move_enabled(&enabled, current, step) else {
        return;
    };
    if Some(next) == current {
        return;
    }
    focus::note_keyboard_navigation(cx);
    window.focus(&handles[next]);
}

impl RenderOnce for Rail {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handles: Rc<Vec<FocusHandle>> =
            Rc::new(self.items.iter().map(|(focus, _)| focus.clone()).collect());
        let focused = handles.iter().position(|handle| handle.is_focused(window));
        if let Some(index) = self.state.observe(focused) {
            if focus::input_modality(cx) == InputModality::Keyboard {
                self.state.scroll.reveal_child(index);
            }
            if let Some(handler) = &self.on_focus {
                handler(index, window, cx);
            }
        }

        let mut view = self
            .base
            .id(self.id)
            .key_context(RAIL_CONTEXT)
            .flex()
            .flex_row()
            .items_start()
            .gap(self.gap.px())
            .min_w(gpui::px(0.0))
            .overflow_x_scroll()
            .track_scroll(self.state.scroll.handle());
        // Leave the vertical wheel to the page; see the module notes.
        view.style().restrict_scroll_to_axis = Some(true);

        view.on_action({
            let handles = Rc::clone(&handles);
            move |_: &NudgeLeft, window, cx| step(&handles, NavStep::Previous, window, cx)
        })
        .on_action({
            let handles = Rc::clone(&handles);
            move |_: &NudgeRight, window, cx| step(&handles, NavStep::Next, window, cx)
        })
        .on_action({
            let handles = Rc::clone(&handles);
            move |_: &NudgeToStart, window, cx| step(&handles, NavStep::First, window, cx)
        })
        .on_action(move |_: &NudgeToEnd, window, cx| step(&handles, NavStep::Last, window, cx))
        // Items keep their own width; the row scrolls instead of squeezing them.
        .children(
            self.items
                .into_iter()
                .map(|(_, element)| div().flex_none().child(element)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_memory_reports_only_moves() {
        let state = RailState::new();
        assert_eq!(state.observe(None), None);
        assert_eq!(state.observe(Some(2)), Some(2));
        assert_eq!(state.observe(Some(2)), None, "same item, no move");
        assert_eq!(state.observe(None), None);
        assert_eq!(state.last_focused(), Some(2), "kept after focus leaves");
        assert_eq!(state.observe(Some(2)), Some(2), "coming back is a move");
    }
}
