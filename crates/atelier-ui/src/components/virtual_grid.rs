//! Virtualized grid: fixed-size cells in responsive columns, of which only
//! the visible rows and a small overscan are built.
//!
//! The grid is for collections far larger than a window. A thousand items
//! or ten thousand cost the same to draw: the content is one tall element
//! whose height comes from arithmetic, and only the cells near the viewport
//! exist as elements. Everything that decides what is near the viewport is
//! the pure [`GridLayout`], so it is tested without a window.
//!
//! - **Columns** follow the available width: as many cells of at least
//!   [`GridSizing::min_cell_width`] as fit, then widened up to
//!   [`GridSizing::max_cell_width`]. Height follows width.
//! - **Focus** is one tab stop for the whole grid. The focused cell is a
//!   logical index kept in [`VirtualGridState`], not a focus handle per
//!   cell, so focus survives its cell leaving the rendered window. Arrow
//!   keys, Home, End, Page Up, and Page Down move it and scroll the least
//!   amount that shows it. Enter and Space activate it. A click focuses the
//!   grid, moves the logical focus to that cell, and activates it. A key
//!   that cannot move any further (Up on the first row) is reported to the
//!   owner, which may hand focus to a control beside the grid.
//! - **Resize** keeps the focused cell (or the first visible one) where it
//!   was on screen when the column count changes.
//! - **Owners** read the same arithmetic through
//!   [`VirtualGridState::frame`] to learn which indices are visible, so they
//!   can load data or images for exactly those.
//!
//! The grid knows nothing about what a cell shows. The owner draws each
//! cell from its index; [`GridCell::focused`] says whether to draw focus.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, RenderOnce, StatefulInteractiveElement, Styled, Window, canvas, div, point, px,
};

use crate::{
    components::{
        ScrollControl,
        keybindings::{
            Activate, GRID_CONTEXT, NudgeDown, NudgeLeft, NudgePageDown, NudgePageUp, NudgeRight,
            NudgeToEnd, NudgeToStart, NudgeUp,
        },
    },
    focus::{self, focus_visible},
    tokens::Space,
};

/// Cell and spacing measurements, in points. Everything except the column
/// count and the stretched cell width is fixed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridSizing {
    /// The narrowest a cell may be. Decides how many columns fit.
    pub min_cell_width: f32,
    /// Cells widen to fill the row, up to this.
    pub max_cell_width: f32,
    /// Cell height is `width * aspect + extra_height`.
    pub aspect: f32,
    /// Fixed height below the proportional part (a caption, say).
    pub extra_height: f32,
    pub column_gap: f32,
    pub row_gap: f32,
    /// Space left and right of the cells.
    pub inset_x: f32,
    pub inset_top: f32,
    pub inset_bottom: f32,
    /// Never more columns than this, however wide the window.
    pub max_columns: usize,
    /// Rows built beyond each edge of the viewport.
    pub overscan_rows: usize,
}

impl Default for GridSizing {
    fn default() -> Self {
        Self {
            min_cell_width: 120.0,
            max_cell_width: 180.0,
            aspect: 1.0,
            extra_height: 0.0,
            column_gap: Space::S4.value(),
            row_gap: Space::S4.value(),
            inset_x: Space::S4.value(),
            inset_top: Space::S4.value(),
            inset_bottom: Space::S4.value(),
            max_columns: 12,
            overscan_rows: 1,
        }
    }
}

impl GridSizing {
    /// How many columns fit in `width`.
    pub fn columns(&self, width: f32) -> usize {
        let usable = (width - 2.0 * self.inset_x).max(0.0);
        let pitch = self.min_cell_width + self.column_gap;
        let fit = ((usable + self.column_gap) / pitch).floor();
        let fit = if fit.is_finite() { fit as usize } else { 1 };
        fit.clamp(1, self.max_columns.max(1))
    }
}

/// A movement of the focused cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridStep {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
}

/// The grid's arithmetic for one width and item count. Pure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLayout {
    pub sizing: GridSizing,
    pub item_count: usize,
    pub columns: usize,
    pub rows: usize,
    pub cell_width: f32,
    pub cell_height: f32,
    /// Height of the trailing area below the last row.
    pub footer_height: f32,
}

