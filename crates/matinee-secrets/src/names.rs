//! Namespace and account names.
//!
//! The two shipping namespaces are preserved so existing OS credentials still
//! open. This module does not interpret the stored bytes.

use crate::error::CredentialError;

/// Shipping Radarr/Sonarr service name. Do not rename.
pub const MEDIA_INTEGRATIONS: &str = "dev.sean.matinee.media-integrations";

/// Shipping image-provider service name. Do not rename.
pub const IMAGE_GENERATION: &str = "dev.sean.matinee.image-generation";

/// Keyring service name, or the equivalent bucket in [`crate::MemoryStore`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CredentialNamespace(String);

impl CredentialNamespace {
    pub fn new(value: impl AsRef<str>) -> Result<Self, CredentialError> {
        let value = value.as_ref();
        if !valid_label(value) {
            return Err(CredentialError::InvalidName);
        }
        Ok(Self(value.to_string()))
    }

    pub fn media_integrations() -> Self {
        Self(MEDIA_INTEGRATIONS.to_string())
    }

    pub fn image_generation() -> Self {
        Self(IMAGE_GENERATION.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Account name inside a namespace. For integrations this is `radarr` or `sonarr`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CredentialKey(String);

impl CredentialKey {
    pub fn new(value: impl AsRef<str>) -> Result<Self, CredentialError> {
        let value = value.as_ref();
        if !valid_label(value) {
            return Err(CredentialError::InvalidName);
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipping_namespaces_round_trip() {
        assert_eq!(
            CredentialNamespace::media_integrations().as_str(),
            "dev.sean.matinee.media-integrations"
        );
        assert_eq!(
            CredentialNamespace::image_generation().as_str(),
            "dev.sean.matinee.image-generation"
        );
        assert!(CredentialKey::new("radarr").is_ok());
        assert!(CredentialKey::new("fal").is_ok());
        assert!(CredentialKey::new("").is_err());
        assert!(CredentialKey::new("has space").is_err());
        assert!(CredentialKey::new("line\nbreak").is_err());
        assert!(CredentialNamespace::new("../escape").is_err());
    }
}
