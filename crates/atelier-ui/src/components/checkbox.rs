//! Checkbox with unchecked, checked, and indeterminate states.
//!
//! Component metrics: 16px box, 8×2 indeterminate bar. The check glyph is
//! [`IconSize::Small`]. Pointer presses do not take keyboard focus.
//! Activation maps unchecked and indeterminate to checked, and checked to
//! unchecked.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, ElementId, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, RenderOnce, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::{
    ActiveTheme,
    components::{FocusRing, Icon, IconName, IconSize, Text, h_stack},
    focus::{self, focus_visible},
    inspect::{self, Inspection},
    tokens::{Space, TextRole},
};

/// Side length of the box.
const BOX: f32 = 16.0;
const BORDER: f32 = 1.0;
/// Indeterminate mark, centered in the box.
const BAR_W: f32 = 8.0;
const BAR_H: f32 = 2.0;

type ChangeHandler = Rc<dyn Fn(CheckboxState, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CheckboxState {
    #[default]
    Unchecked,
    Checked,
    Indeterminate,
}

impl CheckboxState {
    pub const fn name(self) -> &'static str {
        match self {
            CheckboxState::Unchecked => "unchecked",
            CheckboxState::Checked => "checked",
            CheckboxState::Indeterminate => "indeterminate",
        }
    }
}

/// Next state after a click or keyboard activation.
pub fn checkbox_activate(state: CheckboxState) -> CheckboxState {
    match state {
        CheckboxState::Unchecked | CheckboxState::Indeterminate => CheckboxState::Checked,
        CheckboxState::Checked => CheckboxState::Unchecked,
    }
}

/// A tri-state checkbox. `label` is the accessible name.
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    state: CheckboxState,
    label: Option<gpui::SharedString>,
    disabled: bool,
    on_change: Option<ChangeHandler>,
    inspected: bool,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>, state: CheckboxState) -> Self {
        Self {
            id: id.into(),
            state,
            label: None,
            disabled: false,
            on_change: None,
            inspected: false,
        }
    }

    pub fn label(mut self, label: impl Into<gpui::SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(
        mut self,
        handler: impl Fn(CheckboxState, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    pub fn inspected(mut self, inspected: bool) -> Self {
        self.inspected = inspected;
        self
    }
}

struct CheckState {
    focus: FocusHandle,
    pressed: bool,
}

impl RenderOnce for Checkbox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let model = window.use_keyed_state(self.id.clone(), cx, |_, cx| CheckState {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            pressed: false,
        });
        let (pressed, focus) = {
            let model = model.read(cx);
            (model.pressed, model.focus.clone())
        };
        let focused = !self.disabled && focus.is_focused(window);
        if self.inspected {
            inspect::report_inspection(Inspection {
                name: "Checkbox",
                focused,
                value: self.state.name().to_string(),
            });
        }
        let colors = &theme.colors;
        let marked = matches!(
            self.state,
            CheckboxState::Checked | CheckboxState::Indeterminate
        );
        let fill = if self.disabled {
            colors.control.disabled
        } else if marked {
            colors.control.accent
        } else if pressed {
            colors.control.neutral_pressed
        } else {
            colors.surface.elevated
        };
        let border = if self.disabled {
            colors.border.subtle
        } else if marked {
            colors.control.accent
        } else {
            colors.border.strong
        };
        let mark_color = if self.disabled {
            colors.text.disabled
        } else if marked {
            colors.text.on_accent
        } else {
            colors.text.primary
        };
        let radius = theme.radius.get(crate::tokens::Radius::Small);
        let next = checkbox_activate(self.state);
        let enabled = !self.disabled;

        let mark = match self.state {
            CheckboxState::Checked => Icon::new(IconName::Check)
                .size(IconSize::Small)
                .color(mark_color)
                .into_any_element(),
            CheckboxState::Indeterminate => div()
                .w(px(BAR_W))
                .h(px(BAR_H))
                .rounded(px(BAR_H / 2.0))
                .bg(mark_color)
                .into_any_element(),
            CheckboxState::Unchecked => div().into_any_element(),
        };

        let control = div()
            .id(self.id)
            .relative()
            .flex_none()
            .size(px(BOX))
            .rounded(px(radius))
            .bg(fill)
            .border(px(BORDER))
            .border_color(border)
            .flex()
            .items_center()
            .justify_center()
            .when(enabled, |this| this.track_focus(&focus).cursor_pointer())
            .when(enabled, |this| {
                let model = model.clone();
                let release = model.clone();
                let release_out = model.clone();
                this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    focus::note_pointer_interaction(cx);
                    model.update(cx, |state, cx| {
                        state.pressed = true;
                        cx.notify();
                    });
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    release.update(cx, |state, cx| {
                        state.pressed = false;
                        cx.notify();
                    });
                })
                .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                    release_out.update(cx, |state, cx| {
                        state.pressed = false;
                        cx.notify();
                    });
                })
                .hover({
                    let hover = if marked {
                        colors.control.accent_hover
                    } else {
                        colors.control.neutral_hover
                    };
                    move |style| style.bg(hover)
                })
            })
            .when_some(self.on_change.filter(|_| enabled), |this, handler| {
                this.on_click(move |_: &ClickEvent, window, cx| handler(next, window, cx))
            })
            .child(mark)
            .when(focus_visible(focused, cx), |this| {
                this.child(FocusRing::new(radius, BORDER))
            });

        h_stack(Space::S2)
            .child(control)
            .when_some(self.label, |this, label| {
                this.child(
                    Text::new(label)
                        .role(TextRole::Label)
                        .color(if self.disabled {
                            theme.colors.text.disabled
                        } else {
                            theme.colors.text.primary
                        }),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_cycle() {
        assert_eq!(
            checkbox_activate(CheckboxState::Unchecked),
            CheckboxState::Checked
        );
        assert_eq!(
            checkbox_activate(CheckboxState::Indeterminate),
            CheckboxState::Checked
        );
        assert_eq!(
            checkbox_activate(CheckboxState::Checked),
            CheckboxState::Unchecked
        );
    }
}
