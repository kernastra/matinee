//! Segmented control.
//!
//! One tab stop for the group. Arrow keys move the selection without
//! wrapping and skip disabled segments. Pointer selection does not take
//! keyboard focus.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, ElementId, FocusHandle, InteractiveElement, IntoElement, MouseButton, ParentElement,
    RenderOnce, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::{
    ActiveTheme,
    components::{
        FocusRing, Icon, IconName, IconSize, Text,
        keybindings::{
            NudgeDown, NudgeLeft, NudgeRight, NudgeToEnd, NudgeToStart, NudgeUp, SEGMENTED_CONTEXT,
        },
    },
    focus::{self, focus_visible},
    inspect::{self, Inspection},
    tokens::{Space, TextRole},
};

const HEIGHT: f32 = 28.0;
const BORDER: f32 = 1.0;
const PAD_X: f32 = 10.0;

type ChangeHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// One segment. Disabled segments stay visible and are skipped by arrows.
#[derive(Clone)]
pub struct Segment {
    pub label: gpui::SharedString,
    pub icon: Option<IconName>,
    pub disabled: bool,
}

impl Segment {
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
}

/// Move `delta` enabled segments from `selected`. Does not wrap.
/// Positive moves forward.
pub fn move_selection(disabled: &[bool], selected: usize, delta: i32) -> usize {
    if disabled.is_empty() || delta == 0 {
        return selected.min(disabled.len().saturating_sub(1));
    }
    let current = selected.min(disabled.len() - 1);
    let dir = delta.signum();
    let mut index = current as i32;
    loop {
        index += dir;
        if index < 0 || index >= disabled.len() as i32 {
            return current;
        }
        if !disabled[index as usize] {
            return index as usize;
        }
    }
}

/// A single-selection segmented control.
#[derive(IntoElement)]
pub struct SegmentedControl {
    id: ElementId,
    segments: Vec<Segment>,
    selected: usize,
    disabled: bool,
    on_change: Option<ChangeHandler>,
    inspected: bool,
}

