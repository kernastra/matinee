//! Credential vault for Matinee services.
//!
//! Production uses the OS keyring (Linux Secret Service, macOS Keychain,
//! Windows Credential Manager) through [`KeyringStore`]. Tests use
//! [`MemoryStore`]. This crate does not know Jellyfin, Radarr, Sonarr, image
//! providers, GPUI, Atelier, or Tauri. Callers choose the namespace and the
//! payload.
//!
//! # Secret copies
//!
//! [`Secret`] zeroizes the `String` it owns when dropped. That is not a claim
//! that the secret is gone from the process. The keyring crate returns its own
//! `String`. `serde_json` builds another string when a caller stores a JSON
//! payload. HTTP clients copy header values for the duration of a request.
//! [`MemoryStore`] keeps a copy until [`CredentialStore::remove`]. The
//! allocator can reuse freed memory without wiping it.

mod error;
mod keyring_store;
mod memory;
mod names;
mod secret;
mod store;

pub use error::CredentialError;
pub use keyring_store::KeyringStore;
pub use memory::MemoryStore;
pub use names::{CredentialKey, CredentialNamespace, IMAGE_GENERATION, MEDIA_INTEGRATIONS};
pub use secret::Secret;
pub use store::CredentialStore;

/// Namespace and account used by the shipping app for Radarr and Sonarr.
///
/// The payload is the caller's. This crate does not interpret it.
pub fn media_integration(
    provider: &str,
) -> Result<(CredentialNamespace, CredentialKey), CredentialError> {
    Ok((
        CredentialNamespace::media_integrations(),
        CredentialKey::new(provider)?,
    ))
}

/// Namespace and account used by the shipping app for fal and Higgsfield keys.
pub fn image_provider(
    provider: &str,
) -> Result<(CredentialNamespace, CredentialKey), CredentialError> {
    Ok((
        CredentialNamespace::image_generation(),
        CredentialKey::new(provider)?,
    ))
}
