//! Single-line text field.
//!
//! Editing goes through GPUI's input handler (`EntityInputHandler` +
//! `Window::handle_input`), so printable input and IME composition use the
//! platform path. Editing commands are key bindings in the `TextField`
//! context (see `keybindings`).
//!
//! The field is controlled: `value` is the source of truth whenever the
//! field is not composing. `on_change` must write the new value back or the
//! next frame restores the previous one.
//!
//! Component metrics: height 30 (aligned with medium buttons), default width
//! 280, 1px border, 1px caret. A focused field uses a 1px `focus.ring`
//! border and a caret. It does not draw [`FocusRing`]; that ring is reserved
//! for keyboard focus on controls that have no caret.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, GlobalElementId, Hsla,
    InspectorElementId, InteractiveElement, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, Pixels, Point, RenderOnce, ShapedLine, SharedString,
    StatefulInteractiveElement, Style, Styled, TextRun, UTF16Selection, UnderlineStyle, Window,
    div, fill, point, px, relative, size,
};

use crate::{
    ActiveTheme, StyledExt,
    components::{Icon, IconName, IconSize, Text, keybindings::*, v_stack},
    editing::{self, sanitize_single_line},
    focus,
    inspect::{self, Inspection},
    tokens::{Color, Radius, Space, TextRole, Weight},
};

/// Height aligned with [`super::ButtonSize::Medium`].
const FIELD_HEIGHT: f32 = 30.0;
const FIELD_WIDTH: f32 = 280.0;
const BORDER_WIDTH: f32 = 1.0;
const CARET_WIDTH: f32 = 1.0;

type ChangeHandler = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type FocusHandler = Rc<dyn Fn(bool, &mut App)>;
type TrailingHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// A single-line text field.
#[derive(IntoElement)]
pub struct TextField {
    id: ElementId,
    value: SharedString,
    placeholder: SharedString,
    label: Option<SharedString>,
    supporting_text: Option<SharedString>,
    invalid: bool,
    disabled: bool,
    leading_icon: Option<IconName>,
    trailing_icon: Option<IconName>,
    trailing_label: Option<SharedString>,
    trailing_action: Option<TrailingHandler>,
    on_change: Option<ChangeHandler>,
    on_focus_change: Option<FocusHandler>,
    inspected: bool,
    inspection_name: &'static str,
    key_context: &'static str,
}

