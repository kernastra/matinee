use std::borrow::Cow;

use gpui::{
    Animation, AnimationExt, App, AssetSource, IntoElement, RenderOnce, SharedString, Styled,
    Transformation, Window, percentage, px, svg,
};

use crate::{
    ActiveTheme,
    tokens::{Color, MotionDuration},
};

macro_rules! icons {
    ($($variant:ident => $file:literal),* $(,)?) => {
        /// The framework's built-in icon set: generic interface glyphs only.
        /// Product-specific iconography belongs in the application.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum IconName { $($variant),* }

        impl IconName {
            pub const ALL: &'static [IconName] = &[$(IconName::$variant),*];

            pub const fn file_name(self) -> &'static str {
                match self { $(IconName::$variant => $file),* }
            }

            fn bytes(self) -> &'static [u8] {
                match self {
                    $(IconName::$variant => include_bytes!(concat!("../../assets/icons/", $file, ".svg"))),*
                }
            }
        }
    };
}

icons! {
    Check => "check",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    Close => "close",
    Heart => "heart",
    Info => "info",
    More => "more",
    Pause => "pause",
    Play => "play",
    Plus => "plus",
    Search => "search",
    Sliders => "sliders",
    Spinner => "spinner",
    Trash => "trash",
}

const ASSET_PREFIX: &str = "atelier/icons/";

impl IconName {
    pub fn asset_path(self) -> SharedString {
        format!("{ASSET_PREFIX}{}.svg", self.file_name()).into()
    }
}

/// Serves the framework's embedded assets. Applications compose it with
/// their own sources (see `atelier_app::AppAssets`).
#[derive(Clone, Copy, Debug, Default)]
pub struct UiAssets;

impl AssetSource for UiAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let Some(name) = path
            .strip_prefix(ASSET_PREFIX)
            .and_then(|p| p.strip_suffix(".svg"))
        else {
            return Ok(None);
        };
        Ok(IconName::ALL
            .iter()
            .find(|icon| icon.file_name() == name)
            .map(|icon| Cow::Borrowed(icon.bytes())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(IconName::ALL
            .iter()
            .map(|icon| icon.asset_path())
            .filter(|p| p.starts_with(path))
            .collect())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum IconSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl IconSize {
    pub const fn value(self) -> f32 {
        match self {
            IconSize::Small => 14.0,
            IconSize::Medium => 16.0,
            IconSize::Large => 20.0,
        }
    }
}

/// A monochrome icon tinted with the current text color unless overridden.
#[derive(IntoElement, Debug)]
pub struct Icon {
    name: IconName,
    size: IconSize,
    color: Option<Color>,
    spinning: bool,
}

impl Icon {
    pub fn new(name: IconName) -> Self {
        Self {
            name,
            size: IconSize::default(),
            color: None,
            spinning: false,
        }
    }

    pub fn size(mut self, size: IconSize) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Continuously rotates the icon (activity indicators). Under reduced
    /// motion the icon is rendered static.
    pub fn spinning(mut self, spinning: bool) -> Self {
        self.spinning = spinning;
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let color = self.color.unwrap_or(theme.colors.text.primary);
        let size = px(self.size.value());
        let icon = svg()
            .path(self.name.asset_path())
            .flex_none()
            .size(size)
            .text_color(color);

        let animate = self.spinning && theme.motion.allows_continuous(cx.ui_preferences().motion());
        if animate {
            // One revolution spans several `slow` beats so the indicator reads
            // as calm activity rather than urgency.
            let period = theme.motion.duration(MotionDuration::Slow) * 3;
            icon.with_animation(
                SharedString::from(format!("spin-{}", self.name.file_name())),
                Animation::new(period).repeat(),
                |icon, delta| icon.with_transformation(Transformation::rotate(percentage(delta))),
            )
            .into_any_element()
        } else {
            icon.into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_served_and_is_svg() {
        for icon in IconName::ALL {
            let bytes = UiAssets
                .load(&icon.asset_path())
                .expect("load")
                .unwrap_or_else(|| panic!("{icon:?} missing"));
            assert!(bytes.starts_with(b"<svg"), "{icon:?}");
        }
    }

    #[test]
    fn unknown_paths_are_not_claimed() {
        assert!(UiAssets.load("app/logo.png").unwrap().is_none());
        assert!(UiAssets.load("atelier/icons/nope.svg").unwrap().is_none());
    }
}
