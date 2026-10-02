//! On/off switch.
//!
//! Component metrics (logical pixels): track 36×20, thumb 16, inset 2.
//! The thumb uses [`crate::motion::spring`] with [`Spring::Snappy`] after the
//! first value change. Reduced motion snaps, because that constructor
//! returns `None`. Pointer presses do not take keyboard focus.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnimationExt, App, ClickEvent, ElementId, FocusHandle, InteractiveElement, IntoElement,
    MouseButton, ParentElement, RenderOnce, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::{
    ActiveTheme,
    components::{FocusRing, Text, h_stack},
    focus::{self, focus_visible},
    inspect::{self, Inspection},
    motion,
    tokens::{Space, Spring, TextRole},
};

const TRACK_WIDTH: f32 = 36.0;
const TRACK_HEIGHT: f32 = 20.0;
const THUMB: f32 = 16.0;
const INSET: f32 = 2.0;
const BORDER: f32 = 1.0;

type ChangeHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// An on/off switch. `label` is the accessible name.
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    on: bool,
    label: Option<gpui::SharedString>,
    disabled: bool,
    on_change: Option<ChangeHandler>,
    inspected: bool,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>, on: bool) -> Self {
        Self {
            id: id.into(),
            on,
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

    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    pub fn inspected(mut self, inspected: bool) -> Self {
        self.inspected = inspected;
        self
    }
}

struct SwitchState {
    focus: FocusHandle,
    pressed: bool,
    /// Thumb position (0 or 1) to animate from. Equals the target until the
    /// value changes, so the first frame does not animate.
    from: f32,
    seen: Option<bool>,
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| SwitchState {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            pressed: false,
            from: if self.on { 1.0 } else { 0.0 },
            seen: None,
        });
        let (pressed, focus, mut from) = {
            let state = state.read(cx);
            (state.pressed, state.focus.clone(), state.from)
        };
        if state.read(cx).seen != Some(self.on) {
            let first = state.read(cx).seen.is_none();
            from = if first {
                if self.on { 1.0 } else { 0.0 }
            } else {
                state.read(cx).from
            };
            let target = if self.on { 1.0 } else { 0.0 };
            let previous = if first { target } else { from };
            state.update(cx, |state, _| {
                state.from = previous;
                state.seen = Some(self.on);
            });
            from = previous;
        }
        let to = if self.on { 1.0 } else { 0.0 };
        let focused = !self.disabled && focus.is_focused(window);
        if self.inspected {
            inspect::report_inspection(Inspection {
                name: "Switch",
                focused,
                value: self.on.to_string(),
            });
        }

        let colors = &theme.colors;
        let track = if self.disabled {
            colors.control.disabled
        } else if self.on {
            colors.control.accent
        } else if pressed {
            colors.control.neutral_pressed
        } else {
            colors.control.neutral
        };
        let thumb_color = if self.disabled {
            colors.text.disabled
        } else if self.on {
            colors.text.on_accent
        } else {
            colors.text.primary
        };
        let travel = TRACK_WIDTH - THUMB - INSET * 2.0;
        let handler = self.on_change.clone();
        let on = self.on;
        let enabled = !self.disabled;

        let thumb = div()
            .absolute()
            .top(px(INSET))
            .size(px(THUMB))
            .rounded(px(THUMB / 2.0))
            .bg(thumb_color);
        let thumb =
            if let Some(animation) = motion::spring(cx, Spring::Snappy).filter(|_| from != to) {
                thumb
                    .with_animation(
                        (self.id.clone(), if self.on { "on" } else { "off" }),
                        animation,
                        move |thumb, t| thumb.left(px(INSET + travel * (from + (to - from) * t))),
                    )
                    .into_any_element()
            } else {
                thumb.left(px(INSET + travel * to)).into_any_element()
            };

        let control = div()
            .id(self.id.clone())
            .relative()
            .flex_none()
            .w(px(TRACK_WIDTH))
            .h(px(TRACK_HEIGHT))
            .rounded(px(TRACK_HEIGHT / 2.0))
            .bg(track)
            .border(px(BORDER))
            .border_color(if self.on && !self.disabled {
                colors.control.accent
            } else {
                colors.border.default
            })
            .when(enabled, |this| this.track_focus(&focus).cursor_pointer())
            .when(enabled, |this| {
                let down = state.clone();
                let up = state.clone();
                let up_out = state.clone();
                this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    focus::note_pointer_interaction(cx);
                    down.update(cx, |state, cx| {
                        state.pressed = true;
                        cx.notify();
                    });
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    up.update(cx, |state, cx| {
                        state.pressed = false;
                        cx.notify();
                    });
                })
                .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                    up_out.update(cx, |state, cx| {
                        state.pressed = false;
                        cx.notify();
                    });
                })
            })
            .when(enabled, |this| {
                this.hover({
                    let hover = if self.on {
                        colors.control.accent_hover
                    } else {
                        colors.control.neutral_hover
                    };
                    move |style| style.bg(hover)
                })
            })
            .when_some(handler.filter(|_| enabled), |this, handler| {
                this.on_click(move |_: &ClickEvent, window, cx| handler(!on, window, cx))
            })
            .child(thumb)
            .when(focus_visible(focused, cx), |this| {
                this.child(FocusRing::new(TRACK_HEIGHT / 2.0, BORDER))
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
