use gpui::{Styled, px};

use crate::{
    Theme, bridge,
    tokens::{Elevation, Radius},
};

/// Token-driven styling for any GPUI element, so app code can apply shared
/// treatments (elevation, corners) without touching GPUI style types.
pub trait StyledExt: Styled + Sized {
    fn elevation(self, theme: &Theme, level: Elevation) -> Self {
        self.shadow(bridge::box_shadows(theme.elevation.get(level)))
    }

    fn corner_radius(self, theme: &Theme, radius: Radius) -> Self {
        self.rounded(px(theme.radius.get(radius)))
    }
}

impl<T: Styled> StyledExt for T {}
