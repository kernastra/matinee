use std::sync::OnceLock;

use gpui::{App, Global};

use crate::tokens::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Appearance {
    Light,
    Dark,
}

/// A complete set of semantic tokens. Applications build their own `Theme`
/// (mapping brand palettes onto these roles); components read only from it.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    pub appearance: Appearance,
    pub colors: ColorTokens,
    pub typography: Typography,
    pub radius: RadiusScale,
    pub elevation: ElevationScale,
    pub motion: MotionScale,
}

impl Global for Theme {}

impl Theme {
    /// Neutral graphite dark theme; the framework default.
    pub fn neutral_dark() -> Self {
        let white = Color::hex(0xffffff);
        Theme {
            name: "Neutral Dark",
            appearance: Appearance::Dark,
            colors: ColorTokens {
                text: TextColors {
                    primary: Color::hex(0xf2f3f5),
                    secondary: Color::hex(0xb7bbc2),
                    muted: Color::hex(0x8d929b),
                    disabled: Color::hex(0x5d6169),
                    on_accent: white,
                    on_destructive: white,
                },
                surface: SurfaceColors {
                    canvas: Color::hex(0x16171a),
                    panel: Color::hex(0x1d1f23),
                    elevated: Color::hex(0x26282d),
                    overlay: Color::hex(0x000000).with_alpha(0.55),
                },
                border: BorderColors {
                    subtle: white.with_alpha(0.06),
                    default: white.with_alpha(0.11),
                    strong: white.with_alpha(0.2),
                },
                control: ControlColors {
                    accent: Color::hex(0x2563eb),
                    accent_hover: Color::hex(0x2f6bee),
                    accent_pressed: Color::hex(0x1d4fc4),
                    neutral: Color::hex(0x2c2f34),
                    neutral_hover: Color::hex(0x34373d),
                    neutral_pressed: Color::hex(0x3d4047),
                    subtle_hover: white.with_alpha(0.06),
                    subtle_pressed: white.with_alpha(0.11),
                    destructive: Color::hex(0xc4313c),
                    destructive_hover: Color::hex(0xcc3843),
                    destructive_pressed: Color::hex(0xa82731),
                    disabled: Color::hex(0x24262a),
                },
                focus: FocusColors {
                    ring: Color::hex(0x6aa2ff),
                },
            },
            typography: Typography {
                families: FontFamilies::SYSTEM,
                scale: TypeScale::standard(),
            },
            radius: RadiusScale::standard(),
            elevation: ElevationScale::from_shadow_color(Color::hex(0x000000)),
            motion: MotionScale::standard(),
        }
    }

    /// Neutral light theme.
    pub fn neutral_light() -> Self {
        let black = Color::hex(0x000000);
        Theme {
            name: "Neutral Light",
            appearance: Appearance::Light,
            colors: ColorTokens {
                text: TextColors {
                    primary: Color::hex(0x1b1c1f),
                    secondary: Color::hex(0x4a4e56),
                    muted: Color::hex(0x666b74),
                    disabled: Color::hex(0xa3a7ae),
                    on_accent: Color::hex(0xffffff),
                    on_destructive: Color::hex(0xffffff),
                },
                surface: SurfaceColors {
                    canvas: Color::hex(0xf5f5f6),
                    panel: Color::hex(0xffffff),
                    elevated: Color::hex(0xffffff),
                    overlay: black.with_alpha(0.32),
                },
                border: BorderColors {
                    subtle: black.with_alpha(0.06),
                    default: black.with_alpha(0.12),
                    strong: black.with_alpha(0.24),
                },
                control: ControlColors {
                    accent: Color::hex(0x2563eb),
                    accent_hover: Color::hex(0x1f58d6),
                    accent_pressed: Color::hex(0x1a4bb8),
                    neutral: Color::hex(0xe8e9eb),
                    neutral_hover: Color::hex(0xdfe1e4),
                    neutral_pressed: Color::hex(0xd3d6da),
                    subtle_hover: black.with_alpha(0.05),
                    subtle_pressed: black.with_alpha(0.09),
                    destructive: Color::hex(0xc4313c),
                    destructive_hover: Color::hex(0xb02a34),
                    destructive_pressed: Color::hex(0x98232c),
                    disabled: Color::hex(0xececee),
                },
                focus: FocusColors {
                    ring: Color::hex(0x2563eb),
                },
            },
            typography: Typography {
                families: FontFamilies::SYSTEM,
                scale: TypeScale::standard(),
            },
            radius: RadiusScale::standard(),
            elevation: ElevationScale::from_shadow_color(Color::hex(0x10131a)),
            motion: MotionScale::standard(),
        }
    }

