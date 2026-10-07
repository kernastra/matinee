//! A focusable, activatable container with arbitrary content.
//!
//! [`Button`](super::Button) holds a label. Rows and tiles that hold an image,
//! several lines of text, or a progress line use `Pressable` instead. It is a
//! tab stop, activates on click, Enter, or Space (GPUI turns the keys into a
//! click on the focused element), shows hover and pressed fills, and draws
//! the keyboard focus ring only after keyboard navigation. Layout is the
//! caller's: `Pressable` is `Styled` and a `ParentElement`.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FocusHandle, InteractiveElement, IntoElement,
    MouseButton, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::{ActiveTheme, components::FocusRing, tokens::Radius};

type PressHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

struct PressState {
    focus: FocusHandle,
}

/// A tab stop that runs one action. The label is its accessible name.
#[derive(IntoElement)]
pub struct Pressable {
    id: ElementId,
    label: SharedString,
    base: Div,
    radius: Radius,
    disabled: bool,
    on_press: Option<PressHandler>,
    focus: Option<FocusHandle>,
}

impl Pressable {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            base: div(),
            radius: Radius::Medium,
            disabled: false,
            on_press: None,
            focus: None,
        }
    }

    pub fn radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }

    /// Disabled content stays visible, is not a tab stop, and does not activate.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_press(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_press = Some(Rc::new(handler));
        self
    }

    /// Use an existing focus handle, so the owner can move focus here.
    pub fn focus_handle(mut self, focus: FocusHandle) -> Self {
        self.focus = Some(focus);
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
}

impl Styled for Pressable {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl ParentElement for Pressable {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.base.extend(elements);
    }
}

impl RenderOnce for Pressable {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| PressState {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
        });
        let focus = self
            .focus
            .clone()
            .unwrap_or_else(|| state.read(cx).focus.clone());
        let active = !self.disabled && self.on_press.is_some();
        let focused = active && focus.is_focused(window);
        let theme = cx.theme();
        let radius = theme.radius.get(self.radius);
        let hover = theme.colors.control.subtle_hover;
        let pressed = theme.colors.control.subtle_pressed;

        self.base
            .id(self.id)
            .relative()
            .rounded(px(radius))
            .when(self.disabled, |this| this.opacity(0.5))
            .when(active, |this| {
                this.track_focus(&focus)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .active(move |style| style.bg(pressed))
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        // Pointer presses do not show the keyboard focus ring.
                        window.prevent_default();
                        crate::note_pointer_interaction(cx);
                    })
            })
            .when_some(self.on_press.filter(|_| active), |this, handler| {
                this.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .when(crate::focus_visible(focused, cx), |this| {
                this.child(FocusRing::new(radius, 1.0))
            })
    }
}