impl GridLayout {
    pub fn new(sizing: GridSizing, width: f32, item_count: usize, footer_height: f32) -> Self {
        let columns = sizing.columns(width);
        let usable = (width - 2.0 * sizing.inset_x).max(0.0);
        let stretched = (usable - sizing.column_gap * (columns as f32 - 1.0)) / columns as f32;
        let cell_width = stretched
            .min(sizing.max_cell_width)
            .max(sizing.min_cell_width.min(usable.max(1.0)));
        let cell_height = cell_width * sizing.aspect + sizing.extra_height;
        Self {
            sizing,
            item_count,
            columns,
            rows: item_count.div_ceil(columns),
            cell_width,
            cell_height,
            footer_height: footer_height.max(0.0),
        }
    }

    /// Distance from one row's top to the next.
    pub fn row_pitch(&self) -> f32 {
        self.cell_height + self.sizing.row_gap
    }

    pub fn row_of(&self, index: usize) -> usize {
        index / self.columns
    }

    /// Top-left of a cell inside the content.
    pub fn origin(&self, index: usize) -> (f32, f32) {
        let column = index % self.columns;
        let row = self.row_of(index);
        (
            self.sizing.inset_x + column as f32 * (self.cell_width + self.sizing.column_gap),
            self.row_top(row),
        )
    }

    fn row_top(&self, row: usize) -> f32 {
        self.sizing.inset_top + row as f32 * self.row_pitch()
    }

    /// Where the footer starts: below the last row.
    pub fn footer_top(&self) -> f32 {
        if self.rows == 0 {
            self.sizing.inset_top
        } else {
            self.row_top(self.rows - 1) + self.cell_height + self.sizing.row_gap
        }
    }

    pub fn content_height(&self) -> f32 {
        self.footer_top() + self.footer_height + self.sizing.inset_bottom
    }

    /// The largest scroll distance for a viewport this tall.
    pub fn max_scroll(&self, viewport_height: f32) -> f32 {
        (self.content_height() - viewport_height).max(0.0)
    }

    /// Rows that intersect the viewport, partially visible ones included.
    pub fn visible_rows(&self, scroll_top: f32, viewport_height: f32) -> Range<usize> {
        if self.rows == 0 || viewport_height <= 0.0 {
            return 0..0;
        }
        let pitch = self.row_pitch().max(1.0);
        let top = (scroll_top - self.sizing.inset_top).max(0.0);
        let bottom = (scroll_top + viewport_height - self.sizing.inset_top).max(0.0);
        let first = ((top + self.sizing.row_gap) / pitch).floor() as usize;
        let last = (bottom / pitch).ceil() as usize;
        let first = first.min(self.rows);
        first..last.clamp(first, self.rows)
    }

    fn rows_to_items(&self, rows: Range<usize>) -> Range<usize> {
        (rows.start * self.columns).min(self.item_count)
            ..(rows.end * self.columns).min(self.item_count)
    }

    /// Item indices that intersect the viewport.
    pub fn visible(&self, scroll_top: f32, viewport_height: f32) -> Range<usize> {
        self.rows_to_items(self.visible_rows(scroll_top, viewport_height))
    }

    /// Item indices to build: the visible rows plus the overscan rows.
    pub fn materialized(&self, scroll_top: f32, viewport_height: f32) -> Range<usize> {
        let rows = self.visible_rows(scroll_top, viewport_height);
        let overscan = self.sizing.overscan_rows;
        let start = rows.start.saturating_sub(overscan);
        let end = (rows.end + overscan).min(self.rows);
        self.rows_to_items(start..end.max(start))
    }

    /// Whole rows that fit in the viewport, at least one.
    pub fn rows_per_page(&self, viewport_height: f32) -> usize {
        ((viewport_height + self.sizing.row_gap) / self.row_pitch().max(1.0))
            .floor()
            .max(1.0) as usize
    }

