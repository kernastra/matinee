//! Native Calendar: upcoming movie releases from Radarr and episodes from
//! Sonarr, by day.
//!
//! [`model::CalendarModel`] owns the month, the selection, each source's link
//! and request tickets, and the events. It has no GPUI, no HTTP, and no clock
//! of its own. [`event`] is the day rule. [`grid`] is the month arithmetic.
//! [`load`] runs the two integration calls on the service runtime. The screen
//! paints the model with Atelier's `VirtualGrid`. See
//! `docs/architecture/calendar.md`.

mod event;
mod grid;
pub(crate) mod load;
pub(crate) mod model;
mod preview;
mod screen;

#[cfg(test)]
mod model_tests;

#[cfg(test)]
pub(crate) use event::MediaFilter;
pub(crate) use load::CalendarService;
pub(crate) use preview::CalendarPreview;
pub(crate) use screen::{CalendarScreen, CalendarScreenEvent};

#[cfg(test)]
mod view_tests;

#[cfg(test)]
mod load_tests;
