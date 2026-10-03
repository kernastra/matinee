//! Typed client errors. Screens match on the variant. They do not parse text.
//!
//! [`std::fmt::Display`] is a sentence a person can read. Technical context
//! never includes tokens, passwords, or authorization headers.

use std::fmt;

use crate::redact::redact_freeform;
use crate::transport::TransportError;

#[derive(Debug)]
pub enum JellyfinError {
    InvalidUrl {
        message: String,
    },
    Unreachable {
        detail: String,
    },
    AuthRejected,
    NotFound,
    Unauthorized,
    Server {
        status: u16,
        context: String,
    },
    Malformed {
        context: String,
    },
    PlaybackUnavailable {
        message: String,
    },
    NoCompatibleSource,
    Cancelled,
    /// The OS vault rejected a session read or write. The detail is redacted.
    CredentialFailure,
    /// A stored session could not be turned back into a [`crate::Session`].
    ///
    /// The stored bytes are left in place. Logout is the call that removes them.
    CorruptSession,
}

impl JellyfinError {
    pub(crate) fn invalid_url(message: impl Into<String>) -> Self {
        Self::InvalidUrl {
            message: message.into(),
        }
    }

    pub(crate) fn unreachable(detail: impl Into<String>) -> Self {
        Self::Unreachable {
            detail: redact_freeform(&detail.into()),
        }
    }

    pub(crate) fn malformed(context: impl Into<String>) -> Self {
        Self::Malformed {
            context: context.into(),
        }
    }

    pub(crate) fn playback_unavailable(message: impl Into<String>) -> Self {
        Self::PlaybackUnavailable {
            message: message.into(),
        }
    }

    pub(crate) fn from_transport(error: TransportError) -> Self {
        match error {
            TransportError::Cancelled => Self::Cancelled,
            TransportError::Unreachable(detail) => Self::unreachable(detail),
        }
    }

    pub(crate) fn for_status(status: u16, authentication: bool) -> Self {
        match status {
            401 if authentication => Self::AuthRejected,
            401 | 403 => Self::Unauthorized,
            404 => Self::NotFound,
            status => Self::Server {
                status,
                context: format!("HTTP {status}"),
            },
        }
    }

    /// Technical context for logs. Already redacted.
    pub fn context(&self) -> Option<&str> {
        match self {
            Self::Unreachable { detail } => Some(detail),
            Self::Server { context, .. } | Self::Malformed { context } => Some(context),
            Self::InvalidUrl { message } | Self::PlaybackUnavailable { message } => Some(message),
            _ => None,
        }
    }
}

impl fmt::Display for JellyfinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl { message } | Self::PlaybackUnavailable { message } => {
                f.write_str(message)
            }
            Self::Unreachable { .. } => {
                f.write_str("Could not reach Jellyfin. Check the server address and try again.")
            }
            Self::AuthRejected => f.write_str("That username or password was not accepted."),
            Self::NotFound => f.write_str("Jellyfin was not found at that address."),
            Self::Unauthorized => f.write_str("The Jellyfin session is no longer authorized."),
            Self::Server { status, .. } => write!(f, "Jellyfin returned {status}."),
            Self::Malformed { .. } => {
                f.write_str("Jellyfin returned a response Matinee could not read.")
            }
            Self::NoCompatibleSource => {
                f.write_str("Jellyfin could not create a compatible video stream.")
            }
            Self::Cancelled => f.write_str("The request was cancelled."),
            Self::CredentialFailure => {
                f.write_str("The credential vault could not complete the request.")
            }
            Self::CorruptSession => f.write_str("The saved Jellyfin session could not be read."),
        }
    }
}

impl std::error::Error for JellyfinError {}
