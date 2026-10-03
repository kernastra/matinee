//! Horizontal toolbar.
//!
//! Leading and trailing regions share the leftover width, so a center title
//! sits in the middle. The bar does not draw window controls. Callers that
//! place it in an in-client titlebar apply the inset from `atelier-app`
//! around this layout; the bar itself has no platform pixel constants.

use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, App, IntoElement, ParentElement, RenderOnce, Styled, Window, div, px};

use crate::tokens::Space;

/// A desktop toolbar: leading tools, an optional center, trailing tools.
#[derive(IntoElement)]
pub struct Toolbar {
    leading: Vec<AnyElement>,
    center: Option<AnyElement>,
    trailing: Vec<AnyElement>,
}

impl Toolbar {
    pub fn new() -> Self {
        Self {
            leading: Vec::new(),
            center: None,
            trailing: Vec::new(),
        }
    }

    pub fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading.push(element.into_any_element());
        self
    }

    pub fn center(mut self, element: impl IntoElement) -> Self {
        self.center = Some(element.into_any_element());
        self
    }

    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing.push(element.into_any_element());
        self
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for Toolbar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .w_full()
            .min_h(Space::S10.px())
            .px(Space::S3.px())
            .flex()
            .flex_row()
            .items_center()
            .gap(Space::S2.px())
            .child(region(true, self.leading))
            .children(self.center.map(|center| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(center)
            }))
            .child(region(false, self.trailing))
    }
}

fn region(leading: bool, children: Vec<AnyElement>) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(Space::S2.px())
        .when(!leading, |this| this.justify_end())
        .children(children)
}
