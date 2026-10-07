//! Menu, menu items, and context menus.
//!
//! One menu implementation. A context menu is that menu anchored at a
//! pointer point. Submenus are not implemented.
//!
//! Up and Down move the cursor, Home and End jump, Enter and Space activate,
//! and Escape dismisses. Disabled rows are skipped and do not activate.
//!
//! A long menu can be given a [`Menu::max_height`]: its rows then scroll
//! inside the panel, and the keyboard cursor is scrolled into view.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnchoredPositionMode, App, ElementId, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, RenderOnce, ScrollHandle, StatefulInteractiveElement, Styled,
    Window, anchored, deferred, div, point, px,
};

use crate::{
    ActiveTheme, StyledExt,
    components::{
        FocusRing, Icon, IconName, IconSize, Text,
        focus_gate::{FocusGate, GateShare},
        keybindings::{
            Activate, Dismiss, MENU_CONTEXT, NudgeDown, NudgeToEnd, NudgeToStart, NudgeUp,
        },
    },
    focus::{self, focus_visible},
    navigation::{NavStep, move_enabled},
    overlay::{Alignment, Placement, anchor_origin},
    tokens::{Elevation, Radius, Space, TextRole},
};

const MENU_PRIORITY: usize = 20;
const ROW_H: f32 = 28.0;

type ActivateHandler = Rc<dyn Fn(&mut Window, &mut App)>;
type DismissHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// A menu row. The label is the accessible name.
pub struct MenuItem {
    label: gpui::SharedString,
    icon: Option<IconName>,
    shortcut: Option<gpui::SharedString>,
    disabled: bool,
    checked: bool,
    destructive: bool,
    on_activate: Option<ActivateHandler>,
}

impl MenuItem {
    pub fn new(label: impl Into<gpui::SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            shortcut: None,
            disabled: false,
            checked: false,
            destructive: false,
            on_activate: None,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn shortcut(mut self, shortcut: impl Into<gpui::SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn destructive(mut self, destructive: bool) -> Self {
        self.destructive = destructive;
        self
    }

    pub fn on_activate(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_activate = Some(Rc::new(handler));
        self
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}

/// A visual break between items. Not a cursor stop.
pub struct MenuSeparator;

#[derive(Clone)]
enum Entry {
    Item(ItemData),
    Separator,
}

#[derive(Clone)]
struct ItemData {
    label: gpui::SharedString,
    icon: Option<IconName>,
    shortcut: Option<gpui::SharedString>,
    disabled: bool,
    checked: bool,
    destructive: bool,
    on_activate: Option<ActivateHandler>,
}

/// An item or a separator, in menu order.
pub enum MenuEntry {
    Item(MenuItem),
    Separator(MenuSeparator),
}

struct MenuState {
    focus: FocusHandle,
    cursor: RefCell<Option<usize>>,
    scroll: ScrollHandle,
}

/// The menu panel. It claims focus from a parent gate when one is pending.
#[derive(IntoElement)]
pub struct Menu {
    id: ElementId,
    entries: Vec<Entry>,
    on_dismiss: Option<DismissHandler>,
    gate: Option<GateShare>,
    max_height: Option<f32>,
}

impl Menu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            entries: Vec::new(),
            on_dismiss: None,
            gate: None,
            max_height: None,
        }
    }

    /// Scroll the rows inside the panel once they are taller than this.
    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = Some(height);
        self
    }

    pub fn entries(mut self, entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        self.entries = entries
            .into_iter()
            .map(|entry| match entry {
                MenuEntry::Item(item) => Entry::Item(ItemData {
                    label: item.label,
                    icon: item.icon,
                    shortcut: item.shortcut,
                    disabled: item.disabled,
                    checked: item.checked,
                    destructive: item.destructive,
                    on_activate: item.on_activate,
                }),
                MenuEntry::Separator(_) => Entry::Separator,
            })
            .collect();
        self
    }

    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }

    pub(crate) fn gate(mut self, gate: &FocusGate) -> Self {
        self.gate = Some(gate.share());
        self
    }
}

