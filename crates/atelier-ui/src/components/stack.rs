use gpui::{Div, Styled, div};

use crate::tokens::Space;

/// A horizontal flex row with vertically centered children.
pub fn h_stack(gap: Space) -> Div {
    div().flex().flex_row().items_center().gap(gap.px())
}

/// A vertical flex column.
pub fn v_stack(gap: Space) -> Div {
    div().flex().flex_col().gap(gap.px())
}
