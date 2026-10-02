//! Shaping, caret and selection geometry, scrolling, and pointer mapping.

use std::ops::Range;

use gpui::{
    Bounds, Hsla, PaintQuad, Pixels, Point, ShapedLine, SharedString, TextRun, UnderlineStyle,
    Window, fill, point, px, size,
};

use crate::{
    editing,
    tokens::{Color, Weight},
};

use super::state::FieldState;

const CARET_WIDTH: f32 = 1.0;

pub(super) struct LineStyle {
    pub placeholder: SharedString,
    pub font_family: SharedString,
    pub font_weight: Weight,
    pub font_size: f32,
    pub text_color: Color,
    pub placeholder_color: Color,
    pub selection_color: Color,
    pub caret_color: Color,
    pub focused: bool,
    pub disabled: bool,
}

pub(super) struct FieldPrepaint {
    pub line: Option<ShapedLine>,
    pub caret: Option<PaintQuad>,
    pub selection: Option<PaintQuad>,
    pub scroll: Pixels,
}

pub(super) fn index_for_position(state: &FieldState, position: Point<Pixels>) -> usize {
    let bounds = state.text_bounds.borrow();
    let layout = state.layout.borrow();
    let (Some(bounds), Some(line)) = (bounds.as_ref(), layout.as_ref()) else {
        return 0;
    };
    if position.x <= bounds.left() {
        return 0;
    }
    if position.x >= bounds.right() {
        return state.content.len();
    }
    let x = position.x - bounds.left() + px(state.scroll.get());
    line.closest_index_for_x(x).min(state.content.len())
}

pub(super) fn bounds_for_utf16(
    state: &FieldState,
    range_utf16: Range<usize>,
    element_bounds: Bounds<Pixels>,
) -> Option<Bounds<Pixels>> {
    let layout = state.layout.borrow();
    let line = layout.as_ref()?;
    let range = editing::range_from_utf16(&state.content, &range_utf16);
    let scroll = px(state.scroll.get());
    Some(Bounds::from_corners(
        point(
            element_bounds.left() + line.x_for_index(range.start) - scroll,
            element_bounds.top(),
        ),
        point(
            element_bounds.left() + line.x_for_index(range.end) - scroll,
            element_bounds.bottom(),
        ),
    ))
}

pub(super) fn layout_field(
    state: &FieldState,
    style: &LineStyle,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) -> FieldPrepaint {
    let content = state.content.clone();
    let selected = state.selection.clone();
    let cursor = state.cursor();
    let marked = state.marked.clone();
    let show_placeholder = content.is_empty();
    let display: SharedString = if show_placeholder {
        style.placeholder.clone()
    } else {
        content.clone()
    };
    let color = if show_placeholder {
        style.placeholder_color
    } else {
        style.text_color
    };
    let font = field_font(&style.font_family, style.font_weight);
    let run = |len: usize, color: Color, underline: bool| TextRun {
        len,
        font: font.clone(),
        color: Hsla::from(color),
        background_color: None,
        underline: underline.then(|| UnderlineStyle {
            color: Some(Hsla::from(color)),
            thickness: px(1.0),
            wavy: false,
        }),
        strikethrough: None,
    };
    let runs = if !show_placeholder && let Some(marked) = marked.as_ref() {
        vec![
            run(marked.start, color, false),
            run(marked.end.saturating_sub(marked.start), color, true),
            run(display.len().saturating_sub(marked.end), color, false),
        ]
        .into_iter()
        .filter(|run| run.len > 0)
        .collect()
    } else {
        vec![run(display.len().max(0), color, false)]
    };
    let line = window
        .text_system()
        .shape_line(display, px(style.font_size), &runs, None);

    let mut scroll = px(state.scroll.get());
    if !show_placeholder {
        let caret_x = line.x_for_index(cursor.min(content.len()));
        if caret_x > scroll + bounds.size.width - px(CARET_WIDTH) {
            scroll = caret_x - bounds.size.width + px(CARET_WIDTH);
        }
        if caret_x < scroll {
            scroll = caret_x;
        }
        if scroll < px(0.0) {
            scroll = px(0.0);
        }
    } else {
        scroll = px(0.0);
    }
    state.scroll.set(scroll / px(1.0));

    let (selection, caret) = if style.focused && !style.disabled && !show_placeholder {
        if selected.is_empty() {
            let x = bounds.left() + line.x_for_index(cursor) - scroll;
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(x, bounds.top()),
                        size(px(CARET_WIDTH), bounds.size.height),
                    ),
                    Hsla::from(style.caret_color),
                )),
            )
        } else {
            let start = bounds.left() + line.x_for_index(selected.start) - scroll;
            let end = bounds.left() + line.x_for_index(selected.end) - scroll;
            (
                Some(fill(
                    Bounds::from_corners(point(start, bounds.top()), point(end, bounds.bottom())),
                    Hsla::from(style.selection_color),
                )),
                None,
            )
        }
    } else {
        (None, None)
    };

    if show_placeholder {
        *state.layout.borrow_mut() = None;
    } else {
        // Re-shape the content so mouse hit-testing matches the stored line
        // after this one is moved into prepaint state below.
        let content_runs = vec![run(content.len(), style.text_color, false)];
        let stored =
            window
                .text_system()
                .shape_line(content, px(style.font_size), &content_runs, None);
        *state.layout.borrow_mut() = Some(stored);
    }
    *state.text_bounds.borrow_mut() = Some(bounds);

    FieldPrepaint {
        line: Some(line),
        caret,
        selection,
        scroll,
    }
}

fn field_font(family: &SharedString, weight: Weight) -> gpui::Font {
    let mut font = gpui::font(family.clone());
    font.weight = weight.into();
    font
}