    /// The scroll distance that shows `index` with the least movement.
    pub fn reveal(&self, index: usize, scroll_top: f32, viewport_height: f32) -> f32 {
        if index >= self.item_count {
            return scroll_top;
        }
        let (_, top) = self.origin(index);
        let bottom = top + self.cell_height;
        let margin_top = self.sizing.inset_top.min(self.sizing.row_gap.max(0.0));
        let margin_bottom = self.sizing.row_gap.max(0.0);
        let target = if top - margin_top < scroll_top {
            top - margin_top
        } else if bottom + margin_bottom > scroll_top + viewport_height {
            bottom + margin_bottom - viewport_height
        } else {
            scroll_top
        };
        let row = self.row_of(index);
        let target = if row == 0 { 0.0 } else { target };
        target.clamp(0.0, self.max_scroll(viewport_height))
    }

    /// Where focus goes from `index`. Movement does not wrap. Down from a
    /// row above a shorter final row lands on the last item.
    pub fn step(
        &self,
        index: Option<usize>,
        step: GridStep,
        rows_per_page: usize,
    ) -> Option<usize> {
        if self.item_count == 0 {
            return None;
        }
        let last = self.item_count - 1;
        let Some(index) = index.map(|index| index.min(last)) else {
            return Some(match step {
                GridStep::Last => last,
                _ => 0,
            });
        };
        let columns = self.columns;
        let down = |rows: usize| {
            let target = index + rows * columns;
            if target <= last {
                target
            } else if self.row_of(last) > self.row_of(index) {
                last
            } else {
                index
            }
        };
        Some(match step {
            GridStep::Left if index % columns > 0 => index - 1,
            GridStep::Left => index,
            GridStep::Right if index % columns + 1 < columns && index < last => index + 1,
            GridStep::Right => index,
            GridStep::Up => index.checked_sub(columns).unwrap_or(index),
            GridStep::Down => down(1),
            GridStep::PageUp => {
                let rows = rows_per_page.max(1).min(self.row_of(index));
                index - rows * columns
            }
            GridStep::PageDown => down(rows_per_page.max(1)),
            GridStep::First => 0,
            GridStep::Last => last,
        })
    }

    /// The scroll distance that keeps `anchor` at the same height on screen
    /// after the layout changed from `before` (at `scroll_before`) to this.
    pub fn anchored_scroll(
        &self,
        before: &GridLayout,
        scroll_before: f32,
        anchor: usize,
        viewport_height: f32,
    ) -> f32 {
        if anchor >= self.item_count || anchor >= before.item_count {
            return scroll_before.clamp(0.0, self.max_scroll(viewport_height));
        }
        let on_screen = before.origin(anchor).1 - scroll_before;
        (self.origin(anchor).1 - on_screen).clamp(0.0, self.max_scroll(viewport_height))
    }
}

/// What the grid shows for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct GridViewport {
    pub layout: GridLayout,
    pub scroll_top: f32,
    pub viewport_height: f32,
    /// Items intersecting the viewport.
    pub visible: Range<usize>,
    /// Items built: visible plus overscan.
    pub materialized: Range<usize>,
}

/// What a cell needs to know to draw itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridCell {
    pub index: usize,
    /// Draw the keyboard focus ring: the grid is focused, this is its
    /// focused cell, and the last interaction was the keyboard.
    pub focused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    layout: GridLayout,
    scroll_top: f32,
    viewport: (f32, f32),
}

struct Inner {
    scroll: ScrollControl,
    focus: FocusHandle,
    focused: Cell<Option<usize>>,
    /// Scroll the focused cell into view on the next frame.
    reveal: Cell<bool>,
    /// The viewport size measured at the last paint.
    measured: Cell<Option<(f32, f32)>>,
    /// The layout and offset of the last frame, for resize anchoring.
    last: RefCell<Option<Frame>>,
    /// Cells built by the last render.
    rendered: Cell<usize>,
}

/// Scroll offset, focused cell, and measurements for one [`VirtualGrid`].
/// Clone is cheap; clones share the same state. Keep it with the owner so
/// it survives the grid not being drawn for a while.
#[derive(Clone)]
pub struct VirtualGridState {
    inner: Rc<Inner>,
}

