use gpui::{
    AnyElement, App, Div, IntoElement, ParentElement, RenderOnce, StyleRefinement, Styled, Window,
    div,
};

use crate::{
    ActiveTheme, StyledExt,
    tokens::{Elevation, Radius, Space},
};

/// Which surface token a container sits on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SurfaceLevel {
    Canvas,
    #[default]
    Panel,
    Elevated,
}

/// A themed container: background, border, corner radius, and elevation
/// all come from tokens. Accepts children and further layout styling.
#[derive(IntoElement)]
pub struct Surface {
    level: SurfaceLevel,
    radius: Radius,
    padding: Space,
    bordered: bool,
    base: Div,
    children: Vec<AnyElement>,
}

impl Surface {
    pub fn new(level: SurfaceLevel) -> Self {
        Self {
            level,
            radius: Radius::Large,
            padding: Space::S4,
            bordered: level != SurfaceLevel::Canvas,
            base: div(),
            children: Vec::new(),
        }
    }

    pub fn radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }

    pub fn padding(mut self, padding: Space) -> Self {
        self.padding = padding;
        self
    }

    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    fn resting_elevation(&self) -> Elevation {
        match self.level {
            SurfaceLevel::Canvas | SurfaceLevel::Panel => Elevation::Flat,
            SurfaceLevel::Elevated => Elevation::Raised,
        }
    }
}

impl std::fmt::Debug for Surface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Surface")
            .field("level", &self.level)
            .field("radius", &self.radius)
            .field("padding", &self.padding)
            .field("children", &self.children.len())
            .finish()
    }
}

impl ParentElement for Surface {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Surface {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Surface {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let surface = &theme.colors.surface;
        let background = match self.level {
            SurfaceLevel::Canvas => surface.canvas,
            SurfaceLevel::Panel => surface.panel,
            SurfaceLevel::Elevated => surface.elevated,
        };
        let elevation = self.resting_elevation();
        let mut el = self
            .base
            .bg(background)
            .corner_radius(theme, self.radius)
            .p(self.padding.px())
            .elevation(theme, elevation);
        if self.bordered {
            el = el.border_1().border_color(theme.colors.border.subtle);
        }
        el.children(self.children)
    }
}
