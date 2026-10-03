//! Jellyfin client for Matinee.
//!
//! This crate speaks HTTP and Jellyfin JSON, then converts into
//! [`matinee_core`] types. DTOs stay private. Screens and the player depend
//! on the domain, not on `Id`, `RunTimeTicks`, or `UserData`.
//!
//! The session is in memory only. Native persistent login is intentionally
//! not implemented here, and this crate does not read the shipping app's
//! credential vault.
//!
//! # Device id
//!
//! Authorization uses a fixed `DeviceId` of `matinee-desktop`, the same value
//! as the shipping app. It is not an installation identity. Nothing here
//! generates or stores a per-machine id.
//!
//! # HTTP runtime
//!
//! [`ReqwestTransport`] must be polled on a Tokio runtime. The owner is the
//! application service runtime, not this crate and not a UI task. That
//! runtime is not built yet. The first screen that calls this client creates
//! it and drives these futures there. Calling [`ReqwestTransport`] from a UI
//! task panics. Tests substitute [`Transport`] and use
//! `futures::executor::block_on`. The owner diagram is in the architecture
//! notes for this client.

#[cfg(test)]
mod api_tests;
mod artwork;
mod auth;
mod catalog;
mod client;
mod convert;
mod dto;
mod error;
mod playback;
mod profile;
mod query;
mod redact;
mod series;
mod session;
mod ticks;
mod transport;
mod url;

pub use artwork::{ArtworkRequest, ArtworkUrls};
pub use auth::{Password, authenticate};
pub use client::JellyfinClient;
pub use error::JellyfinError;
pub use session::{ServerInfo, Session};
pub use transport::{
    CancelFlag, HttpRequest, HttpResponse, Method, ReqwestTransport, Transport, TransportError,
};
pub use url::normalize_server_url;

/// `Client` field on the Jellyfin authorization header.
pub const CLIENT_NAME: &str = "Matinee";

/// `Device` field on the Jellyfin authorization header.
pub const DEVICE_NAME: &str = "Desktop";

/// Fixed `DeviceId` shared with the shipping desktop app.
///
/// Jellyfin binds an access token to this id. It does not identify an
/// installation, and this crate does not persist one.
pub const DEVICE_ID: &str = "matinee-desktop";

/// `Version` field. Matches the shipping app until the native app versions itself.
pub const CLIENT_VERSION: &str = "0.5.6";
