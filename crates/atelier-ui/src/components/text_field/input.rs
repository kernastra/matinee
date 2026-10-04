//! Editing actions, clipboard, and the GPUI input handler.

use std::ops::Range;

use gpui::{
    Bounds, ClipboardItem, Context, Entity, EntityInputHandler, InteractiveElement, MouseDownEvent,
    MouseMoveEvent, Pixels, Point, UTF16Selection, Window,
};

use crate::{
    components::keybindings::*,
    editing::{self, sanitize_single_line},
};

use super::{
    layout::{self, bounds_for_utf16},
    state::FieldState,
};

pub(super) fn bind_editing(
    el: gpui::Stateful<gpui::Div>,
    model: Entity<FieldState>,
) -> gpui::Stateful<gpui::Div> {
    el.on_action({
        let model = model.clone();
        move |_: &Backspace, window, cx| {
            model.update(cx, |state, cx| state.backspace(window, cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &Delete, window, cx| {
            model.update(cx, |state, cx| state.delete(window, cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &MoveLeft, _, cx| {
            model.update(cx, |state, cx| state.move_left(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &MoveRight, _, cx| {
            model.update(cx, |state, cx| state.move_right(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &SelectLeft, _, cx| {
            model.update(cx, |state, cx| state.select_left(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &SelectRight, _, cx| {
            model.update(cx, |state, cx| state.select_right(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &LineStart, _, cx| {
            model.update(cx, |state, cx| state.move_to(0, cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &LineEnd, _, cx| {
            model.update(cx, |state, cx| state.move_end(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &SelectToStart, _, cx| {
            model.update(cx, |state, cx| state.select_to(0, cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &SelectToEnd, _, cx| {
            model.update(cx, |state, cx| state.select_end(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &MoveWordLeft, _, cx| {
            model.update(cx, |state, cx| state.move_word_left(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &MoveWordRight, _, cx| {
            model.update(cx, |state, cx| state.move_word_right(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &SelectWordLeft, _, cx| {
            model.update(cx, |state, cx| state.select_word_left(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &SelectWordRight, _, cx| {
            model.update(cx, |state, cx| state.select_word_right(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &SelectAll, _, cx| {
            model.update(cx, |state, cx| state.select_all(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &Paste, window, cx| {
            model.update(cx, |state, cx| state.paste(window, cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &Copy, _, cx| {
            model.update(cx, |state, cx| state.copy(cx));
        }
    })
    .on_action({
        let model = model.clone();
        move |_: &Cut, window, cx| {
            model.update(cx, |state, cx| state.cut(window, cx));
        }
    })
    .on_action(|_: &ShowCharacterPalette, window, _| {
        window.show_character_palette();
    })
}

impl FieldState {
    pub(super) fn mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.selecting.set(true);
        let index = self.index_for_position(event.position);
        if event.click_count >= 3 {
            self.select_all(cx);
        } else if event.click_count == 2 {
            let bounds = editing::word_bounds(&self.content, index);
            self.reversed = false;
            self.selection = bounds;
            cx.notify();
        } else if event.modifiers.shift {
            self.select_to(index, cx);
        } else {
            self.move_to(index, cx);
        }
    }

    pub(super) fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.selecting.get() && !self.disabled {
            self.select_to(self.index_for_position(event.position), cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.selection = offset..offset;
        self.reversed = false;
        cx.notify();
    }

    fn move_end(&mut self, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        if self.reversed {
            self.selection.start = offset;
        } else {
            self.selection.end = offset;
        }
        if self.selection.end < self.selection.start {
            self.reversed = !self.reversed;
            self.selection = self.selection.end..self.selection.start;
        }
        cx.notify();
    }

    fn select_end(&mut self, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }

    fn move_left(&mut self, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            self.move_to(editing::previous_grapheme(&self.content, self.cursor()), cx);
        } else {
            self.move_to(self.selection.start, cx);
        }
    }

    fn move_right(&mut self, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            self.move_to(editing::next_grapheme(&self.content, self.cursor()), cx);
        } else {
            self.move_to(self.selection.end, cx);
        }
    }

    fn select_left(&mut self, cx: &mut Context<Self>) {
        self.select_to(editing::previous_grapheme(&self.content, self.cursor()), cx);
    }

    fn select_right(&mut self, cx: &mut Context<Self>) {
        self.select_to(editing::next_grapheme(&self.content, self.cursor()), cx);
    }

    fn move_word_left(&mut self, cx: &mut Context<Self>) {
        self.move_to(editing::previous_word(&self.content, self.cursor()), cx);
    }

    fn move_word_right(&mut self, cx: &mut Context<Self>) {
        self.move_to(editing::next_word(&self.content, self.cursor()), cx);
    }

    fn select_word_left(&mut self, cx: &mut Context<Self>) {
        self.select_to(editing::previous_word(&self.content, self.cursor()), cx);
    }

    fn select_word_right(&mut self, cx: &mut Context<Self>) {
        self.select_to(editing::next_word(&self.content, self.cursor()), cx);
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.reversed = false;
        self.selection = 0..self.content.len();
        cx.notify();
    }

    fn backspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if self.selection.is_empty() {
            self.select_to(editing::previous_grapheme(&self.content, self.cursor()), cx);
        }
        self.replace(None, "", window, cx);
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if self.selection.is_empty() {
            self.select_to(editing::next_grapheme(&self.content, self.cursor()), cx);
        }
        self.replace(None, "", window, cx);
    }

    fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace(None, &text, window, cx);
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        // A masked field must not place its value on the clipboard.
        if self.masked || self.selection.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.content[self.selection.clone()].to_string(),
        ));
    }

    fn cut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled || self.masked || self.selection.is_empty() {
            return;
        }
        self.copy(cx);
        self.replace(None, "", window, cx);
    }

    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        layout::index_for_position(self, position)
    }

    fn target_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .as_ref()
            .map(|range| editing::range_from_utf16(&self.content, range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone())
    }

    fn replace(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let new_text = sanitize_single_line(new_text);
        let range = self.target_range(range_utf16);
        self.splice(range, &new_text, None);
        self.emit(window, cx);
    }

    fn splice(&mut self, range: Range<usize>, new_text: &str, selection: Option<Range<usize>>) {
        let start = range.start.min(self.content.len());
        let end = range.end.min(self.content.len()).max(start);
        let mut next = String::with_capacity(self.content.len() - (end - start) + new_text.len());
        next.push_str(&self.content[..start]);
        next.push_str(new_text);
        next.push_str(&self.content[end..]);
        self.content = next.into();
        self.selection = selection.unwrap_or_else(|| {
            let caret = start + new_text.len();
            caret..caret
        });
        self.reversed = false;
    }

    fn emit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        if let Some(handler) = self.on_change.clone() {
            handler(self.content.clone(), window, cx);
        }
        cx.notify();
    }
}

impl EntityInputHandler for FieldState {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = editing::range_from_utf16(&self.content, &range_utf16);
        *adjusted = Some(editing::range_to_utf16(&self.content, &range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.disabled {
            return None;
        }
        Some(UTF16Selection {
            range: editing::range_to_utf16(&self.content, &self.selection),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|range| editing::range_to_utf16(&self.content, range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(range_utf16, text, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let new_text = sanitize_single_line(new_text);
        let range = self.target_range(range_utf16);
        let selection = new_selected_utf16.as_ref().map(|sel| {
            let relative = editing::range_from_utf16(&new_text, sel);
            let start = range.start + relative.start.min(new_text.len());
            let end = range.start + relative.end.min(new_text.len());
            start.min(end)..start.max(end)
        });
        self.splice(range.clone(), &new_text, selection);
        self.marked = if new_text.is_empty() {
            None
        } else {
            Some(range.start..range.start + new_text.len())
        };
        if let Some(handler) = self.on_change.clone() {
            handler(self.content.clone(), window, cx);
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        bounds_for_utf16(self, range_utf16, element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(editing::offset_to_utf16(
            &self.content,
            self.index_for_position(point),
        ))
    }
}