impl VirtualGridState {
    pub fn new(cx: &mut App) -> Self {
        Self {
            inner: Rc::new(Inner {
                scroll: ScrollControl::new(),
                focus: cx.focus_handle().tab_index(0).tab_stop(true),
                focused: Cell::new(None),
                reveal: Cell::new(false),
                measured: Cell::new(None),
                last: RefCell::new(None),
                rendered: Cell::new(0),
            }),
        }
    }

    pub fn scroll(&self) -> &ScrollControl {
        &self.inner.scroll
    }

    /// The grid's one tab stop.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.inner.focus
    }

    /// The logically focused cell, kept while it is not rendered.
    pub fn focused(&self) -> Option<usize> {
        self.inner.focused.get()
    }

    /// Move the logical focus and scroll it into view on the next frame.
    pub fn focus_index(&self, index: Option<usize>) {
        self.inner.focused.set(index);
        self.inner.reveal.set(index.is_some());
    }

    /// Back to the top with nothing focused, for a new collection.
    pub fn reset(&self) {
        self.inner.focused.set(None);
        self.inner.reveal.set(false);
        self.inner.last.replace(None);
        self.inner
            .scroll
            .handle()
            .set_offset(point(px(0.0), px(0.0)));
    }

    /// How many cells the last render built.
    pub fn rendered_cells(&self) -> usize {
        self.inner.rendered.get()
    }

    /// The viewport size measured at the last paint, if any.
    pub fn measured(&self) -> Option<(f32, f32)> {
        self.inner.measured.get()
    }

    /// Lay out this frame and decide what is visible. Applies resize
    /// anchoring and a pending reveal, so owners and the grid agree;
    /// calling it again with the same inputs changes nothing. `fallback`
    /// is the viewport size to assume before the grid has been measured.
    pub fn frame(
        &self,
        sizing: GridSizing,
        item_count: usize,
        footer_height: f32,
        fallback: (f32, f32),
    ) -> GridViewport {
        let (width, height) = self.inner.measured.get().unwrap_or(fallback);
        let layout = GridLayout::new(sizing, width, item_count, footer_height);
        let mut scroll_top = -f32::from(self.inner.scroll.offset().y);
        if let Some(focused) = self.inner.focused.get()
            && focused >= item_count
        {
            self.inner
                .focused
                .set(item_count.checked_sub(1).filter(|_| item_count > 0));
        }
        let before = *self.inner.last.borrow();
        if let Some(before) = before
            && before.layout.columns != layout.columns
        {
            let anchor = self
                .inner
                .focused
                .get()
                .filter(|index| {
                    before
                        .layout
                        .visible(before.scroll_top, before.viewport.1)
                        .contains(index)
                })
                .or_else(|| {
                    let visible = before.layout.visible(before.scroll_top, before.viewport.1);
                    (!visible.is_empty()).then_some(visible.start)
                });
            if let Some(anchor) = anchor {
                scroll_top =
                    layout.anchored_scroll(&before.layout, before.scroll_top, anchor, height);
            }
        }
        if self.inner.reveal.get()
            && self.inner.measured.get().is_some()
            && let Some(focused) = self.inner.focused.get()
        {
            self.inner.reveal.set(false);
            scroll_top = layout.reveal(focused, scroll_top, height);
        }
        let scroll_top = scroll_top.clamp(0.0, layout.max_scroll(height));
        self.inner
            .scroll
            .handle()
            .set_offset(point(px(0.0), px(-scroll_top)));
        self.inner.last.replace(Some(Frame {
            layout,
            scroll_top,
            viewport: (width, height),
        }));
        GridViewport {
            visible: layout.visible(scroll_top, height),
            materialized: layout.materialized(scroll_top, height),
            layout,
            scroll_top,
            viewport_height: height,
        }
    }

    fn step(&self, step: GridStep, window: &mut Window, cx: &mut App) -> Option<usize> {
        let frame = (*self.inner.last.borrow())?;
        let rows = frame.layout.rows_per_page(frame.viewport.1);
        let next = frame.layout.step(self.focused(), step, rows)?;
        focus::note_keyboard_navigation(cx);
        self.focus_index(Some(next));
        window.refresh();
        Some(next)
    }
}

type IndexHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;
type EdgeHandler = Rc<dyn Fn(GridStep, &mut Window, &mut App)>;
type CellRenderer = Box<dyn Fn(GridCell, &mut Window, &mut App) -> AnyElement>;

/// A virtualized grid of fixed-size cells. See the module notes.
#[derive(IntoElement)]
pub struct VirtualGrid {
    id: ElementId,
    state: VirtualGridState,
    sizing: GridSizing,
    item_count: usize,
    render_cell: CellRenderer,
    footer: Option<(f32, AnyElement)>,
    fallback: Option<(f32, f32)>,
    on_activate: Option<IndexHandler>,
    on_focus: Option<IndexHandler>,
    on_edge: Option<EdgeHandler>,
}

impl VirtualGrid {
    pub fn new(
        id: impl Into<ElementId>,
        state: &VirtualGridState,
        item_count: usize,
        render_cell: impl Fn(GridCell, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            sizing: GridSizing::default(),
            item_count,
            render_cell: Box::new(render_cell),
            footer: None,
            fallback: None,
            on_activate: None,
            on_focus: None,
            on_edge: None,
        }
    }

    pub fn sizing(mut self, sizing: GridSizing) -> Self {
        self.sizing = sizing;
        self
    }

    /// Content below the last row, `height` tall (a loading line, say).
    pub fn footer(mut self, height: f32, footer: impl IntoElement) -> Self {
        self.footer = Some((height, footer.into_any_element()));
        self
    }

    /// The viewport size to assume before the first measurement. Defaults
    /// to the window's size.
    pub fn fallback_viewport(mut self, size: (f32, f32)) -> Self {
        self.fallback = Some(size);
        self
    }

    /// Enter, Space, or a click on a cell.
    pub fn on_activate(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_activate = Some(Rc::new(handler));
        self
    }

    /// The logical focus moved to another cell from the keyboard or a click.
    pub fn on_focus(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_focus = Some(Rc::new(handler));
        self
    }

