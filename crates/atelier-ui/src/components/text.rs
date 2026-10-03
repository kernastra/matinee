use gpui::{App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px};

use crate::{
    ActiveTheme,
    tokens::{Color, TextRole},
};

/// Semantic text colors. Use [`Text::color`] only for genuinely custom cases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextTone {
    #[default]
    Primary,
    Secondary,
    Muted,
    Disabled,
}

/// A run of text styled by role. Sizes, families, weights, and line heights
/// come from the theme's type scale; callers choose meaning, not metrics.
#[derive(IntoElement, Debug)]
pub struct Text {
    content: SharedString,
    role: TextRole,
    tone: TextTone,
    color: Option<Color>,
    truncate: bool,
}

impl Text {
    pub fn new(content: impl Into<SharedString>) -> Self {
        Self {
            content: content.into(),
            role: TextRole::Body,
            tone: TextTone::Primary,
            color: None,
            truncate: false,
        }
    }

    pub fn role(mut self, role: TextRole) -> Self {
        self.role = role;
        self
    }

    pub fn tone(mut self, tone: TextTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Single line, clipped with an ellipsis.
    pub fn truncate(mut self) -> Self {
        self.truncate = true;
        self
    }
}

impl RenderOnce for Text {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let style = theme.typography.style(self.role);
        let text = &theme.colors.text;
        let color = self.color.unwrap_or(match self.tone {
            TextTone::Primary => text.primary,
            TextTone::Secondary => text.secondary,
            TextTone::Muted => text.muted,
            TextTone::Disabled => text.disabled,
        });
        let el = div()
            .font_family(theme.typography.family(self.role))
            .text_size(px(style.size))
            .line_height(px(style.line_height))
            .font_weight(style.weight.into())
            .text_color(color)
            .child(self.content);
        if self.truncate {
            el.min_w_0().truncate()
        } else {
            el
        }
    }
}
