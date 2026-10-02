//! Register font files with GPUI.
//!
//! Applications that ship brand fonts call [`add_fonts`] at startup.
//! Atelier itself stays font-neutral: themes name families, and this
//! function is the only framework API that accepts font bytes.

use std::borrow::Cow;

use gpui::{App, Result};

/// Adds font files (TTF or OTF) to the process text system.
///
/// Family and weight come from each file's name tables. Call this before
/// opening windows so the first frame can resolve theme families.
pub fn add_fonts(cx: &mut App, fonts: impl IntoIterator<Item = Cow<'static, [u8]>>) -> Result<()> {
    cx.text_system().add_fonts(fonts.into_iter().collect())
}
