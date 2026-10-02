//! Scrolling container.
//!
//! Wraps GPUI's overflow scroll: mouse wheel, trackpad pixel deltas, and
//! clipping are GPUI's. Scrollbars are GPUI's as well; this view only
//! reserves a gutter. Programmatic scrolling sets the offset directly.
//! GPUI 0.2.2 does not animate that offset, so reduced motion needs no
//! separate path.

use gpui::prelude::FluentBuilder;
use gpui::{
    App, Bounds, Div, ElementId, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Pixels, Point, RenderOnce, ScrollHandle, Size, StatefulInteractiveElement, StyleRefinement,
    Styled, Window, div, point, px,
};

use crate::{
    ActiveTheme,
    components::{
        FocusRing,
        keybindings::{
            NudgeDown, NudgeLeft, NudgePageDown, NudgePageUp, NudgeRight, NudgeToEnd, NudgeToStart,
            NudgeUp, SCROLL_CONTEXT,
        },
    },
    focus::{self, focus_visible},
    tokens::{Radius, Space},
};

const LINE: f32 = Space::S8.value();
const PAGE_FRACTION: f32 = 0.9;

/// Which axes accept a scroll offset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ScrollAxis {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

impl ScrollAxis {
    pub const fn allows_x(self) -> bool {
        matches!(self, ScrollAxis::Horizontal | ScrollAxis::Both)
    }

    pub const fn allows_y(self) -> bool {
        matches!(self, ScrollAxis::Vertical | ScrollAxis::Both)
    }
}

/// Clamps a scroll offset. `0` is the start; values become negative as the
/// content moves. `max_extent` is the positive overflow on that axis.
pub fn clamp_scroll_offset(offset: f32, max_extent: f32) -> f32 {
    let max_extent = if max_extent.is_finite() {
        max_extent.max(0.0)
    } else {
        0.0
    };
    if !offset.is_finite() {
        return 0.0;
    }
    offset.clamp(-max_extent, 0.0)
}

/// Shared handle for a [`ScrollView`]. Clone is cheap; copies control the
/// same offset. Lists use it to bring a row into view.
#[derive(Clone, Debug)]
pub struct ScrollControl {
    handle: ScrollHandle,
}

impl Default for ScrollControl {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollControl {
    pub fn new() -> Self {
        Self {
            handle: ScrollHandle::new(),
        }
    }

    pub fn offset(&self) -> Point<Pixels> {
        self.handle.offset()
    }

    pub fn max_offset(&self) -> Size<Pixels> {
        self.handle.max_offset()
    }

    pub fn bounds(&self) -> Bounds<Pixels> {
        self.handle.bounds()
    }

    pub fn set_offset(&self, position: Point<Pixels>) {
        let max = self.max_offset();
        self.handle.set_offset(point(
            px(clamp_scroll_offset(
                f32::from(position.x),
                f32::from(max.width),
            )),
            px(clamp_scroll_offset(
                f32::from(position.y),
                f32::from(max.height),
            )),
        ));
    }

    /// `dx` and `dy` are added to the offset. Negative values reveal content
    /// further along that axis.
    pub fn scroll_by(&self, dx: f32, dy: f32) {
        let current = self.offset();
        self.set_offset(point(
            px(f32::from(current.x) + dx),
            px(f32::from(current.y) + dy),
        ));
    }

    pub(crate) fn handle(&self) -> &ScrollHandle {
        &self.handle
    }

    pub(crate) fn scroll_line(&self, horizontal: bool, forward: bool) {
        let delta = LINE * if forward { -1.0 } else { 1.0 };
        if horizontal {
            self.scroll_by(delta, 0.0);
        } else {
            self.scroll_by(0.0, delta);
        }
    }

    pub(crate) fn scroll_page(&self, horizontal: bool, forward: bool) {
        let bounds = self.bounds().size;
        let extent = f32::from(if horizontal {
            bounds.width
        } else {
            bounds.height
        });
        let delta = extent * PAGE_FRACTION * if forward { -1.0 } else { 1.0 };
        if horizontal {
            self.scroll_by(delta, 0.0);
        } else {
            self.scroll_by(0.0, delta);
        }
    }

