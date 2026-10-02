//! Conversions from framework tokens to GPUI values.
//!
//! This is the only module in `atelier-ui` that knows how tokens are
//! represented inside GPUI. When GPUI changes, this file and the component
//! render functions are the expected blast radius.

use gpui::{BoxShadow, Fill, FontWeight, Hsla, Pixels, Rgba, point, px};

use crate::tokens::{Color, Shadow, Space, Weight};

impl From<Color> for Hsla {
    fn from(c: Color) -> Self {
        Rgba {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
        .into()
    }
}

impl From<Color> for Fill {
    fn from(c: Color) -> Self {
        Hsla::from(c).into()
    }
}

impl From<Weight> for FontWeight {
    fn from(w: Weight) -> Self {
        FontWeight(w.0 as f32)
    }
}

impl From<Space> for Pixels {
    fn from(s: Space) -> Self {
        px(s.value())
    }
}

impl Space {
    pub fn px(self) -> Pixels {
        self.into()
    }
}

pub(crate) fn box_shadows(shadows: &[Shadow]) -> Vec<BoxShadow> {
    shadows
        .iter()
        .map(|s| BoxShadow {
            color: s.color.into(),
            offset: point(px(0.0), px(s.offset_y)),
            blur_radius: px(s.blur),
            spread_radius: px(s.spread),
        })
        .collect()
}
