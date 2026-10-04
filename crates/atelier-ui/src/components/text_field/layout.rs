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

use super::mask::{
    content_to_mask_offset, mask_offset_to_content, masked_glyphs, snap_mask_offset,
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
    let index = line.closest_index_for_x(x);
    if state.masked {
        mask_offset_to_content(&state.content, snap_mask_offset(index))
    } else {
        index.min(state.content.len())
    }
}

pub(super) fn bounds_for_utf16(
    state: &FieldState,
    range_utf16: Range<usize>,
    element_bounds: Bounds<Pixels>,
) -> Option<Bounds<Pixels>> {
    let layout = state.layout.borrow();
    let line = layout.as_ref()?;
    let range = editing::range_from_utf16(&state.content, &range_utf16);
    let start = shaped_index(state, range.start);
    let end = shaped_index(state, range.end);
    let scroll = px(state.scroll.get());
    Some(Bounds::from_corners(
        point(
            element_bounds.left() + line.x_for_index(start) - scroll,
            element_bounds.top(),
        ),
        point(
            element_bounds.left() + line.x_for_index(end) - scroll,
            element_bounds.bottom(),
        ),
    ))
}

fn shaped_index(state: &FieldState, content_byte: usize) -> usize {
    if state.masked {
        content_to_mask_offset(&state.content, content_byte)
    } else {
        content_byte.min(state.content.len())
    }
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
    let masked = state.masked;
    let show_placeholder = content.is_empty();
    let display: SharedString = if show_placeholder {
        style.placeholder.clone()
    } else if masked {
        masked_glyphs(&content).into()
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
        let start = shaped_index(state, marked.start).min(display.len());
        let end = shaped_index(state, marked.end)
            .max(start)
            .min(display.len());
        vec![
            run(start, color, false),
            run(end.saturating_sub(start), color, true),
            run(display.len().saturating_sub(end), color, false),
        ]
        .into_iter()
        .filter(|run| run.len > 0)
        .collect()
    } else {
        vec![run(display.len(), color, false)]
    };
    let stored_text = display.clone();
    let line = window
        .text_system()
        .shape_line(display, px(style.font_size), &runs, None);

    let mut scroll = px(state.scroll.get());
    if !show_placeholder {
        let caret_x = line.x_for_index(shaped_index(state, cursor));
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
            let x = bounds.left() + line.x_for_index(shaped_index(state, cursor)) - scroll;
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
            let start =
                bounds.left() + line.x_for_index(shaped_index(state, selected.start)) - scroll;
            let end = bounds.left() + line.x_for_index(shaped_index(state, selected.end)) - scroll;
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
        // Keep the shaped display line for hit testing. A masked field's line
        // is the bullet string, and `index_for_position` maps back to the value.
        let stored_runs = vec![run(stored_text.len(), style.text_color, false)];
        let stored =
            window
                .text_system()
                .shape_line(stored_text, px(style.font_size), &stored_runs, None);
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