impl SegmentedControl {
    pub fn new(id: impl Into<ElementId>, segments: Vec<Segment>, selected: usize) -> Self {
        Self {
            id: id.into(),
            segments,
            selected,
            disabled: false,
            on_change: None,
            inspected: false,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    pub fn inspected(mut self, inspected: bool) -> Self {
        self.inspected = inspected;
        self
    }
}

struct SegmentState {
    focus: FocusHandle,
}

impl RenderOnce for SegmentedControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let model = window.use_keyed_state(self.id.clone(), cx, |_, cx| SegmentState {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
        });
        let focus = model.read(cx).focus.clone();
        let focused = !self.disabled && focus.is_focused(window);
        let selected = self.selected.min(self.segments.len().saturating_sub(1));
        if self.inspected {
            let label = self
                .segments
                .get(selected)
                .map(|segment| segment.label.to_string())
                .unwrap_or_default();
            inspect::report_inspection(Inspection {
                name: "Segmented control",
                focused,
                value: label,
            });
        }
        let flags: Vec<bool> = self
            .segments
            .iter()
            .map(|segment| segment.disabled)
            .collect();
        let colors = &theme.colors;
        let radius = theme.radius.get(crate::tokens::Radius::Medium);
        let enabled = !self.disabled && !self.segments.is_empty();
        let on_change = self.on_change.clone();

        let mut row = div()
            .id(self.id.clone())
            .key_context(SEGMENTED_CONTEXT)
            .relative()
            .flex()
            .flex_row()
            .h(px(HEIGHT))
            .p(px(2.0))
            .gap(px(2.0))
            .rounded(px(radius))
            .bg(colors.surface.panel)
            .border(px(BORDER))
            .border_color(colors.border.default)
            .when(enabled, |this| this.track_focus(&focus));

        if enabled {
            let flags_left = flags.clone();
            let flags_right = flags.clone();
            let change_left = on_change.clone();
            let change_right = on_change.clone();
            row = row
                .on_action(move |_: &NudgeLeft, window, cx| {
                    nudge(&flags_left, selected, -1, change_left.clone(), window, cx);
                })
                .on_action(move |_: &NudgeDown, window, cx| {
                    nudge(&flags_right, selected, -1, change_right.clone(), window, cx);
                });
            let flags_right = flags.clone();
            let flags_up = flags.clone();
            let change = on_change.clone();
            let change_up = on_change.clone();
            row = row
                .on_action(move |_: &NudgeRight, window, cx| {
                    nudge(&flags_right, selected, 1, change.clone(), window, cx);
                })
                .on_action(move |_: &NudgeUp, window, cx| {
                    nudge(&flags_up, selected, 1, change_up.clone(), window, cx);
                });
            let flags_home = flags.clone();
            let flags_end = flags.clone();
            let change_home = on_change.clone();
            let change_end = on_change.clone();
            row = row
                .on_action(move |_: &NudgeToStart, window, cx| {
                    if let Some(index) = flags_home.iter().position(|disabled| !disabled)
                        && index != selected
                        && let Some(change) = change_home.clone()
                    {
                        change(index, window, cx);
                    }
                })
                .on_action(move |_: &NudgeToEnd, window, cx| {
                    if let Some(index) = flags_end.iter().rposition(|disabled| !disabled)
                        && index != selected
                        && let Some(change) = change_end.clone()
                    {
                        change(index, window, cx);
                    }
                })
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    focus::note_pointer_interaction(cx);
                });
        }

        for (index, segment) in self.segments.iter().enumerate() {
            let active = index == selected;
            let segment_enabled = enabled && !segment.disabled;
            let bg = if segment.disabled {
                colors.control.disabled
            } else if active {
                colors.control.accent
            } else {
                crate::tokens::Color::TRANSPARENT
            };
            let fg = if segment.disabled {
                colors.text.disabled
            } else if active {
                colors.text.on_accent
            } else {
                colors.text.primary
            };
            let hover = colors.control.subtle_hover;
            let change = on_change.clone();
            row = row.child(
                div()
                    .id((self.id.clone(), index.to_string()))
                    .h_full()
                    .px(px(PAD_X))
                    .flex()
                    .items_center()
                    .gap(Space::S1.px())
                    .rounded(px((radius - 2.0).max(0.0)))
                    .bg(bg)
                    .when(segment_enabled && !active, |this| {
                        this.cursor_pointer().hover(move |style| style.bg(hover))
                    })
                    .when(segment_enabled, |this| {
                        this.on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            focus::note_pointer_interaction(cx);
                        })
                        .on_click(move |_, window, cx| {
                            if let Some(change) = change.clone() {
                                change(index, window, cx);
                            }
                        })
                    })
                    .when_some(segment.icon, |this, icon| {
                        this.child(Icon::new(icon).size(IconSize::Small).color(fg))
                    })
                    .child(
                        Text::new(segment.label.clone())
                            .role(TextRole::Label)
                            .color(fg),
                    ),
            );
        }

        row.when(focus_visible(focused, cx), |this| {
            this.child(FocusRing::new(radius, BORDER))
        })
    }
}

fn nudge(
    disabled: &[bool],
    selected: usize,
    delta: i32,
    on_change: Option<ChangeHandler>,
    window: &mut Window,
    cx: &mut App,
) {
    let next = move_selection(disabled, selected, delta);
    if next != selected
        && let Some(on_change) = on_change
    {
        on_change(next, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_skip_disabled_and_do_not_wrap() {
        let disabled = [false, true, false, false];
        assert_eq!(move_selection(&disabled, 0, 1), 2);
        assert_eq!(move_selection(&disabled, 2, -1), 0);
        assert_eq!(move_selection(&disabled, 0, -1), 0);
        assert_eq!(move_selection(&disabled, 3, 1), 3);
    }
}
