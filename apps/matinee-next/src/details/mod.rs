//! Native Details.
//!
//! [`model::DetailsModel`] owns the screen state and decides what to load.
//! [`screen::DetailsScreen`] paints it, runs its requests on the application
//! [`crate::runtime::ServiceRuntime`], and asks the shared
//! [`crate::artwork::ArtworkLoader`] for images. Play and Resume do not start
//! playback here: the screen emits [`DetailsEvent::Play`] and the shell opens
//! the existing Player. When the Player closes, the shell calls
//! [`DetailsScreen::resume`], which refreshes the title's progress.

mod load;
mod model;
mod preview;
mod screen;

pub(crate) use model::{PlayAction, meta_line, progress_fraction, runtime_label, summary};
pub(crate) use preview::DetailsPreview;
pub(crate) use screen::{DetailsEvent, DetailsScreen};