    /// A movement key could not move the focus any further: Up on the first
    /// row, Left at the start of a row, and so on. The grid keeps focus
    /// unless the handler moves it, so an owner can hand focus to a control
    /// above or beside the grid.
    pub fn on_edge(mut self, handler: impl Fn(GridStep, &mut Window, &mut App) + 'static) -> Self {
        self.on_edge = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for VirtualGrid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.clone();
        let fallback = self.fallback.unwrap_or_else(|| {
            let size = window.viewport_size();
            (f32::from(size.width), f32::from(size.height))
        });
        let footer_height = self.footer.as_ref().map_or(0.0, |(height, _)| *height);
        let grid_focused = state.focus_handle().is_focused(window);
        if grid_focused && state.focused().is_none() && self.item_count > 0 {
            state.inner.focused.set(Some(0));
        }
        let viewport = state.frame(self.sizing, self.item_count, footer_height, fallback);
        let layout = viewport.layout;
        let ring = focus_visible(grid_focused, cx);
        let focused = state.focused();

        let mut cells: Vec<AnyElement> = Vec::with_capacity(viewport.materialized.len());
        for index in viewport.materialized.clone() {
            let (x, y) = layout.origin(index);
            let cell = (self.render_cell)(
                GridCell {
                    index,
                    focused: ring && focused == Some(index),
                },
                window,
                cx,
            );
            let activate = self.on_activate.clone();
            let moved = self.on_focus.clone();
            let click_state = state.clone();
            cells.push(
                div()
                    .id(ElementId::NamedInteger("grid-cell".into(), index as u64))
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(layout.cell_width))
                    .h(px(layout.cell_height))
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        focus::note_pointer_interaction(cx);
                    })
                    .on_click(move |_, window, cx| {
                        let changed = click_state.focused() != Some(index);
                        click_state.inner.focused.set(Some(index));
                        window.focus(click_state.focus_handle());
                        if changed && let Some(moved) = &moved {
                            moved(index, window, cx);
                        }
                        if let Some(activate) = &activate {
                            activate(index, window, cx);
                        }
                    })
                    .child(cell)
                    .into_any_element(),
            );
        }
        state.inner.rendered.set(cells.len());

        let footer = self.footer.map(|(height, element)| {
            div()
                .absolute()
                .left(px(layout.sizing.inset_x))
                .right(px(layout.sizing.inset_x))
                .top(px(layout.footer_top()))
                .h(px(height))
                .child(element)
        });

        let step_handler = |step: GridStep,
                            state: VirtualGridState,
                            moved: Option<IndexHandler>,
                            edge: Option<EdgeHandler>| {
            move |window: &mut Window, cx: &mut App| {
                let before = state.focused();
                match state.step(step, window, cx) {
                    Some(index) if Some(index) == before => {
                        if let Some(edge) = &edge {
                            edge(step, window, cx);
                        }
                    }
                    Some(index) => {
                        if let Some(moved) = &moved {
                            moved(index, window, cx);
                        }
                    }
                    None => {}
                }
            }
        };
        let action = |step: GridStep| {
            step_handler(
                step,
                state.clone(),
                self.on_focus.clone(),
                self.on_edge.clone(),
            )
        };
        let left = action(GridStep::Left);
        let right = action(GridStep::Right);
        let up = action(GridStep::Up);
        let down = action(GridStep::Down);
        let page_up = action(GridStep::PageUp);
        let page_down = action(GridStep::PageDown);
        let first = action(GridStep::First);
        let last = action(GridStep::Last);
        let activate = self.on_activate.clone();
        let activate_state = state.clone();
        let measure_state = state.clone();

        div()
            .id(self.id)
            .relative()
            .size_full()
            .min_h(px(0.0))
            .key_context(GRID_CONTEXT)
            .track_focus(state.focus_handle())
            .on_action(move |_: &NudgeLeft, window, cx| left(window, cx))
            .on_action(move |_: &NudgeRight, window, cx| right(window, cx))
            .on_action(move |_: &NudgeUp, window, cx| up(window, cx))
            .on_action(move |_: &NudgeDown, window, cx| down(window, cx))
            .on_action(move |_: &NudgePageUp, window, cx| page_up(window, cx))
            .on_action(move |_: &NudgePageDown, window, cx| page_down(window, cx))
            .on_action(move |_: &NudgeToStart, window, cx| first(window, cx))
            .on_action(move |_: &NudgeToEnd, window, cx| last(window, cx))
            .on_action(move |_: &Activate, window, cx| {
                if let (Some(index), Some(activate)) = (activate_state.focused(), &activate) {
                    activate(index, window, cx);
                }
            })
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let size = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                        let previous = measure_state.inner.measured.replace(Some(size));
                        // Laid out with a guess or an older size: draw the
                        // owner again with the real one, once this frame is done.
                        if previous != Some(size) {
                            let view = window.current_view();
                            cx.defer(move |cx| cx.notify(view));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .id("grid-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .scrollbar_width(Space::S2.px())
                    .track_scroll(state.scroll().handle())
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(layout.content_height()))
                            .children(cells)
                            .children(footer),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn columns_follow_the_width() {
        let sizing = sizing();
        // 640 wide: 600 usable, (600 + 10) / 110 = 5.5.
        assert_eq!(sizing.columns(640.0), 5);
        assert_eq!(sizing.columns(150.0), 1, "never fewer than one");
        assert_eq!(sizing.columns(0.0), 1);
        assert_eq!(sizing.columns(100_000.0), 12, "capped");
        let layout = GridLayout::new(sizing, 640.0, 7, 0.0);
        assert_eq!(layout.rows, 2, "a partial final row counts");
        assert!(layout.cell_width >= 100.0 && layout.cell_width <= 120.0);
        assert_eq!(layout.cell_height, layout.cell_width * 1.5 + 20.0);
    }

    /// 5 columns at 640, rows 200 tall (120 * 1.5 + 20) plus a 10 gap.
    fn five_wide(count: usize) -> GridLayout {
        let layout = GridLayout::new(sizing(), 640.0, count, 0.0);
        assert_eq!(layout.columns, 5);
        assert_eq!(layout.cell_width, 112.0);
        layout
    }

    #[test]
    fn the_visible_range_follows_the_scroll_offset() {
        let layout = five_wide(100);
        let pitch = layout.row_pitch();
        assert_eq!(pitch, 112.0 * 1.5 + 20.0 + 10.0);
        // Rows start at 10, 208, 406: a 400-tall viewport at the top
        // shows rows 0 and 1; row 2 starts just below it.
        assert_eq!(layout.visible_rows(0.0, 400.0), 0..2);
        assert_eq!(layout.visible(0.0, 400.0), 0..10);
        assert_eq!(layout.visible(10.0, 400.0), 0..15, "part of row 2");
        // Scrolled past row 0 and its gap: row 0 is gone.
        let past_first = 10.0 + pitch;
        assert_eq!(layout.visible_rows(past_first, 400.0).start, 1);
        assert_eq!(layout.visible(past_first, 400.0).start, 5);
        // The materialized window adds one row each side.
        assert_eq!(layout.materialized(past_first, 400.0).start, 0);
        let deep = 10.0 + 10.0 * pitch;
        let visible = layout.visible_rows(deep, 400.0);
        assert_eq!(visible.start, 10);
        let built = layout.materialized(deep, 400.0);
        assert_eq!(built.start, 9 * 5);
        assert_eq!(built.end, (visible.end + 1) * 5);
    }

    #[test]
    fn overscan_never_runs_past_the_ends() {
        let layout = five_wide(12);
        assert_eq!(layout.materialized(0.0, 10_000.0), 0..12);
        assert_eq!(layout.materialized(layout.max_scroll(300.0), 300.0).end, 12);
        let none = GridSizing {
            overscan_rows: 0,
            ..sizing()
        };
        let layout = GridLayout::new(none, 640.0, 100, 0.0);
        assert_eq!(
            layout.materialized(500.0, 400.0),
            layout.visible(500.0, 400.0)
        );
    }

    #[test]
    fn empty_and_single_item_grids() {
        let empty = five_wide(0);
        assert_eq!(empty.rows, 0);
        assert_eq!(empty.visible(0.0, 400.0), 0..0);
        assert_eq!(empty.materialized(0.0, 400.0), 0..0);
        assert_eq!(empty.step(None, GridStep::Down, 3), None);
        assert_eq!(empty.content_height(), 20.0, "just the insets");

        let one = five_wide(1);
        assert_eq!(one.visible(0.0, 400.0), 0..1);
        for step in [
            GridStep::Left,
            GridStep::Right,
            GridStep::Up,
            GridStep::Down,
            GridStep::PageDown,
            GridStep::Last,
        ] {
            assert_eq!(one.step(Some(0), step, 3), Some(0), "{step:?}");
        }
    }

    #[test]
    fn arrow_keys_move_by_cell_and_row_without_wrapping() {
        let layout = five_wide(23); // rows of 5, 5, 5, 5, 3
        assert_eq!(layout.step(None, GridStep::Right, 2), Some(0));
        assert_eq!(layout.step(None, GridStep::Last, 2), Some(22));
        assert_eq!(layout.step(Some(8), GridStep::Down, 2), Some(13));
        assert_eq!(layout.step(Some(8), GridStep::Up, 2), Some(3));
        assert_eq!(layout.step(Some(3), GridStep::Up, 2), Some(3), "top row");
        assert_eq!(layout.step(Some(5), GridStep::Left, 2), Some(5), "no wrap");
        assert_eq!(layout.step(Some(4), GridStep::Right, 2), Some(4), "no wrap");
        assert_eq!(layout.step(Some(6), GridStep::Left, 2), Some(5));
        // Down into the shorter final row lands on its last item.
        assert_eq!(layout.step(Some(19), GridStep::Down, 2), Some(22));
        assert_eq!(layout.step(Some(16), GridStep::Down, 2), Some(21));
        assert_eq!(
            layout.step(Some(21), GridStep::Down, 2),
            Some(21),
            "last row"
        );
        assert_eq!(layout.step(Some(22), GridStep::Right, 2), Some(22));
        assert_eq!(layout.step(Some(13), GridStep::First, 2), Some(0));
        assert_eq!(layout.step(Some(13), GridStep::Last, 2), Some(22));
        assert_eq!(layout.step(Some(2), GridStep::PageDown, 2), Some(12));
        assert_eq!(layout.step(Some(17), GridStep::PageDown, 2), Some(22));
        assert_eq!(layout.step(Some(17), GridStep::PageUp, 2), Some(7));
        assert_eq!(layout.step(Some(7), GridStep::PageUp, 2), Some(2));
        // An index past the end (items shrank) is treated as the last.
        assert_eq!(layout.step(Some(99), GridStep::Left, 2), Some(21));
    }

    #[test]
    fn reveal_scrolls_the_least_amount() {
        let layout = five_wide(100);
        let pitch = layout.row_pitch();
        // Already visible: no movement.
        assert_eq!(layout.reveal(1, 0.0, 400.0), 0.0);
        // Row 1 ends at 396; with its gap below it needs 6 more.
        assert_eq!(layout.reveal(6, 0.0, 400.0), 6.0);
        // Below: its bottom edge (plus a gap) meets the viewport's bottom.
        let target = layout.reveal(40, 0.0, 400.0);
        let (_, top) = layout.origin(40);
        assert_eq!(target, top + layout.cell_height + 10.0 - 400.0);
        assert!(layout.visible(target, 400.0).contains(&40));
        // Above: its top edge meets the viewport's top, less a gap.
        let deep = 30.0 * pitch;
        let back = layout.reveal(12, deep, 400.0);
        assert_eq!(back, layout.origin(12).1 - 10.0);
        // The first row reveals all the way to the top inset.
        assert_eq!(layout.reveal(3, deep, 400.0), 0.0);
        // The last item never scrolls past the end.
        let last = layout.reveal(99, 0.0, 400.0);
        assert!(last <= layout.max_scroll(400.0));
        assert!(layout.visible(last, 400.0).contains(&99));
    }

    #[test]
    fn resizing_keeps_the_anchor_on_screen() {
        let six = GridLayout::new(sizing(), 760.0, 1000, 0.0);
        let four = GridLayout::new(sizing(), 520.0, 1000, 0.0);
        assert_eq!(six.columns, 6);
        assert_eq!(four.columns, 4);
        let anchor = 300;
        let scroll = six.reveal(anchor, 0.0, 500.0);
        let on_screen = six.origin(anchor).1 - scroll;
        let after = four.anchored_scroll(&six, scroll, anchor, 500.0);
        assert_eq!(four.origin(anchor).1 - after, on_screen);
        assert!(four.visible(after, 500.0).contains(&anchor));
        // The logical index is not touched by the layout: still item 300.
        assert_eq!(four.row_of(anchor), 75);
        assert_eq!(six.row_of(anchor), 50);
    }

    #[test]
    fn footer_sits_below_the_last_row() {
        let layout = GridLayout::new(sizing(), 640.0, 7, 48.0);
        let (_, last_top) = layout.origin(6);
        assert_eq!(layout.footer_top(), last_top + layout.cell_height + 10.0);
        assert_eq!(layout.content_height(), layout.footer_top() + 48.0 + 10.0);
    }

    #[test]
    fn built_cells_scale_with_the_viewport_not_the_collection() {
        for count in [100, 1_000, 10_000] {
            let layout = five_wide(count);
            let mut most = 0;
            let max = layout.max_scroll(620.0);
            let mut scroll = 0.0;
            while scroll <= max {
                let built = layout.materialized(scroll, 620.0);
                most = most.max(built.len());
                scroll += 97.0;
            }
            let built = layout.materialized(max, 620.0);
            assert_eq!(built.end, count, "the end is reachable");
            // Rows are 198 apart, so 620 tall touches at most 5 rows (a
            // sliver, three whole rows, a sliver); one overscan row each
            // side makes 7 rows of 5.
            assert!(most <= 35, "{count} items built {most} cells");
        }
    }
}
