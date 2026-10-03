use atelier_ui::{
    Appearance, Theme,
    tokens::{
        BorderColors, Color, ColorTokens, ControlColors, ElevationScale, FocusColors, FontFamilies,
        FontRole, MotionScale, RadiusScale, SurfaceColors, TextColors, TypeScale, TypeStyle,
        Typography, Weight,
    },
};

/// The Matinee brand palette (docs/design-spec.md).
pub mod palette {
    use atelier_ui::tokens::Color;

    /// Primary authenticated content surface (the painted swatch).
    pub const MIDNIGHT_NAVY: Color = Color::hex(0x080e15);
    /// Login photography blending surface; used for panels.
    pub const PRINTED_NAVY: Color = Color::hex(0x111820);
    pub const PROJECTION_ROOM: Color = Color::hex(0x1b232b);
    pub const THEATER_BROWN: Color = Color::hex(0x29231f);
    pub const TICKET_CREAM: Color = Color::hex(0xf6eedd);
    /// `--muted` in the shipping stylesheet.
    pub const USHER_GREY: Color = Color::hex(0xa89e8d);
    pub const MARQUEE_AMBER: Color = Color::hex(0xe6a452);
    /// `--accent-hover` in the shipping stylesheet.
    pub const MARQUEE_AMBER_LIT: Color = Color::hex(0xf0b76d);
    pub const CURTAIN_BURGUNDY: Color = Color::hex(0x963f47);
    pub const FADED_TEAL: Color = Color::hex(0x658184);
    /// Ink used on amber (the stylesheet's `::selection` color).
    pub const LOBBY_INK: Color = Color::hex(0x1c140c);
}

/// Font families as specified for Matinee. Static OFL instances are bundled
/// in `assets/fonts` and registered by [`crate::load_bundled_fonts`].
pub const FAMILIES: FontFamilies = FontFamilies {
    display: "Fraunces",
    interface: "Manrope",
    monospace: "IBM Plex Mono",
};

/// Matinee's 64/40/26/17/16/13/11 reference scale (hero, page, section,
/// poster title, body, metadata, caption), with line heights on a 2px grid.
pub const fn matinee_type_scale() -> TypeScale {
    use FontRole::*;
    TypeScale {
        display: TypeStyle::new(Display, 64.0, 70.0, Weight::SEMIBOLD),
        title: TypeStyle::new(Display, 40.0, 46.0, Weight::SEMIBOLD),
        heading: TypeStyle::new(Display, 26.0, 32.0, Weight::SEMIBOLD),
        subheading: TypeStyle::new(Display, 17.0, 22.0, Weight::SEMIBOLD),
        body: TypeStyle::new(Interface, 16.0, 24.0, Weight::REGULAR),
        label: TypeStyle::new(Interface, 14.0, 20.0, Weight::SEMIBOLD),
        metadata: TypeStyle::new(Monospace, 13.0, 18.0, Weight::REGULAR),
        caption: TypeStyle::new(Monospace, 11.0, 14.0, Weight::REGULAR),
    }
}

/// Matinee's dark-only theme.
pub fn matinee_theme() -> Theme {
    use palette::*;
    let cream = TICKET_CREAM;
    Theme {
        name: "Matinee",
        appearance: Appearance::Dark,
        colors: ColorTokens {
            text: TextColors {
                primary: cream,
                secondary: Color::hex(0xcdc4b3),
                muted: USHER_GREY,
                disabled: Color::hex(0x5f5a52),
                on_accent: LOBBY_INK,
                on_destructive: cream,
                danger: Color::hex(0xe8a8ad),
            },
            surface: SurfaceColors {
                canvas: MIDNIGHT_NAVY,
                panel: PRINTED_NAVY,
                elevated: PROJECTION_ROOM,
                overlay: MIDNIGHT_NAVY.with_alpha(0.72),
            },
            border: BorderColors {
                subtle: cream.with_alpha(0.07),
                default: cream.with_alpha(0.13),
                strong: cream.with_alpha(0.24),
            },
            control: ControlColors {
                accent: MARQUEE_AMBER,
                accent_hover: MARQUEE_AMBER_LIT,
                accent_pressed: Color::hex(0xd18f3f),
                neutral: PROJECTION_ROOM,
                neutral_hover: Color::hex(0x232c35),
                neutral_pressed: Color::hex(0x2b3540),
                subtle_hover: cream.with_alpha(0.08),
                subtle_pressed: cream.with_alpha(0.13),
                destructive: CURTAIN_BURGUNDY,
                destructive_hover: Color::hex(0xa4474f),
                destructive_pressed: Color::hex(0x82353c),
                disabled: Color::hex(0x141b23),
            },
            // Amber carries focus and selection in Matinee.
            focus: FocusColors {
                ring: MARQUEE_AMBER,
            },
        },
        typography: Typography {
            families: FAMILIES,
            scale: matinee_type_scale(),
        },
        radius: RadiusScale {
            small: 5.0,
            medium: 7.0,
            large: 10.0,
        },
        elevation: ElevationScale::from_shadow_color(Color::hex(0x000000)),
        motion: MotionScale::standard(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atelier_ui::tokens::TextRole;

    #[test]
    fn matinee_theme_meets_framework_contrast_contract() {
        let issues = matinee_theme().validate();
        assert!(
            issues.is_empty(),
            "{}",
            issues
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn brand_roles_follow_design_spec() {
        let t = matinee_theme();
        assert_eq!(t.colors.surface.canvas, palette::MIDNIGHT_NAVY);
        assert_eq!(t.colors.text.primary, palette::TICKET_CREAM);
        assert_eq!(t.colors.control.accent, palette::MARQUEE_AMBER);
        assert_eq!(t.colors.focus.ring, palette::MARQUEE_AMBER);
        assert_eq!(t.colors.control.destructive, palette::CURTAIN_BURGUNDY);
        assert_eq!(t.appearance, Appearance::Dark);
    }

    #[test]
    fn typography_follows_reference_scale() {
        let t = matinee_theme().typography;
        let sizes: Vec<f32> = [
            TextRole::Display,
            TextRole::Title,
            TextRole::Heading,
            TextRole::Subheading,
            TextRole::Body,
            TextRole::Metadata,
            TextRole::Caption,
        ]
        .iter()
        .map(|r| t.style(*r).size)
        .collect();
        assert_eq!(sizes, [64.0, 40.0, 26.0, 17.0, 16.0, 13.0, 11.0]);
        assert_eq!(t.family(TextRole::Display), "Fraunces");
        assert_eq!(t.family(TextRole::Body), "Manrope");
        assert_eq!(t.family(TextRole::Metadata), "IBM Plex Mono");
    }
}
