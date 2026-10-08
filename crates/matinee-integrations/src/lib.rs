//! Radarr and Sonarr for Matinee.
//!
//! UI, Tauri, GPUI, and Atelier do not appear here. API keys are stored through
//! [`matinee_secrets`] in the shipping namespace
//! `dev.sean.matinee.media-integrations`. This crate does not call `keyring`
//! itself.
//!
//! # Runtime
//!
//! [`ReqwestTransport`] must be polled on a Tokio runtime supplied by the
//! application. This crate does not create one. Tests use [`Transport`] and
//! `futures::executor::block_on`.
//!
//! # Calendar cache
//!
//! [`Integrations::upcoming`] owns the five-minute cache. The key is the
//! configured Radarr URL, Sonarr URL, and the requested range. Concurrent
//! identical calls share one fetch. Changing or removing an integration, or
//! [`Integrations::invalidate_cache`], drops the cache. A fetch cancelled by
//! dropping every waiter does not stay running: the cache holds only a weak
//! reference to that work.
//!
//! # Window
//!
//! Release instants are UTC. The shipping UI still asks for a window that
//! starts at local midnight; [`upcoming_window`] is that same rule for a
//! native caller. See `docs/architecture/integrations.md`.

#[cfg(test)]
mod behavior;
#[cfg(test)]
mod calendar_tests;
mod destination;
#[cfg(test)]
mod destination_tests;
mod error;
mod home;
mod model;
mod normalize;
mod provider;
mod redact;
mod service;
mod time;
mod transport;
mod url;
mod window;

pub use error::IntegrationError;
pub use home::home_upcoming;
pub use model::{
    IntegrationConnection, IntegrationKeyStatus, ReleaseKind, ReleaseMilestone, ReleaseTiming,
    UpcomingQuery, UpcomingRelease, UpcomingResult,
};
pub use normalize::normalize_calendar;
pub use provider::IntegrationProvider;
pub use service::{Clock, Integrations, SystemClock, UPCOMING_CACHE_TTL};
pub use transport::{
    IntegrationRequest, IntegrationResponse, ReqwestTransport, Transport, TransportError,
};
pub use url::normalize_server_url;
pub use window::{DEFAULT_WINDOW_DAYS, upcoming_window};