impl TextField {
    pub fn new(id: impl Into<ElementId>, value: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            value: value.into(),
            placeholder: SharedString::default(),
            label: None,
            supporting_text: None,
            invalid: false,
            disabled: false,
            leading_icon: None,
            trailing_icon: None,
            trailing_label: None,
            trailing_action: None,
            on_change: None,
            on_focus_change: None,
            inspected: false,
            inspection_name: "Text field",
            key_context: TEXT_FIELD_CONTEXT,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn supporting_text(mut self, text: impl Into<SharedString>) -> Self {
        self.supporting_text = Some(text.into());
        self
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn leading_icon(mut self, icon: IconName) -> Self {
        self.leading_icon = Some(icon);
        self
    }

    /// A pointer-only trailing icon. It is not a tab stop and does not take
    /// keyboard focus away from the field.
    pub fn trailing(
        mut self,
        icon: IconName,
        label: impl Into<SharedString>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.trailing_icon = Some(icon);
        self.trailing_label = Some(label.into());
        self.trailing_action = Some(Rc::new(action));
        self
    }

    pub fn on_change(
        mut self,
        handler: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    pub fn on_focus_change(mut self, handler: impl Fn(bool, &mut App) + 'static) -> Self {
        self.on_focus_change = Some(Rc::new(handler));
        self
    }

    /// Report this field to the Gallery inspector.
    pub fn inspected(mut self, inspected: bool) -> Self {
        self.inspected = inspected;
        self
    }

    pub fn inspection_name(mut self, name: &'static str) -> Self {
        self.inspection_name = name;
        self
    }

    pub(crate) fn key_context(mut self, context: &'static str) -> Self {
        self.key_context = context;
        self
    }
}

impl RenderOnce for TextField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let model = window.use_keyed_state(self.id.clone(), cx, {
            let initial = sanitize_single_line(&self.value);
            move |_, cx| FieldState::new(initial, cx)
        });

        let focused = model.read(cx).focus.is_focused(window) && !self.disabled;
        model.update(cx, |state, cx| {
            state.disabled = self.disabled;
            state.on_change.clone_from(&self.on_change);
            state.on_focus_change.clone_from(&self.on_focus_change);
            if !self.disabled {
                state.sync_external(&self.value);
            }
            let reported = state.reported_focus.get();
            if reported != Some(focused) {
                state.reported_focus.set(Some(focused));
                if let Some(handler) = state.on_focus_change.clone() {
                    cx.defer(move |cx| handler(focused, cx));
                }
            }
        });

        if self.inspected {
            inspect::report_inspection(Inspection {
                name: self.inspection_name,
                focused,
                value: self.value.to_string(),
            });
        }

        let colors = &theme.colors;
        let border = if self.invalid {
            colors.control.destructive
        } else if focused {
            colors.focus.ring
        } else {
            colors.border.default
        };
        let hover_border = if self.invalid {
            colors.control.destructive
        } else if focused {
            colors.focus.ring
        } else {
            colors.border.strong
        };
        let text_color = if self.disabled {
            colors.text.disabled
        } else {
            colors.text.primary
        };
        let supporting_color = if self.invalid && !self.disabled {
            colors.text.danger
        } else {
            colors.text.muted
        };
        let style = theme.typography.style(TextRole::Body);
        let family = theme.typography.family(TextRole::Body);
        let radius = theme.radius.get(Radius::Medium);
        let focus_handle = model.read(cx).focus.clone();

        let field = div()
            .id(self.id.clone())
            .key_context(self.key_context)
            .when(!self.disabled, |this| this.track_focus(&focus_handle))
            .w(px(FIELD_WIDTH))
            .h(px(FIELD_HEIGHT))
            .px(Space::S2.px())
            .flex()
            .flex_row()
            .items_center()
            .gap(Space::S2.px())
            .rounded(px(radius))
            .bg(if self.disabled {
                colors.control.disabled
            } else {
                colors.surface.elevated
            })
            .border(px(BORDER_WIDTH))
            .border_color(border)
            .when(!self.disabled, |this| {
                this.cursor(CursorStyle::IBeam)
                    .hover(move |style| style.border_color(hover_border))
            })
            .when(!self.disabled, |this| bind_editing(this, model.clone()))
            .when(!self.disabled, |this| {
                let down = model.clone();
                let up = model.clone();
                let up_out = model.clone();
                let moved = model.clone();
                this.on_mouse_down(MouseButton::Left, move |event, _, cx| {
                    focus::note_pointer_interaction(cx);
                    down.update(cx, |state, cx| state.mouse_down(event, cx));
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    up.update(cx, |state, _| state.selecting.set(false));
                })
                .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                    up_out.update(cx, |state, _| state.selecting.set(false));
                })
                .on_mouse_move(move |event: &MouseMoveEvent, _, cx| {
                    moved.update(cx, |state, cx| state.mouse_move(event, cx));
                })
            })
            .when_some(self.leading_icon, |this, icon| {
                this.child(
                    Icon::new(icon)
                        .size(IconSize::Small)
                        .color(if self.disabled {
                            colors.text.disabled
                        } else {
                            colors.text.muted
                        }),
                )
            })
            .child(FieldText {
                model: model.clone(),
                placeholder: self.placeholder.clone(),
                font_family: family.to_string().into(),
                font_weight: style.weight,
                font_size: style.size,
                line_height: (FIELD_HEIGHT - BORDER_WIDTH * 2.0).min(style.line_height),
                text_color,
                placeholder_color: colors.text.muted,
                selection_color: colors.focus.ring.with_alpha(0.35),
                caret_color: colors.text.primary,
                focused,
                disabled: self.disabled,
            })
            .when_some(
                self.trailing_icon.zip(self.trailing_label),
                |this, (icon, label)| {
                    let action = self.trailing_action.clone();
                    this.child(trailing_button(
                        icon,
                        label,
                        colors.text.secondary,
                        theme.radius.get(Radius::Small),
                        action,
                    ))
                },
            );

        v_stack(Space::S1)
            .when_some(self.label, |this, label| {
                this.child(
                    Text::new(label)
                        .role(TextRole::Label)
                        .color(if self.disabled {
                            colors.text.disabled
                        } else {
                            colors.text.secondary
                        }),
                )
            })
            .child(field)
            .when_some(self.supporting_text, |this, text| {
                this.child(
                    Text::new(text)
                        .role(TextRole::Caption)
                        .color(supporting_color),
                )
            })
    }
}

fn trailing_button(
    icon: IconName,
    label: SharedString,
    color: Color,
    radius: f32,
    action: Option<TrailingHandler>,
) -> impl IntoElement {
    div()
        .id("text-field-trailing")
        .flex_none()
        .size(px(20.0))
        .rounded(px(radius))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .tooltip(move |_, cx| trailing_tooltip(label.clone(), cx))
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            focus::note_pointer_interaction(cx);
        })
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            if let Some(action) = action.clone() {
                action(window, cx);
            }
        })
        .child(Icon::new(icon).size(IconSize::Small).color(color))
}

fn trailing_tooltip(text: SharedString, cx: &mut App) -> gpui::AnyView {
    cx.new(|_| TrailingTooltip { text }).into()
}

struct TrailingTooltip {
    text: SharedString,
}

impl gpui::Render for TrailingTooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .px(Space::S2.px())
            .py(Space::S1.px())
            .corner_radius(theme, Radius::Small)
            .bg(theme.colors.surface.elevated)
            .border_1()
            .border_color(theme.colors.border.default)
            .child(Text::new(self.text.clone()).role(TextRole::Caption))
    }
}