    /// Checks the theme against the framework's accessibility contract.
    /// Every shipped theme must return an empty list (enforced by tests).
    pub fn validate(&self) -> Vec<ThemeIssue> {
        let c = &self.colors;
        let mut issues = Vec::new();
        let mut require = |fg: ColorRole, bg: ColorRole, minimum: f32| {
            let ratio = c.get(fg).contrast_ratio(c.get(bg));
            if ratio < minimum {
                issues.push(ThemeIssue {
                    foreground: fg,
                    background: bg,
                    ratio,
                    minimum,
                });
            }
        };

        use ColorRole::*;
        for surface in [SurfaceCanvas, SurfacePanel, SurfaceElevated] {
            require(TextPrimary, surface, 7.0);
            require(TextSecondary, surface, 4.5);
            require(TextMuted, surface, 4.5);
        }
        require(TextPrimary, ControlNeutral, 4.5);
        require(TextPrimary, ControlNeutralHover, 4.5);
        require(TextPrimary, ControlNeutralPressed, 4.5);
        for accent in [ControlAccent, ControlAccentHover, ControlAccentPressed] {
            require(TextOnAccent, accent, 4.5);
        }
        for destructive in [
            ControlDestructive,
            ControlDestructiveHover,
            ControlDestructivePressed,
        ] {
            require(TextOnDestructive, destructive, 4.5);
        }
        // Non-text contrast (WCAG 1.4.11) for focus indication.
        require(FocusRing, SurfaceCanvas, 3.0);
        require(FocusRing, SurfacePanel, 3.0);
        issues
    }
}

/// A failed contrast requirement reported by [`Theme::validate`].
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeIssue {
    pub foreground: ColorRole,
    pub background: ColorRole,
    pub ratio: f32,
    pub minimum: f32,
}

impl std::fmt::Display for ThemeIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} on {}: {:.2}:1 (needs {:.1}:1)",
            self.foreground.token_name(),
            self.background.token_name(),
            self.ratio,
            self.minimum
        )
    }
}

/// Environment preferences that components must respect but that are not
/// part of a theme (they come from the platform or the user's settings).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiPreferences {
    pub motion: MotionPreference,
}

impl Global for UiPreferences {}

/// Read access to the active theme and preferences from any GPUI context.
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
    fn ui_preferences(&self) -> &UiPreferences;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        static FALLBACK: OnceLock<Theme> = OnceLock::new();
        self.try_global::<Theme>()
            .unwrap_or_else(|| FALLBACK.get_or_init(Theme::neutral_dark))
    }

    fn ui_preferences(&self) -> &UiPreferences {
        static FALLBACK: OnceLock<UiPreferences> = OnceLock::new();
        self.try_global::<UiPreferences>()
            .unwrap_or_else(|| FALLBACK.get_or_init(UiPreferences::default))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_themes_meet_contrast_contract() {
        for theme in [Theme::neutral_dark(), Theme::neutral_light()] {
            let issues = theme.validate();
            assert!(
                issues.is_empty(),
                "{}:\n{}",
                theme.name,
                issues
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }

    #[test]
    fn validation_reports_failures() {
        let mut theme = Theme::neutral_dark();
        theme.colors.text.muted = theme.colors.surface.canvas;
        let issues = theme.validate();
        assert!(
            issues
                .iter()
                .any(|i| i.foreground == ColorRole::TextMuted && i.ratio < 1.01)
        );
    }

    #[test]
    fn appearance_matches_canvas_luminance() {
        for theme in [Theme::neutral_dark(), Theme::neutral_light()] {
            let dark = theme.colors.surface.canvas.relative_luminance() < 0.2;
            assert_eq!(dark, theme.appearance == Appearance::Dark, "{}", theme.name);
        }
    }
}
