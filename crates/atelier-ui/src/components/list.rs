//! Single-selection list.
//!
//! One tab stop. Arrow keys, Home, and End move the selection and skip
//! disabled rows. Enter activates the selection. Pointer clicks select and
//! do not take keyboard focus. Place the list inside a [`ScrollView`]; pass
//! that view's [`ScrollControl`] so a keyboard move reveals the row.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, RenderOnce, ScrollAnchor, StatefulInteractiveElement, Styled,
    Window, div, px,
};

use crate::{
    ActiveTheme,
    components::{
        FocusRing, ScrollControl, Text,
        keybindings::{Activate, LIST_CONTEXT, NudgeDown, NudgeToEnd, NudgeToStart, NudgeUp},
    },
    focus::{self, focus_visible},
    inspect::{self, Inspection},
    navigation::{NavStep, move_enabled},
    tokens::{Radius, Space, TextRole},
};

type IndexHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;
type ContextHandler = Rc<dyn Fn(usize, Point<Pixels>, &mut Window, &mut App)>;

/// One row. Leading and trailing content are optional elements.
pub struct ListRow {
    primary: gpui::SharedString,
    secondary: Option<gpui::SharedString>,
    disabled: bool,
    leading: Option<AnyElement>,
    trailing: Option<AnyElement>,
}

impl ListRow {
    pub fn new(primary: impl Into<gpui::SharedString>) -> Self {
        Self {
            primary: primary.into(),
            secondary: None,
            disabled: false,
            leading: None,
            trailing: None,
        }
    }

