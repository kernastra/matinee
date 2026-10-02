//! Restrained empty state.
//!
//! An optional icon, a title, a description, and up to two actions. The
//! caller supplies the actions, so this stays a layout rather than a
//! product-specific prompt.

use gpui::{AnyElement, App, IntoElement, ParentElement, RenderOnce, Styled, Window, div};

use crate::{
    ActiveTheme,
    components::{Icon, IconName, IconSize, Text, v_stack},
    tokens::{Space, TextRole},
};

/// Centered explanation shown when a region has nothing to list.
#[derive(IntoElement)]
pub struct EmptyState {
    title: gpui::SharedString,
    description: Option<gpui::SharedString>,
    icon: Option<IconName>,
    primary: Option<AnyElement>,
    secondary: Option<AnyElement>,
}

impl EmptyState {
    pub fn new(title: impl Into<gpui::SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            icon: None,
            primary: None,
            secondary: None,
        }
    }

    pub fn description(mut self, description: impl Into<gpui::SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn primary_action(mut self, action: impl IntoElement) -> Self {
        self.primary = Some(action.into_any_element());
        self
    }

    pub fn secondary_action(mut self, action: impl IntoElement) -> Self {
        self.secondary = Some(action.into_any_element());
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.colors.text.muted;
        let mut body = v_stack(Space::S2).items_center();
        if let Some(icon) = self.icon {
            body = body.child(Icon::new(icon).size(IconSize::Medium).color(muted));
        }
        body = body.child(Text::new(self.title).role(TextRole::Subheading));
        if let Some(description) = self.description {
            body = body.child(
                Text::new(description)
                    .role(TextRole::Body)
                    .tone(crate::components::TextTone::Muted),
            );
        }
        let actions = (self.primary.is_some() || self.secondary.is_some()).then(|| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(Space::S2.px())
                .children(self.primary)
                .children(self.secondary)
        });
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(Space::S8.px())
            .px(Space::S4.px())
            .gap(Space::S4.px())
            .child(body)
            .children(actions)
    }
}
