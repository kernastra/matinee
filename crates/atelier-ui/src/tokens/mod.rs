//! Renderer-independent design tokens.
//!
//! Nothing in this module touches GPUI, so tokens can be unit-tested and
//! documented in isolation. Conversion to GPUI types lives in `crate::bridge`.

mod color;
mod colors;
mod motion;
mod shape;
mod spacing;
mod typography;

pub use color::Color;
pub use colors::{
    BorderColors, ColorRole, ColorTokens, ControlColors, FocusColors, SurfaceColors, TextColors,
};
pub use motion::{MotionDuration, MotionPreference, MotionScale, Spring, SpringParams};
pub use shape::{Elevation, ElevationScale, Radius, RadiusScale, Shadow};
pub use spacing::{SPACING_UNIT, Space};
pub use typography::{
    FontFamilies, FontRole, SYSTEM_UI_FONT, TextRole, TypeScale, TypeStyle, Typography, Weight,
};