impl RenderOnce for Menu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let model = window.use_keyed_state(self.id.clone(), cx, |_, cx| MenuState {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            cursor: RefCell::new(None),
            scroll: ScrollHandle::new(),
        });
        let focus = model.read(cx).focus.clone();
        if let Some(gate) = &self.gate {
            gate.claim(&focus, window);
        }
        let items: Vec<ItemData> = self
            .entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Item(item) => Some(item.clone()),
                Entry::Separator => None,
            })
            .collect();
        let disabled: Vec<bool> = items.iter().map(|item| item.disabled).collect();
        // Where each item sits among the panel's rows, separators included,
        // so the cursor can be scrolled into view.
        let rows: Rc<Vec<usize>> = Rc::new(
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| matches!(entry, Entry::Item(_)))
                .map(|(row, _)| row)
                .collect(),
        );
        if model.read(cx).cursor.borrow().is_none() {
            *model.read(cx).cursor.borrow_mut() = move_enabled(&disabled, None, NavStep::First);
        }
        let cursor = *model.read(cx).cursor.borrow();
        let focused = focus.is_focused(window);
        let show_ring = focus_visible(focused, cx);
        let on_dismiss = self.on_dismiss.clone();

        let mut panel = div()
            .id(self.id)
            .key_context(MENU_CONTEXT)
            .track_focus(&focus)
            .flex()
            .flex_col()
            .min_w(px(180.0))
            .py(Space::S1.px())
            .px(Space::S1.px())
            .bg(theme.colors.surface.elevated)
            .border_1()
            .border_color(theme.colors.border.default)
            .corner_radius(&theme, Radius::Medium)
            .elevation(&theme, Elevation::Overlay)
            .on_action({
                let model = model.clone();
                let disabled = disabled.clone();
                let rows = Rc::clone(&rows);
                move |_: &NudgeUp, _, cx| {
                    step(&model, &disabled, &rows, cursor, NavStep::Previous, cx)
                }
            })
            .on_action({
                let model = model.clone();
                let disabled = disabled.clone();
                let rows = Rc::clone(&rows);
                move |_: &NudgeDown, _, cx| {
                    step(&model, &disabled, &rows, cursor, NavStep::Next, cx)
                }
            })
            .on_action({
                let model = model.clone();
                let disabled = disabled.clone();
                let rows = Rc::clone(&rows);
                move |_: &NudgeToStart, _, cx| {
                    step(&model, &disabled, &rows, cursor, NavStep::First, cx)
                }
            })
            .on_action({
                let model = model.clone();
                let disabled = disabled.clone();
                let rows = Rc::clone(&rows);
                move |_: &NudgeToEnd, _, cx| {
                    step(&model, &disabled, &rows, cursor, NavStep::Last, cx)
                }
            })
            .on_action({
                let items = items.clone();
                let dismiss = on_dismiss.clone();
                move |_: &Activate, window, cx| {
                    activate(cursor, &items, dismiss.clone(), window, cx);
                }
            });
        if let Some(dismiss) = on_dismiss.clone() {
            panel = panel.on_action(move |_: &Dismiss, window, cx| dismiss(window, cx));
        }

        let mut list = div().id("menu-rows").flex().flex_col();
        if let Some(height) = self.max_height {
            list = list
                .max_h(px(height))
                .overflow_y_scroll()
                .track_scroll(&model.read(cx).scroll);
        }
        let mut item_index = 0;
        for entry in self.entries {
            match entry {
                Entry::Separator => {
                    list = list.child(
                        div()
                            .my(Space::S1.px())
                            .h(px(1.0))
                            .bg(theme.colors.border.subtle),
                    );
                }
                Entry::Item(item) => {
                    let index = item_index;
                    item_index += 1;
                    list = list.child(render_item(
                        index,
                        &item,
                        cursor == Some(index),
                        show_ring,
                        &theme,
                        on_dismiss.clone(),
                        &model,
                    ));
                }
            }
        }
        panel.child(list)
    }
}

fn step(
    model: &gpui::Entity<MenuState>,
    disabled: &[bool],
    rows: &[usize],
    cursor: Option<usize>,
    nav: NavStep,
    cx: &mut App,
) {
    let Some(next) = move_enabled(disabled, cursor, nav) else {
        return;
    };
    if Some(next) == cursor {
        return;
    }
    focus::note_keyboard_navigation(cx);
    model.update(cx, |state, cx| {
        *state.cursor.borrow_mut() = Some(next);
        if let Some(row) = rows.get(next) {
            state.scroll.scroll_to_item(*row);
        }
        cx.notify();
    });
}

fn activate(
    cursor: Option<usize>,
    items: &[ItemData],
    dismiss: Option<DismissHandler>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(index) = cursor else {
        return;
    };
    let Some(item) = items.get(index) else {
        return;
    };
    if item.disabled {
        return;
    }
    if let Some(handler) = item.on_activate.clone() {
        handler(window, cx);
    }
    if let Some(dismiss) = dismiss {
        dismiss(window, cx);
    }
}

