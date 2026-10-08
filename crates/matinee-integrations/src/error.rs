//! Typed integration errors. Display is the sentence the shipping UI already shows.

use std::fmt;

use matinee_secrets::CredentialError;

use crate::provider::IntegrationProvider;
use crate::redact::redact;

#[derive(Debug)]
pub enum IntegrationError {
    UnknownProvider,
    InvalidServer {
        message: String,
    },
    NotConfigured {
        provider: IntegrationProvider,
    },
    AuthenticationRejected {
        provider: IntegrationProvider,
        detail: String,
    },
    WrongProvider {
        provider: IntegrationProvider,
    },
    Unreachable {
        provider: IntegrationProvider,
        detail: String,
    },
    Server {
        provider: IntegrationProvider,
        status: u16,
        detail: String,
    },
    MalformedResponse {
        provider: IntegrationProvider,
    },
    Credential(CredentialError),
    CorruptCredential {
        provider: IntegrationProvider,
    },
    KeyBoundToOtherServer {
        provider: IntegrationProvider,
    },
    IncompleteKey,
    InvalidWindow,
    /// An artwork address that is not http or https, or an oversized body.
    InvalidImage {
        provider: IntegrationProvider,
    },
    /// The image host answered with a failure status.
    ImageUnavailable {
        provider: IntegrationProvider,
        status: u16,
    },
    Client {
        detail: String,
    },
}

impl IntegrationError {
    pub(crate) fn invalid_server(message: impl Into<String>) -> Self {
        Self::InvalidServer {
            message: message.into(),
        }
    }

    pub(crate) fn unreachable(provider: IntegrationProvider, detail: impl Into<String>) -> Self {
        Self::Unreachable {
            provider,
            detail: redact(&detail.into()),
        }
    }

    pub(crate) fn server(provider: IntegrationProvider, status: u16) -> Self {
        if status == 401 {
            Self::AuthenticationRejected {
                provider,
                detail: String::new(),
            }
        } else {
            Self::Server {
                provider,
                status,
                detail: String::new(),
            }
        }
    }
}

impl fmt::Display for IntegrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProvider => f.write_str("Choose Radarr or Sonarr."),
            Self::InvalidServer { message } => f.write_str(message),
            Self::NotConfigured { provider } => {
                write!(f, "No {provider} integration is configured.")
            }
            Self::AuthenticationRejected { provider, .. } | Self::Server { provider, .. } => {
                let status = self.status_code().unwrap_or(401);
                write!(f, "{provider} returned HTTP {status}.")
            }
            Self::WrongProvider { provider } => {
                write!(
                    f,
                    "That address responded, but it does not appear to be {provider}."
                )
            }
            Self::Unreachable { provider, detail } => {
                write!(f, "Matinee could not reach {provider}: {detail}")
            }
            Self::MalformedResponse { provider } => {
                write!(f, "{provider} returned an unreadable response.")
            }
            Self::Credential(_) => {
                f.write_str("The credential vault could not complete the request.")
            }
            Self::CorruptCredential { provider } => {
                write!(
                    f,
                    "Reconnect {provider} once to securely bind its saved API key to the server address."
                )
            }
            Self::KeyBoundToOtherServer { provider } => {
                write!(
                    f,
                    "The saved {provider} key is bound to a different server. Enter the API key again to change addresses."
                )
            }
            Self::IncompleteKey => {
                f.write_str("Enter a complete API key before testing the connection.")
            }
            Self::InvalidWindow => f.write_str("The release window is not a valid time range."),
            Self::InvalidImage { provider } => {
                write!(
                    f,
                    "{provider} artwork uses an address Matinee does not load."
                )
            }
            Self::ImageUnavailable { provider, status } => {
                write!(f, "{provider} artwork returned HTTP {status}.")
            }
            Self::Client { .. } => f.write_str("Matinee could not prepare the integration client."),
        }
    }
}

impl IntegrationError {
    fn status_code(&self) -> Option<u16> {
        match self {
            Self::AuthenticationRejected { .. } => Some(401),
            Self::Server { status, .. } => Some(*status),
            _ => None,
        }
    }
}

impl std::error::Error for IntegrationError {}

impl From<CredentialError> for IntegrationError {
    fn from(error: CredentialError) -> Self {
        Self::Credential(error)
    }
}
