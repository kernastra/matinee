//! Native Home: the authenticated root of the application.
//!
//! [`model::HomeModel`] owns the shelves, the hero choice, and where focus
//! returns. [`screen::HomeScreen`] paints it, runs one request per shelf on
//! the application [`crate::runtime::ServiceRuntime`], and asks the shared
//! [`crate::artwork::ArtworkLoader`] for images. Cards open the existing
//! Details; the hero's Play or Resume opens the existing Player. Home emits
//! that intent and the shell acts on it. See `docs/architecture/home.md`.

mod load;
mod model;
mod preview;
mod screen;

pub(crate) use preview::HomePreview;
pub(crate) use screen::{HomeEvent, HomeScreen};

#[cfg(test)]
mod flow_tests;