    pub(crate) fn scroll_edge(&self, horizontal: bool, end: bool) {
        let max = self.max_offset();
        let mut offset = self.offset();
        if horizontal {
            offset.x = px(if end { -f32::from(max.width) } else { 0.0 });
        } else {
            offset.y = px(if end { -f32::from(max.height) } else { 0.0 });
        }
        self.set_offset(offset);
    }
}

struct ScrollState {
    focus: FocusHandle,
    control: ScrollControl,
}

/// A clipping scroll container. Place a [`crate::List`] inside it; the list
/// does not scroll on its own.
#[derive(IntoElement)]
pub struct ScrollView {
    id: ElementId,
    axis: ScrollAxis,
    focusable: bool,
    control: Option<ScrollControl>,
    base: Div,
    child: Option<gpui::AnyElement>,
}

impl ScrollView {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            axis: ScrollAxis::Vertical,
            focusable: false,
            control: None,
            base: div(),
            child: None,
        }
    }

    pub fn vertical(id: impl Into<ElementId>) -> Self {
        Self::new(id).axis(ScrollAxis::Vertical)
    }

    pub fn axis(mut self, axis: ScrollAxis) -> Self {
        self.axis = axis;
        self
    }

    /// Keyboard scrolling (arrows, page, home, end). Off by default so a
    /// list inside the view keeps the arrow keys.
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }

    /// Observe or drive the offset from outside the view.
    pub fn control(mut self, control: ScrollControl) -> Self {
        self.control = Some(control);
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(child.into_any_element());
        self
    }
}

impl Styled for ScrollView {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl ParentElement for ScrollView {
    fn extend(&mut self, elements: impl IntoIterator<Item = gpui::AnyElement>) {
        if let Some(child) = elements.into_iter().last() {
            self.child = Some(child);
        }
    }
}

impl RenderOnce for ScrollView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let model = window.use_keyed_state(self.id.clone(), cx, |_, cx| ScrollState {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            control: ScrollControl::new(),
        });
        let focus = model.read(cx).focus.clone();
        let control = self
            .control
            .clone()
            .unwrap_or_else(|| model.read(cx).control.clone());
        let axis = self.axis;
        let focused = self.focusable && focus.is_focused(window);

        let mut view = self
            .base
            .id(self.id)
            .relative()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .when(axis.allows_x(), |this| this.overflow_x_scroll())
            .when(axis.allows_y(), |this| this.overflow_y_scroll())
            .scrollbar_width(Space::S2.px())
            .track_scroll(control.handle());

        if self.focusable {
            let line = control.clone();
            let page = control.clone();
            let edge = control.clone();
            view = view
                .key_context(SCROLL_CONTEXT)
                .track_focus(&focus)
                .on_action({
                    let line = line.clone();
                    move |_: &NudgeUp, _, cx| {
                        focus::note_keyboard_navigation(cx);
                        line.scroll_line(false, false);
                    }
                })
                .on_action({
                    let line = line.clone();
                    move |_: &NudgeDown, _, cx| {
                        focus::note_keyboard_navigation(cx);
                        line.scroll_line(false, true);
                    }
                })
                .on_action({
                    let line = line.clone();
                    move |_: &NudgeLeft, _, cx| {
                        focus::note_keyboard_navigation(cx);
                        line.scroll_line(true, false);
                    }
                })
                .on_action(move |_: &NudgeRight, _, cx| {
                    focus::note_keyboard_navigation(cx);
                    line.scroll_line(true, true);
                })
                .on_action({
                    let page = page.clone();
                    move |_: &NudgePageUp, _, cx| {
                        focus::note_keyboard_navigation(cx);
                        page.scroll_page(false, false);
                    }
                })
                .on_action({
                    let page = page.clone();
                    move |_: &NudgePageDown, _, cx| {
                        focus::note_keyboard_navigation(cx);
                        page.scroll_page(false, true);
                    }
                })
                .on_action({
                    let edge = edge.clone();
                    move |_: &NudgeToStart, _, cx| {
                        focus::note_keyboard_navigation(cx);
                        edge.scroll_edge(axis == ScrollAxis::Horizontal, false);
                    }
                })
                .on_action(move |_: &NudgeToEnd, _, cx| {
                    focus::note_keyboard_navigation(cx);
                    edge.scroll_edge(axis == ScrollAxis::Horizontal, true);
                });
        }

        view.when(focus_visible(focused, cx), |this| {
            this.child(FocusRing::new(cx.theme().radius.get(Radius::Medium), 0.0))
        })
        .children(self.child)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_match_the_requested_direction() {
        assert!(ScrollAxis::Vertical.allows_y() && !ScrollAxis::Vertical.allows_x());
        assert!(ScrollAxis::Horizontal.allows_x() && !ScrollAxis::Horizontal.allows_y());
        assert!(ScrollAxis::Both.allows_x() && ScrollAxis::Both.allows_y());
    }

    #[test]
    fn offsets_stay_within_the_overflow() {
        assert_eq!(clamp_scroll_offset(10.0, 40.0), 0.0);
        assert_eq!(clamp_scroll_offset(-12.0, 40.0), -12.0);
        assert_eq!(clamp_scroll_offset(-80.0, 40.0), -40.0);
        assert_eq!(clamp_scroll_offset(f32::NAN, 40.0), 0.0);
        assert_eq!(clamp_scroll_offset(-8.0, f32::NAN), 0.0);
    }
}
