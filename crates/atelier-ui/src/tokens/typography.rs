/// The platform UI font. GPUI resolves this name to San Francisco on macOS,
/// Segoe UI on Windows, and the fontconfig default on Linux.
pub const SYSTEM_UI_FONT: &str = ".SystemUIFont";

/// Which family a text role draws from. Themes choose the actual families.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontRole {
    /// Editorial, expressive headings.
    Display,
    /// Interface and body copy.
    Interface,
    /// Metadata, code, and compact chrome.
    Monospace,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontFamilies {
    pub display: &'static str,
    pub interface: &'static str,
    pub monospace: &'static str,
}

impl FontFamilies {
    pub const SYSTEM: FontFamilies = FontFamilies {
        display: SYSTEM_UI_FONT,
        interface: SYSTEM_UI_FONT,
        monospace: "monospace",
    };

    pub fn get(&self, role: FontRole) -> &'static str {
        match role {
            FontRole::Display => self.display,
            FontRole::Interface => self.interface,
            FontRole::Monospace => self.monospace,
        }
    }
}

/// Semantic text roles. Components and apps pick a role, never a size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextRole {
    Display,
    Title,
    Heading,
    Subheading,
    Body,
    Label,
    Metadata,
    Caption,
}

impl TextRole {
    pub const ALL: [TextRole; 8] = [
        TextRole::Display,
        TextRole::Title,
        TextRole::Heading,
        TextRole::Subheading,
        TextRole::Body,
        TextRole::Label,
        TextRole::Metadata,
        TextRole::Caption,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            TextRole::Display => "display",
            TextRole::Title => "title",
            TextRole::Heading => "heading",
            TextRole::Subheading => "subheading",
            TextRole::Body => "body",
            TextRole::Label => "label",
            TextRole::Metadata => "metadata",
            TextRole::Caption => "caption",
        }
    }
}

/// CSS-style numeric font weight (100–900).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Weight(pub u16);

impl Weight {
    pub const REGULAR: Weight = Weight(400);
    pub const MEDIUM: Weight = Weight(500);
    pub const SEMIBOLD: Weight = Weight(600);
    pub const BOLD: Weight = Weight(700);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeStyle {
    pub font: FontRole,
    /// Font size in logical pixels.
    pub size: f32,
    /// Line height in logical pixels. Kept on a 2px grid for baseline rhythm.
    pub line_height: f32,
    pub weight: Weight,
}

impl TypeStyle {
    pub const fn new(font: FontRole, size: f32, line_height: f32, weight: Weight) -> Self {
        Self {
            font,
            size,
            line_height,
            weight,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeScale {
    pub display: TypeStyle,
    pub title: TypeStyle,
    pub heading: TypeStyle,
    pub subheading: TypeStyle,
    pub body: TypeStyle,
    pub label: TypeStyle,
    pub metadata: TypeStyle,
    pub caption: TypeStyle,
}

impl TypeScale {
    pub fn get(&self, role: TextRole) -> TypeStyle {
        match role {
            TextRole::Display => self.display,
            TextRole::Title => self.title,
            TextRole::Heading => self.heading,
            TextRole::Subheading => self.subheading,
            TextRole::Body => self.body,
            TextRole::Label => self.label,
            TextRole::Metadata => self.metadata,
            TextRole::Caption => self.caption,
        }
    }

    /// A neutral desktop scale suited to system fonts.
    pub const fn standard() -> Self {
        use FontRole::*;
        Self {
            display: TypeStyle::new(Display, 40.0, 48.0, Weight::BOLD),
            title: TypeStyle::new(Display, 28.0, 34.0, Weight::SEMIBOLD),
            heading: TypeStyle::new(Interface, 20.0, 26.0, Weight::SEMIBOLD),
            subheading: TypeStyle::new(Interface, 16.0, 22.0, Weight::SEMIBOLD),
            body: TypeStyle::new(Interface, 14.0, 20.0, Weight::REGULAR),
            label: TypeStyle::new(Interface, 13.0, 18.0, Weight::MEDIUM),
            metadata: TypeStyle::new(Monospace, 12.0, 16.0, Weight::REGULAR),
            caption: TypeStyle::new(Interface, 11.0, 14.0, Weight::REGULAR),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Typography {
    pub families: FontFamilies,
    pub scale: TypeScale,
}

impl Typography {
    pub fn style(&self, role: TextRole) -> TypeStyle {
        self.scale.get(role)
    }

    pub fn family(&self, role: TextRole) -> &'static str {
        self.families.get(self.style(role).font)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_scale_is_monotonic_and_readable() {
        let scale = TypeScale::standard();
        let sizes: Vec<f32> = TextRole::ALL.iter().map(|r| scale.get(*r).size).collect();
        assert!(
            sizes.windows(2).all(|w| w[0] > w[1]),
            "roles should step down in size: {sizes:?}"
        );
        for role in TextRole::ALL {
            let style = scale.get(role);
            assert!(style.line_height >= style.size, "{role:?} line height");
            assert_eq!(style.line_height % 2.0, 0.0, "{role:?} off the 2px grid");
        }
    }
}
