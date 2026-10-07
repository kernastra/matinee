//! Native Search: type a title, see matching movies, series, and episodes,
//! and open one.
//!
//! [`model::SearchModel`] owns the text, the effective query, the pages, and
//! the request tickets. It has no GPUI and no HTTP. The screen paints it in
//! Atelier's virtualized grid, and asks the shared artwork loader only for
//! the cards near the viewport. See `docs/architecture/search.md`.

mod load;
pub(crate) mod model;
mod screen;

#[cfg(test)]
mod model_tests;

pub(crate) use screen::{SearchEvent, SearchScreen};