fn bind_editing(
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

struct FieldState {
    content: SharedString,
    selection: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    focus: FocusHandle,
    disabled: bool,
    layout: RefCell<Option<ShapedLine>>,
    text_bounds: RefCell<Option<Bounds<Pixels>>>,
    scroll: Cell<f32>,
    selecting: Cell<bool>,
    reported_focus: Cell<Option<bool>>,
    on_change: Option<ChangeHandler>,
    on_focus_change: Option<FocusHandler>,
}

impl FieldState {
    fn new(value: String, cx: &mut Context<Self>) -> Self {
        Self {
            content: value.into(),
            selection: 0..0,
            reversed: false,
            marked: None,
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            disabled: false,
            layout: RefCell::new(None),
            text_bounds: RefCell::new(None),
            scroll: Cell::new(0.0),
            selecting: Cell::new(false),
            reported_focus: Cell::new(None),
            on_change: None,
            on_focus_change: None,
        }
    }

    fn sync_external(&mut self, value: &str) {
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

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
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
        if !self.selection.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selection.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled || self.selection.is_empty() {
            return;
        }
        self.copy(cx);
        self.replace(None, "", window, cx);
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
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

    fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.selecting.get() && !self.disabled {
            self.select_to(self.index_for_position(event.position), cx);
        }
    }

    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        let bounds = self.text_bounds.borrow();
        let layout = self.layout.borrow();
        let (Some(bounds), Some(line)) = (bounds.as_ref(), layout.as_ref()) else {
            return 0;
        };
        if position.x <= bounds.left() {
            return 0;
        }
        if position.x >= bounds.right() {
            return self.content.len();
        }
        let x = position.x - bounds.left() + px(self.scroll.get());
        line.closest_index_for_x(x).min(self.content.len())
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
        let layout = self.layout.borrow();
        let line = layout.as_ref()?;
        let range = editing::range_from_utf16(&self.content, &range_utf16);
        let scroll = px(self.scroll.get());
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

struct FieldText {
    model: Entity<FieldState>,
    placeholder: SharedString,
    font_family: SharedString,
    font_weight: Weight,
    font_size: f32,
    line_height: f32,
    text_color: Color,
    placeholder_color: Color,
    selection_color: Color,
    caret_color: Color,
    focused: bool,
    disabled: bool,
}

struct FieldPrepaint {
    line: Option<ShapedLine>,
    caret: Option<gpui::PaintQuad>,
    selection: Option<gpui::PaintQuad>,
    scroll: Pixels,
}

impl IntoElement for FieldText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FieldText {
    type RequestLayoutState = ();
    type PrepaintState = FieldPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(self.line_height).into();
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let state = self.model.read(cx);
        let content = state.content.clone();
        let selected = state.selection.clone();
        let cursor = state.cursor();
        let marked = state.marked.clone();
        let show_placeholder = content.is_empty();
        let display: SharedString = if show_placeholder {
            self.placeholder.clone()
        } else {
            content.clone()
        };
        let color = if show_placeholder {
            self.placeholder_color
        } else {
            self.text_color
        };
        let font = field_font(&self.font_family, self.font_weight);
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
            .shape_line(display, px(self.font_size), &runs, None);

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

        let (selection, caret) = if self.focused && !self.disabled && !show_placeholder {
            if selected.is_empty() {
                let x = bounds.left() + line.x_for_index(cursor) - scroll;
                (
                    None,
                    Some(fill(
                        Bounds::new(
                            point(x, bounds.top()),
                            size(px(CARET_WIDTH), bounds.size.height),
                        ),
                        Hsla::from(self.caret_color),
                    )),
                )
            } else {
                let start = bounds.left() + line.x_for_index(selected.start) - scroll;
                let end = bounds.left() + line.x_for_index(selected.end) - scroll;
                (
                    Some(fill(
                        Bounds::from_corners(
                            point(start, bounds.top()),
                            point(end, bounds.bottom()),
                        ),
                        Hsla::from(self.selection_color),
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
            let content_runs = vec![run(content.len(), self.text_color, false)];
            let stored =
                window
                    .text_system()
                    .shape_line(content, px(self.font_size), &content_runs, None);
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

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.focused && !self.disabled {
            let focus = self.model.read(cx).focus.clone();
            window.handle_input(
                &focus,
                ElementInputHandler::new(bounds, self.model.clone()),
                cx,
            );
        }
        let scroll = prepaint.scroll;
        let line = prepaint.line.take();
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection);
            }
            if let Some(line) = line {
                line.paint(
                    point(bounds.left() - scroll, bounds.top()),
                    px(self.line_height),
                    window,
                    cx,
                )
                .ok();
            }
            if let Some(caret) = prepaint.caret.take() {
                window.paint_quad(caret);
            }
        });
    }
}

fn field_font(family: &SharedString, weight: Weight) -> gpui::Font {
    let mut font = gpui::font(family.clone());
    font.weight = weight.into();
    font
}
