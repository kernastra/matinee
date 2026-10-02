//! Matinee's presentation layer on top of the generic framework.
//!
//! Brand names (Midnight Navy, Marquee Amber, ...) exist only here. The
//! framework sees nothing but semantic roles. Source of truth for the
//! palette and type: `docs/design-spec.md` and `src/styles.css` of the
//! shipping app.

mod fonts;
mod theme;

pub use fonts::{bundled_font_data, load_bundled_fonts};
pub use theme::{matinee_theme, palette};