fn render_item(
    index: usize,
    item: &ItemData,
    current: bool,
    show_ring: bool,
    theme: &crate::Theme,
    dismiss: Option<DismissHandler>,
    model: &gpui::Entity<MenuState>,
) -> impl IntoElement {
    let colors = &theme.colors;
    let radius = theme.radius.get(Radius::Small);
    let foreground = if item.disabled {
        colors.text.disabled
    } else if item.destructive {
        colors.text.danger
    } else {
        colors.text.primary
    };
    let hover = colors.control.subtle_hover;
    let handler = item.on_activate.clone();
    let enabled = !item.disabled;
    div()
        .id(("menu-item", index))
        .relative()
        .h(px(ROW_H))
        .px(Space::S2.px())
        .flex()
        .flex_row()
        .items_center()
        .gap(Space::S2.px())
        .rounded(px(radius))
        .when(current, |this| this.bg(colors.control.subtle_pressed))
        .when(enabled && !current, |this| {
            this.cursor_pointer().hover(move |style| style.bg(hover))
        })
        .when(enabled, |this| {
            let model = model.clone();
            let dismiss = dismiss.clone();
            let handler = handler.clone();
            this.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                window.prevent_default();
                focus::note_pointer_interaction(cx);
                *model.read(cx).cursor.borrow_mut() = Some(index);
            })
            .on_click(move |_, window, cx| {
                if let Some(handler) = handler.clone() {
                    handler(window, cx);
                }
                if let Some(dismiss) = dismiss.clone() {
                    dismiss(window, cx);
                }
            })
        })
        .child(mark(item, foreground))
        .child(
            div().flex_1().min_w(px(0.0)).child(
                Text::new(item.label.clone())
                    .role(TextRole::Label)
                    .color(foreground)
                    .truncate(),
            ),
        )
        .when_some(item.shortcut.clone(), |this, shortcut| {
            this.child(
                Text::new(shortcut)
                    .role(TextRole::Caption)
                    .color(colors.text.muted),
            )
        })
        .when(current && show_ring, |this| {
            this.child(FocusRing::new(radius, 0.0))
        })
}

fn mark(item: &ItemData, color: crate::tokens::Color) -> impl IntoElement {
    div()
        .w(px(16.0))
        .flex()
        .justify_center()
        .child(if item.checked {
            Icon::new(IconName::Check)
                .size(IconSize::Small)
                .color(color)
                .into_any_element()
        } else if let Some(icon) = item.icon {
            Icon::new(icon)
                .size(IconSize::Small)
                .color(color)
                .into_any_element()
        } else {
            div().into_any_element()
        })
}

fn menu_at(
    id: ElementId,
    entries: Vec<MenuEntry>,
    dismiss: Option<DismissHandler>,
    gate: &FocusGate,
) -> Menu {
    let mut menu = Menu::new((id, "menu")).entries(entries).gate(gate);
    if let Some(dismiss) = dismiss {
        menu = menu.on_dismiss(move |window, cx| dismiss(window, cx));
    }
    menu
}

struct ContextState {
    gate: FocusGate,
}

/// A menu opened at a pointer position. Uses [`Menu`] for every row.
#[derive(IntoElement)]
pub struct ContextMenu {
    id: ElementId,
    open: bool,
    position: Point<Pixels>,
    entries: Vec<MenuEntry>,
    on_dismiss: Option<DismissHandler>,
}

impl ContextMenu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: false,
            position: point(px(0.0), px(0.0)),
            entries: Vec::new(),
            on_dismiss: None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn at(mut self, position: Point<Pixels>) -> Self {
        self.position = position;
        self
    }

    pub fn entries(mut self, entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        self.entries = entries.into_iter().collect();
        self
    }

    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ContextMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id.clone();
        let model = window.use_keyed_state(id.clone(), cx, |_, cx| ContextState {
            gate: FocusGate::new(cx.focus_handle().tab_stop(false)),
        });
        let mut open = self.open;
        if !model.update(cx, |state, cx| state.gate.sync(open, false, window, cx)) {
            open = false;
            if let Some(dismiss) = self.on_dismiss.clone() {
                dismiss(window, cx);
            }
        }
        if !open {
            return div().id(id).into_any_element();
        }
        let origin = anchor_origin(
            crate::overlay::AnchorBox {
                x: f32::from(self.position.x),
                y: f32::from(self.position.y),
                width: 0.0,
                height: 0.0,
            },
            Placement::Bottom,
            Alignment::Start,
            0.0,
        );
        let dismiss = self.on_dismiss.clone();
        let dismiss_out = dismiss.clone();
        deferred(
            anchored()
                .anchor(origin.corner)
                .position(point(px(origin.x), px(origin.y)))
                .position_mode(AnchoredPositionMode::Window)
                .snap_to_window_with_margin(px(8.0))
                .child(
                    div()
                        .on_mouse_down_out(move |_, window, cx| {
                            if let Some(dismiss) = dismiss_out.clone() {
                                dismiss(window, cx);
                            }
                        })
                        .child(menu_at(id, self.entries, dismiss, &model.read(cx).gate)),
                ),
        )
        .with_priority(MENU_PRIORITY)
        .into_any_element()
    }
}
