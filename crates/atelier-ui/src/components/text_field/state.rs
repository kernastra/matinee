//! Controlled buffer for [`super::TextField`].

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gpui::{App, Bounds, Context, FocusHandle, Pixels, SharedString, Window};

use crate::editing::sanitize_single_line;

pub(super) type ChangeHandler = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
pub(super) type FocusHandler = Rc<dyn Fn(bool, &mut App)>;
pub(super) type TrailingHandler = Rc<dyn Fn(&mut Window, &mut App)>;

pub(super) struct FieldState {
    pub(super) content: SharedString,
    pub(super) selection: Range<usize>,
    pub(super) reversed: bool,
    pub(super) marked: Option<Range<usize>>,
    pub(super) focus: FocusHandle,
    pub(super) disabled: bool,
    pub(super) masked: bool,
    pub(super) layout: RefCell<Option<gpui::ShapedLine>>,
    pub(super) text_bounds: RefCell<Option<Bounds<Pixels>>>,
    pub(super) scroll: Cell<f32>,
    pub(super) selecting: Cell<bool>,
    pub(super) reported_focus: Cell<Option<bool>>,
    pub(super) on_change: Option<ChangeHandler>,
    pub(super) on_focus_change: Option<FocusHandler>,
}

impl FieldState {
    pub(super) fn new(value: String, cx: &mut Context<Self>) -> Self {
        Self {
            content: value.into(),
            selection: 0..0,
            reversed: false,
            marked: None,
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            disabled: false,
            masked: false,
            layout: RefCell::new(None),
            text_bounds: RefCell::new(None),
            scroll: Cell::new(0.0),
            selecting: Cell::new(false),
            reported_focus: Cell::new(None),
            on_change: None,
            on_focus_change: None,
        }
    }

    /// Apply the controlled value unless a composition is in progress.
    pub(super) fn sync_external(&mut self, value: &str) {
        if self.marked.is_some() {
            return;
        }
        let value = sanitize_single_line(value);
        if self.content.as_ref() == value {
            return;
        }
        self.content = value.into();
        let len = self.content.len();
        self.selection = self.selection.start.min(len)..self.selection.end.min(len);
        if self.selection.end < self.selection.start {
            self.selection = self.selection.end..self.selection.start;
        }
    }

    pub(super) fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }
}