    pub fn secondary(mut self, text: impl Into<gpui::SharedString>) -> Self {
        self.secondary = Some(text.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn label(&self) -> &str {
        &self.primary
    }
}

struct ListState {
    focus: FocusHandle,
    anchors: RefCell<Vec<ScrollAnchor>>,
    keyboard: RefCell<bool>,
    seen: RefCell<Option<usize>>,
}

/// A vertical, single-selection list. It does not scroll; the parent
/// [`super::ScrollView`] does.
#[derive(IntoElement)]
pub struct List {
    id: ElementId,
    rows: Vec<ListRow>,
    selected: Option<usize>,
    on_select: Option<IndexHandler>,
    on_activate: Option<IndexHandler>,
    on_context_menu: Option<ContextHandler>,
    scroll: Option<ScrollControl>,
    inspected: bool,
}

impl List {
    pub fn new(id: impl Into<ElementId>, rows: Vec<ListRow>) -> Self {
        Self {
            id: id.into(),
            rows,
            selected: None,
            on_select: None,
            on_activate: None,
            on_context_menu: None,
            scroll: None,
            inspected: false,
        }
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }

    pub fn on_activate(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_activate = Some(Rc::new(handler));
        self
    }

    /// Secondary click. The point is in window coordinates.
    pub fn on_context_menu(
        mut self,
        handler: impl Fn(usize, Point<Pixels>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_context_menu = Some(Rc::new(handler));
        self
    }

    pub fn scroll_control(mut self, control: ScrollControl) -> Self {
        self.scroll = Some(control);
        self
    }

    pub fn inspected(mut self, inspected: bool) -> Self {
        self.inspected = inspected;
        self
    }
}

impl RenderOnce for List {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let model = window.use_keyed_state(self.id.clone(), cx, |_, cx| ListState {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            anchors: RefCell::new(Vec::new()),
            keyboard: RefCell::new(false),
            seen: RefCell::new(None),
        });
        let focus = model.read(cx).focus.clone();
        let focused = focus.is_focused(window);
        let disabled: Vec<bool> = self.rows.iter().map(ListRow::is_disabled).collect();
        let selected = self.selected.filter(|index| *index < self.rows.len());
        if self.inspected {
            let value = selected
                .and_then(|index| self.rows.get(index))
                .map(|row| row.primary.to_string())
                .unwrap_or_default();
            inspect::report_inspection(Inspection {
                name: "List",
                focused,
                value,
            });
        }
        sync_anchors(&model, self.scroll.as_ref(), self.rows.len(), cx);
        let reveal = {
            let state = model.read(cx);
            let keyboard = *state.keyboard.borrow();
            let seen = *state.seen.borrow();
            if keyboard && seen != selected {
                selected.and_then(|index| state.anchors.borrow().get(index).cloned())
            } else {
                None
            }
        };
        if let Some(anchor) = reveal {
            anchor.scroll_to(window, cx);
            *model.read(cx).keyboard.borrow_mut() = false;
        }
        *model.read(cx).seen.borrow_mut() = selected;

        let on_select = self.on_select.clone();
        let mut column = div()
            .id(self.id)
            .key_context(LIST_CONTEXT)
            .w_full()
            .flex()
            .flex_col()
            .track_focus(&focus);

        if let Some(on_select) = on_select.clone() {
            let flags = disabled.clone();
            let flags_down = disabled.clone();
            let flags_home = disabled.clone();
            let flags_end = disabled.clone();
            let select_up = on_select.clone();
            let select_down = on_select.clone();
            let select_home = on_select.clone();
            let select_end = on_select.clone();
            let model_up = model.clone();
            let model_down = model.clone();
            let model_home = model.clone();
            let model_end = model.clone();
            column = column
                .on_action(move |_: &NudgeUp, window, cx| {
                    move_to(
                        &flags,
                        selected,
                        NavStep::Previous,
                        &model_up,
                        &select_up,
                        window,
                        cx,
                    );
                })
                .on_action(move |_: &NudgeDown, window, cx| {
                    move_to(
                        &flags_down,
                        selected,
                        NavStep::Next,
                        &model_down,
                        &select_down,
                        window,
                        cx,
                    );
                })
                .on_action(move |_: &NudgeToStart, window, cx| {
                    move_to(
                        &flags_home,
                        selected,
                        NavStep::First,
                        &model_home,
                        &select_home,
                        window,
                        cx,
                    );
                })
                .on_action(move |_: &NudgeToEnd, window, cx| {
                    move_to(
                        &flags_end,
                        selected,
                        NavStep::Last,
                        &model_end,
                        &select_end,
                        window,
                        cx,
                    );
                });
        }
        if let Some(on_activate) = self.on_activate.clone() {
            column = column.on_action(move |_: &Activate, window, cx| {
                if let Some(index) = selected
                    && !disabled.get(index).copied().unwrap_or(true)
                {
                    focus::note_keyboard_navigation(cx);
                    on_activate(index, window, cx);
                }
            });
        }

        let anchors = model.read(cx).anchors.borrow().clone();
        for (index, row) in self.rows.into_iter().enumerate() {
            column = column.child(render_row(
                index,
                row,
                selected == Some(index),
                focused && focus_visible(true, cx),
                RowChrome {
                    theme: &theme,
                    anchor: anchors.get(index).cloned(),
                    on_select: self.on_select.clone(),
                    on_context_menu: self.on_context_menu.clone(),
                },
            ));
        }
        column
    }
}

fn sync_anchors(
    model: &gpui::Entity<ListState>,
    scroll: Option<&ScrollControl>,
    count: usize,
    cx: &App,
) {
    let current = model.read(cx).anchors.borrow().len();
    if current == count {
        return;
    }
    let anchors = scroll
        .map(|control| {
            (0..count)
                .map(|_| ScrollAnchor::for_handle(control.handle().clone()))
                .collect()
        })
        .unwrap_or_default();
    *model.read(cx).anchors.borrow_mut() = anchors;
}

fn move_to(
    disabled: &[bool],
    selected: Option<usize>,
    step: NavStep,
    model: &gpui::Entity<ListState>,
    on_select: &IndexHandler,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(next) = move_enabled(disabled, selected, step) else {
        return;
    };
    if Some(next) == selected {
        return;
    }
    focus::note_keyboard_navigation(cx);
    *model.read(cx).keyboard.borrow_mut() = true;
    on_select(next, window, cx);
}

struct RowChrome<'a> {
    theme: &'a crate::Theme,
    anchor: Option<ScrollAnchor>,
    on_select: Option<IndexHandler>,
    on_context_menu: Option<ContextHandler>,
}

fn render_row(
    index: usize,
    row: ListRow,
    selected: bool,
    show_ring: bool,
    chrome: RowChrome<'_>,
) -> AnyElement {
    let RowChrome {
        theme,
        anchor,
        on_select,
        on_context_menu,
    } = chrome;
    let colors = &theme.colors;
    let radius = theme.radius.get(Radius::Medium);
    let enabled = !row.disabled;
    let label = row.primary.clone();
    let secondary = row.secondary.clone();
    let tone = if !enabled {
        crate::components::TextTone::Disabled
    } else if selected {
        crate::components::TextTone::Primary
    } else {
        crate::components::TextTone::Secondary
    };
    let hover = colors.control.subtle_hover;
    let mut item = div()
        .id(("row", index))
        .relative()
        .w_full()
        .min_h(px(36.0))
        .px(Space::S2.px())
        .py(Space::S1.px())
        .flex()
        .flex_row()
        .items_center()
        .gap(Space::S3.px())
        .rounded(px(radius))
        .when(selected, |this| this.bg(colors.control.subtle_pressed))
        .when_some(anchor, |this, anchor| this.anchor_scroll(Some(anchor)))
        .when(enabled, |this| {
            this.cursor_pointer()
                .when(!selected, |this| this.hover(move |style| style.bg(hover)))
        });
    if enabled && let Some(on_select) = on_select {
        item = item
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                focus::note_pointer_interaction(cx);
            })
            .on_click(move |_, window, cx| on_select(index, window, cx));
    }
    if enabled && let Some(on_context_menu) = on_context_menu {
        item = item.on_mouse_down(MouseButton::Right, move |event, window, cx| {
            focus::note_pointer_interaction(cx);
            cx.stop_propagation();
            on_context_menu(index, event.position, window, cx);
        });
    }
    item.child(row.leading.unwrap_or_else(|| div().into_any_element()))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(Text::new(label).role(TextRole::Label).tone(tone).truncate())
                .when_some(secondary, |this, text| {
                    this.child(
                        Text::new(text)
                            .role(TextRole::Caption)
                            .tone(if enabled {
                                crate::components::TextTone::Muted
                            } else {
                                crate::components::TextTone::Disabled
                            })
                            .truncate(),
                    )
                }),
        )
        .children(row.trailing)
        .when(selected && show_ring, |this| {
            this.child(FocusRing::new(radius, 0.0))
        })
        .into_any_element()
}
