//! Native Library: browse every movie or series, sorted and filtered on
//! the server, a page at a time.
//!
//! [`model::LibraryModel`] owns the query, the loaded pages, and the
//! request tickets. The screen paints it in Atelier's virtualized grid and
//! asks the shared artwork loader only for the cards near the viewport.
//! See `docs/architecture/library.md`.

mod load;
pub(crate) mod model;
mod preview;
mod screen;

pub(crate) use preview::LibraryPreview;
pub(crate) use screen::{LibraryEvent, LibraryScreen};

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod view_tests;
