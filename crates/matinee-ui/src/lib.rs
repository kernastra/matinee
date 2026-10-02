//! Matinee's presentation layer on top of the generic framework.
//!
//! Brand names (Midnight Navy, Marquee Amber, ...) exist only here. The
//! framework sees nothing but semantic roles. Source of truth for the
//! palette and type: `docs/design-spec.md` and `src/styles.css` of the
//! shipping app.

mod theme;

pub use theme::{matinee_theme, palette};
