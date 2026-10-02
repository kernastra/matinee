//! Text tooltip.
//!
//! GPUI owns hover tracking: the tip appears after its show delay (500ms in
//! 0.2.2), hides when the pointer leaves, and hides on scroll. That delay is
//! not a motion token; it lives inside GPUI so a quick pointer pass does not
//! flash a tip. The bubble itself does not animate and does not take focus.
//! Reduced motion therefore changes nothing, which is the simplified path.

use gpui::{
    AnyView, App, AppContext, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div,
};

use crate::{
    ActiveTheme, StyledExt,
    components::Text,
    tokens::{Elevation, Radius, Space, TextRole},
};

/// A themed tooltip bubble. Construct it with [`Tooltip::view`] for GPUI's
/// tooltip hook, or [`WithTooltip::text_tooltip`] on an element.
pub struct Tooltip {
    text: gpui::SharedString,
}

impl Tooltip {
    pub fn new(text: impl Into<gpui::SharedString>) -> Self {
        Self { text: text.into() }
    }

    /// The view GPUI's `.tooltip` callback expects.
    pub fn view(text: impl Into<gpui::SharedString>, cx: &mut App) -> AnyView {
        let text = text.into();
        cx.new(|_| Tooltip { text }).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .px(Space::S2.px())
            .py(Space::S1.px())
            .corner_radius(theme, Radius::Small)
            .bg(theme.colors.surface.elevated)
            .border_1()
            .border_color(theme.colors.border.default)
            .elevation(theme, Elevation::Overlay)
            .child(
                Text::new(self.text.clone())
                    .role(TextRole::Caption)
                    .color(theme.colors.text.primary),
            )
    }
}

/// Attach a text tooltip that uses GPUI's hover delay and does not take focus.
pub trait WithTooltip: StatefulInteractiveElement + Sized {
    fn text_tooltip(self, text: impl Into<gpui::SharedString>) -> Self {
        let text = text.into();
        self.tooltip(move |_window, cx| Tooltip::view(text.clone(), cx))
    }
}

impl<T> WithTooltip for T where T: StatefulInteractiveElement {}
