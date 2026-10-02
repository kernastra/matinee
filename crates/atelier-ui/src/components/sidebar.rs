//! Sectioned navigation sidebar.
//!
//! One tab stop. Up and Down move the selection and skip disabled items.
//! Home and End jump. Pointer clicks select and do not take keyboard focus.
//! The footer stays at the bottom when the parent gives the sidebar a height.
//! Nothing here is a product destination.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, RenderOnce, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::{
    ActiveTheme,
    components::{
        FocusRing, Icon, IconName, IconSize, Text, TextTone,
        keybindings::{Activate, NudgeDown, NudgeToEnd, NudgeToStart, NudgeUp, SIDEBAR_CONTEXT},
    },
    focus::{self, focus_visible},
    navigation::{NavStep, move_enabled},
    tokens::{Radius, Space, TextRole},
};

const ROW_H: f32 = 28.0;

type IndexHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// One destination. The label is the accessible name.
pub struct SidebarItem {
    label: gpui::SharedString,
    icon: Option<IconName>,
    disabled: bool,
}

impl SidebarItem {
    pub fn new(label: impl Into<gpui::SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            disabled: false,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}

/// A labeled group of items. The label is not a selection stop.
pub struct SidebarSection {
    label: Option<gpui::SharedString>,
    items: Vec<SidebarItem>,
}

impl SidebarSection {
    pub fn new(items: Vec<SidebarItem>) -> Self {
        Self { label: None, items }
    }

    pub fn label(mut self, label: impl Into<gpui::SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

struct SidebarState {
    focus: FocusHandle,
}

/// Flat single selection across sections, in source order.
#[derive(IntoElement)]
pub struct Sidebar {
    id: ElementId,
    sections: Vec<SidebarSection>,
    selected: Option<usize>,
    on_select: Option<IndexHandler>,
    on_activate: Option<IndexHandler>,
    footer: Option<AnyElement>,
}

impl Sidebar {
    pub fn new(id: impl Into<ElementId>, sections: Vec<SidebarSection>) -> Self {
        Self {
            id: id.into(),
            sections,
            selected: None,
            on_select: None,
            on_activate: None,
            footer: None,
        }
    }

    /// Flat index across every section, in order.
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

    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }
}

impl RenderOnce for Sidebar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let model = window.use_keyed_state(self.id.clone(), cx, |_, cx| SidebarState {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
        });
        let focus = model.read(cx).focus.clone();
        let focused = focus.is_focused(window);
        let items = flatten(&self.sections);
        let disabled: Vec<bool> = items.iter().map(|item| item.disabled).collect();
        let selected = self.selected.filter(|index| *index < items.len());
        let show_ring = focused && focus_visible(true, cx);

        let mut nav = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .gap(Space::S3.px());
        let mut flat = 0usize;
        for section in self.sections {
            let mut group = div().flex().flex_col().gap(px(2.0));
            if let Some(label) = section.label {
                group = group.child(
                    div().px(Space::S2.px()).child(
                        Text::new(label)
                            .role(TextRole::Caption)
                            .tone(TextTone::Muted),
                    ),
                );
            }
            for item in section.items {
                let index = flat;
                flat += 1;
                group = group.child(render_item(
                    index,
                    item,
                    selected == Some(index),
                    show_ring,
                    &theme,
                    self.on_select.clone(),
                ));
            }
            nav = nav.child(group);
        }

        let mut column = div()
            .id(self.id)
            .key_context(SIDEBAR_CONTEXT)
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap(Space::S3.px())
            .track_focus(&focus);
        if let Some(on_select) = self.on_select.clone() {
            column = bind_nav(column, disabled.clone(), selected, on_select);
        }
        if let Some(on_activate) = self.on_activate.clone() {
            let flags = disabled.clone();
            column = column.on_action(move |_: &Activate, window, cx| {
                if let Some(index) = selected
                    && !flags.get(index).copied().unwrap_or(true)
                {
                    focus::note_keyboard_navigation(cx);
                    on_activate(index, window, cx);
                }
            });
        }
        column.child(nav).children(self.footer)
    }
}

fn flatten(sections: &[SidebarSection]) -> Vec<&SidebarItem> {
    sections
        .iter()
        .flat_map(|section| section.items.iter())
        .collect()
}

fn bind_nav(
    column: gpui::Stateful<gpui::Div>,
    disabled: Vec<bool>,
    selected: Option<usize>,
    on_select: IndexHandler,
) -> gpui::Stateful<gpui::Div> {
    let up = (disabled.clone(), on_select.clone());
    let down = (disabled.clone(), on_select.clone());
    let first = (disabled.clone(), on_select.clone());
    let last = (disabled, on_select);
    column
        .on_action(move |_: &NudgeUp, window, cx| {
            step(&up.0, selected, NavStep::Previous, &up.1, window, cx);
        })
        .on_action(move |_: &NudgeDown, window, cx| {
            step(&down.0, selected, NavStep::Next, &down.1, window, cx);
        })
        .on_action(move |_: &NudgeToStart, window, cx| {
            step(&first.0, selected, NavStep::First, &first.1, window, cx);
        })
        .on_action(move |_: &NudgeToEnd, window, cx| {
            step(&last.0, selected, NavStep::Last, &last.1, window, cx);
        })
}

fn step(
    disabled: &[bool],
    selected: Option<usize>,
    nav: NavStep,
    on_select: &IndexHandler,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(next) = move_enabled(disabled, selected, nav) else {
        return;
    };
    if Some(next) == selected {
        return;
    }
    focus::note_keyboard_navigation(cx);
    on_select(next, window, cx);
}

fn render_item(
    index: usize,
    item: SidebarItem,
    selected: bool,
    show_ring: bool,
    theme: &crate::Theme,
    on_select: Option<IndexHandler>,
) -> AnyElement {
    let colors = &theme.colors;
    let radius = theme.radius.get(Radius::Medium);
    let enabled = !item.disabled;
    let tone = if !enabled {
        TextTone::Disabled
    } else if selected {
        TextTone::Primary
    } else {
        TextTone::Secondary
    };
    let text = &theme.colors.text;
    let color = match tone {
        TextTone::Primary => text.primary,
        TextTone::Secondary => text.secondary,
        TextTone::Muted => text.muted,
        TextTone::Disabled => text.disabled,
    };
    let hover = colors.control.subtle_hover;
    let mut row = div()
        .id(("sidebar-item", index))
        .relative()
        .w_full()
        .h(px(ROW_H))
        .px(Space::S2.px())
        .flex()
        .flex_row()
        .items_center()
        .gap(Space::S2.px())
        .rounded(px(radius))
        .when(selected, |this| this.bg(colors.control.subtle_pressed))
        .when(enabled, |this| {
            this.cursor_pointer()
                .when(!selected, |this| this.hover(move |style| style.bg(hover)))
        });
    if enabled && let Some(on_select) = on_select {
        row = row
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                focus::note_pointer_interaction(cx);
            })
            .on_click(move |_, window, cx| on_select(index, window, cx));
    }
    row.when_some(item.icon, |this, icon| {
        this.child(Icon::new(icon).size(IconSize::Small).color(color))
    })
    .child(
        Text::new(item.label)
            .role(TextRole::Label)
            .tone(tone)
            .truncate(),
    )
    .when(selected && show_ring, |this| {
        this.child(FocusRing::new(radius, 0.0))
    })
    .into_any_element()
}
