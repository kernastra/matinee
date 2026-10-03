//! In-memory Jellyfin session.
//!
//! The access token lives here for the process lifetime of this value. It is
//! not written to a file. Persistent native login is still open; do not copy
//! the shipping app's browser session or the Tauri credential vault into this
//! crate.

use std::fmt;

use zeroize::Zeroize;

use crate::artwork::ArtworkUrls;
use crate::auth::{authorization_header, validate_token};
use crate::error::JellyfinError;
use crate::url::normalize_server_url;
use matinee_core::User;

/// Public server identity from `/System/Info/Public`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    pub name: Option<String>,
    pub version: Option<String>,
    pub operating_system: Option<String>,
    pub product: Option<String>,
    pub id: Option<String>,
}

pub struct Session {
    server_url: String,
    access_token: String,
    user: User,
}

impl Session {
    /// Build an in-memory session from a server address, token, and user.
    ///
    /// This does not read or write credentials.
    pub fn new(
        server_url: impl AsRef<str>,
        access_token: impl Into<String>,
        user: User,
    ) -> Result<Self, JellyfinError> {
        let server_url = normalize_server_url(server_url.as_ref())?;
        let access_token = access_token.into();
        validate_token(&access_token)?;
        Ok(Self {
            server_url,
            access_token,
            user,
        })
    }

    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    pub fn user(&self) -> &User {
        &self.user
    }

    /// The access token. Prefer [`Self::authorization_header`] for requests.
    ///
    /// Artwork URLs still carry `api_key` so they match the shipping image
    /// loader. That puts the token in the URL, where it can land in a log or
    /// an image cache. A future native loader should send
    /// [`Self::authorization_header`] and leave the query empty.
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    pub fn authorization_header(&self) -> Result<String, JellyfinError> {
        authorization_header(Some(&self.access_token))
    }

    pub fn artwork(&self) -> ArtworkUrls<'_> {
        ArtworkUrls::new(self)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("server_url", &self.server_url)
            .field("access_token", &"<redacted>")
            .field("user", &self.user)
            .finish()
    }
}

impl Clone for Session {
    fn clone(&self) -> Self {
        Self {
            server_url: self.server_url.clone(),
            access_token: self.access_token.clone(),
            user: self.user.clone(),
        }
    }
}
