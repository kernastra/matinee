use super::Color;

/// Semantic color tokens. Components read only these roles; brand palettes
/// live in application themes and are mapped onto these roles there.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorTokens {
    pub text: TextColors,
    pub surface: SurfaceColors,
    pub border: BorderColors,
    pub control: ControlColors,
    pub focus: FocusColors,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextColors {
    pub primary: Color,
    pub secondary: Color,
    pub muted: Color,
    pub disabled: Color,
    /// Foreground placed on `control.accent`.
    pub on_accent: Color,
    /// Foreground placed on `control.destructive`.
    pub on_destructive: Color,
    /// Error and invalid text. Meets 4.5:1 on canvas, panel, and elevated
    /// surfaces. `control.destructive` is a fill and is too dark for small text.
    pub danger: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceColors {
    /// The window background.
    pub canvas: Color,
    /// Grouped content sitting directly on the canvas (sidebars, cards).
    pub panel: Color,
    /// Raised content (popovers, menus, focused cards).
    pub elevated: Color,
    /// Scrim behind modal content.
    pub overlay: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BorderColors {
    pub subtle: Color,
    pub default: Color,
    pub strong: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlColors {
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_pressed: Color,
    /// Neutral filled control (secondary buttons).
    pub neutral: Color,
    pub neutral_hover: Color,
    pub neutral_pressed: Color,
    /// Background washes for chromeless (subtle) controls.
    pub subtle_hover: Color,
    pub subtle_pressed: Color,
    pub destructive: Color,
    pub destructive_hover: Color,
    pub destructive_pressed: Color,
    pub disabled: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FocusColors {
    pub ring: Color,
}

/// A named color role, used by documentation surfaces (the Gallery) and
/// validation to enumerate tokens without reflecting over struct fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorRole {
    TextPrimary,
    TextSecondary,
    TextMuted,
    TextDisabled,
    TextOnAccent,
    TextOnDestructive,
    TextDanger,
    SurfaceCanvas,
    SurfacePanel,
    SurfaceElevated,
    SurfaceOverlay,
    BorderSubtle,
    BorderDefault,
    BorderStrong,
    ControlAccent,
    ControlAccentHover,
    ControlAccentPressed,
    ControlNeutral,
    ControlNeutralHover,
    ControlNeutralPressed,
    ControlSubtleHover,
    ControlSubtlePressed,
    ControlDestructive,
    ControlDestructiveHover,
    ControlDestructivePressed,
    ControlDisabled,
    FocusRing,
}

impl ColorRole {
    pub const ALL: [ColorRole; 27] = [
        ColorRole::TextPrimary,
        ColorRole::TextSecondary,
        ColorRole::TextMuted,
        ColorRole::TextDisabled,
        ColorRole::TextOnAccent,
        ColorRole::TextOnDestructive,
        ColorRole::TextDanger,
        ColorRole::SurfaceCanvas,
        ColorRole::SurfacePanel,
        ColorRole::SurfaceElevated,
        ColorRole::SurfaceOverlay,
        ColorRole::BorderSubtle,
        ColorRole::BorderDefault,
        ColorRole::BorderStrong,
        ColorRole::ControlAccent,
        ColorRole::ControlAccentHover,
        ColorRole::ControlAccentPressed,
        ColorRole::ControlNeutral,
        ColorRole::ControlNeutralHover,
        ColorRole::ControlNeutralPressed,
        ColorRole::ControlSubtleHover,
        ColorRole::ControlSubtlePressed,
        ColorRole::ControlDestructive,
        ColorRole::ControlDestructiveHover,
        ColorRole::ControlDestructivePressed,
        ColorRole::ControlDisabled,
        ColorRole::FocusRing,
    ];

    /// The dotted token name used in documentation, e.g. `text.primary`.
    pub const fn token_name(self) -> &'static str {
        match self {
            ColorRole::TextPrimary => "text.primary",
            ColorRole::TextSecondary => "text.secondary",
            ColorRole::TextMuted => "text.muted",
            ColorRole::TextDisabled => "text.disabled",
            ColorRole::TextOnAccent => "text.on_accent",
            ColorRole::TextOnDestructive => "text.on_destructive",
            ColorRole::TextDanger => "text.danger",
            ColorRole::SurfaceCanvas => "surface.canvas",
            ColorRole::SurfacePanel => "surface.panel",
            ColorRole::SurfaceElevated => "surface.elevated",
            ColorRole::SurfaceOverlay => "surface.overlay",
            ColorRole::BorderSubtle => "border.subtle",
            ColorRole::BorderDefault => "border.default",
            ColorRole::BorderStrong => "border.strong",
            ColorRole::ControlAccent => "control.accent",
            ColorRole::ControlAccentHover => "control.accent_hover",
            ColorRole::ControlAccentPressed => "control.accent_pressed",
            ColorRole::ControlNeutral => "control.neutral",
            ColorRole::ControlNeutralHover => "control.neutral_hover",
            ColorRole::ControlNeutralPressed => "control.neutral_pressed",
            ColorRole::ControlSubtleHover => "control.subtle_hover",
            ColorRole::ControlSubtlePressed => "control.subtle_pressed",
            ColorRole::ControlDestructive => "control.destructive",
            ColorRole::ControlDestructiveHover => "control.destructive_hover",
            ColorRole::ControlDestructivePressed => "control.destructive_pressed",
            ColorRole::ControlDisabled => "control.disabled",
            ColorRole::FocusRing => "focus.ring",
        }
    }
}

impl ColorTokens {
    pub fn get(&self, role: ColorRole) -> Color {
        match role {
            ColorRole::TextPrimary => self.text.primary,
            ColorRole::TextSecondary => self.text.secondary,
            ColorRole::TextMuted => self.text.muted,
            ColorRole::TextDisabled => self.text.disabled,
            ColorRole::TextOnAccent => self.text.on_accent,
            ColorRole::TextOnDestructive => self.text.on_destructive,
            ColorRole::TextDanger => self.text.danger,
            ColorRole::SurfaceCanvas => self.surface.canvas,
            ColorRole::SurfacePanel => self.surface.panel,
            ColorRole::SurfaceElevated => self.surface.elevated,
            ColorRole::SurfaceOverlay => self.surface.overlay,
            ColorRole::BorderSubtle => self.border.subtle,
            ColorRole::BorderDefault => self.border.default,
            ColorRole::BorderStrong => self.border.strong,
            ColorRole::ControlAccent => self.control.accent,
            ColorRole::ControlAccentHover => self.control.accent_hover,
            ColorRole::ControlAccentPressed => self.control.accent_pressed,
            ColorRole::ControlNeutral => self.control.neutral,
            ColorRole::ControlNeutralHover => self.control.neutral_hover,
            ColorRole::ControlNeutralPressed => self.control.neutral_pressed,
            ColorRole::ControlSubtleHover => self.control.subtle_hover,
            ColorRole::ControlSubtlePressed => self.control.subtle_pressed,
            ColorRole::ControlDestructive => self.control.destructive,
            ColorRole::ControlDestructiveHover => self.control.destructive_hover,
            ColorRole::ControlDestructivePressed => self.control.destructive_pressed,
            ColorRole::ControlDisabled => self.control.disabled,
            ColorRole::FocusRing => self.focus.ring,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn token_names_are_unique_and_dotted() {
        let names: HashSet<_> = ColorRole::ALL.iter().map(|r| r.token_name()).collect();
        assert_eq!(names.len(), ColorRole::ALL.len());
        assert!(names.iter().all(|n| n.contains('.')));
    }
}
