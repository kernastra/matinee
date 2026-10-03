//! Studio errors. Display text is what the shipping Poster Studio already shows.

use std::fmt;

use matinee_secrets::CredentialError;

#[derive(Debug)]
pub enum StudioError {
    UnknownProvider,
    ProviderNotConfigured { provider: &'static str },
    ProviderUnavailable { message: String },
    ExecutableMissing { message: String },
    ExecutableChanged { message: String },
    GenerationBusy,
    GenerationTimeout,
    InvalidManifest { message: String },
    InvalidReference { message: String },
    UnsafeMediaPath { message: String },
    ImageTooLarge,
    PromptTooShort,
    PromptTooLong,
    ProviderError { message: String },
    Filesystem { message: String },
    Credential(CredentialError),
    PosterExists,
    InvalidItem,
    UnsupportedArtwork,
    IncompleteKey,
}

impl StudioError {
    pub(crate) fn provider(message: impl Into<String>) -> Self {
        Self::ProviderError {
            message: crate::redact::redact(&message.into()),
        }
    }

    pub(crate) fn filesystem(message: impl Into<String>) -> Self {
        Self::Filesystem {
            message: message.into(),
        }
    }

    pub(crate) fn reference(message: impl Into<String>) -> Self {
        Self::InvalidReference {
            message: message.into(),
        }
    }
}

impl fmt::Display for StudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProvider => f.write_str("Choose a supported image-generation provider."),
            Self::ProviderNotConfigured { provider } => {
                write!(f, "No {provider} API key is configured.")
            }
            Self::ProviderUnavailable { message }
            | Self::ExecutableMissing { message }
            | Self::ExecutableChanged { message }
            | Self::InvalidManifest { message }
            | Self::InvalidReference { message }
            | Self::UnsafeMediaPath { message }
            | Self::ProviderError { message }
            | Self::Filesystem { message } => f.write_str(message),
            Self::GenerationBusy => f.write_str(
                "A Codex poster is already being generated. Wait for it to finish before starting another.",
            ),
            Self::GenerationTimeout => f.write_str(
                "Codex image generation timed out after 12 minutes. The request was stopped cleanly; retry it or choose fal.ai in Settings.",
            ),
            Self::ImageTooLarge => {
                f.write_str("The generated image is larger than Matinee's 32 MB safety limit.")
            }
            Self::PromptTooShort => f.write_str("The poster prompt needs a little more visual direction."),
            Self::PromptTooLong => {
                f.write_str("The poster brief is larger than Matinee's 30,000-character safety limit.")
            }
            Self::Credential(_) => f.write_str("The credential vault could not complete the request."),
            Self::PosterExists => f.write_str("POSTER_EXISTS"),
            Self::InvalidItem => f.write_str("The Jellyfin item identifier is not valid."),
            Self::UnsupportedArtwork => f.write_str("Choose a supported Matinee artwork type."),
            Self::IncompleteKey => f.write_str("Enter a complete API key before saving."),
        }
    }
}

impl std::error::Error for StudioError {}

impl From<CredentialError> for StudioError {
    fn from(error: CredentialError) -> Self {
        Self::Credential(error)
    }
}
