use gpui::{App, IntoElement, RenderOnce, Styled, Window, div, px};

use crate::ActiveTheme;

/// Width of the framework focus ring.
pub const FOCUS_RING_WIDTH: f32 = 2.0;
/// Gap between the control's border box and the ring.
pub const FOCUS_RING_GAP: f32 = 1.0;

/// The framework focus indicator: a ring drawn just outside the parent's
/// border box, following its corner radius.
///
/// Add it as the last child of a `relative()` element *only while that
/// element is focused*. (GPUI 0.2.2 cannot draw a zero-blur spread shadow,
/// which is why this is an overlay rather than a style.)
#[derive(IntoElement, Debug)]
pub struct FocusRing {
    corner_radius: f32,
    border_width: f32,
}

impl FocusRing {
    /// `corner_radius` and `border_width` describe the focused element.
    pub fn new(corner_radius: f32, border_width: f32) -> Self {
        Self {
            corner_radius,
            border_width,
        }
    }
}

impl RenderOnce for FocusRing {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Absolute children are positioned against the padding box, so the
        // parent's border is added to reach outside its border box.
        let outset = self.border_width + FOCUS_RING_GAP + FOCUS_RING_WIDTH;
        div()
            .absolute()
            .top(px(-outset))
            .left(px(-outset))
            .right(px(-outset))
            .bottom(px(-outset))
            .rounded(px(self.corner_radius + FOCUS_RING_GAP + FOCUS_RING_WIDTH))
            .border(px(FOCUS_RING_WIDTH))
            .border_color(cx.theme().colors.focus.ring)
    }
}
