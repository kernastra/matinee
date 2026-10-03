//! OS credential vault.
//!
//! Linux uses Secret Service (`sync-secret-service`, Rust crypto). macOS uses
//! Keychain. Windows uses Credential Manager. `keyring::Entry` does not leave
//! this module. Calls are not made from unit tests.

use crate::error::CredentialError;
use crate::names::{CredentialKey, CredentialNamespace};
use crate::secret::Secret;
use crate::store::CredentialStore;

/// Production vault. Constructing it does not touch the OS; each call does.
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl CredentialStore for KeyringStore {
    fn get(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
    ) -> Result<Option<Secret>, CredentialError> {
        match open(namespace, key)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(CredentialError::backend(error.to_string())),
        }
    }

    fn set(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
        secret: &Secret,
    ) -> Result<(), CredentialError> {
        open(namespace, key)?
            .set_password(secret.expose())
            .map_err(|error| CredentialError::backend(error.to_string()))
    }

    fn remove(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
    ) -> Result<(), CredentialError> {
        match open(namespace, key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(CredentialError::backend(error.to_string())),
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
impl CredentialStore for KeyringStore {
    fn get(
        &self,
        _namespace: &CredentialNamespace,
        _key: &CredentialKey,
    ) -> Result<Option<Secret>, CredentialError> {
        Err(unsupported())
    }

    fn set(
        &self,
        _namespace: &CredentialNamespace,
        _key: &CredentialKey,
        _secret: &Secret,
    ) -> Result<(), CredentialError> {
        Err(unsupported())
    }

    fn remove(
        &self,
        _namespace: &CredentialNamespace,
        _key: &CredentialKey,
    ) -> Result<(), CredentialError> {
        Err(unsupported())
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn open(
    namespace: &CredentialNamespace,
    key: &CredentialKey,
) -> Result<keyring::Entry, CredentialError> {
    keyring::Entry::new(namespace.as_str(), key.as_str())
        .map_err(|error| CredentialError::backend(error.to_string()))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn unsupported() -> CredentialError {
    CredentialError::backend("This operating system has no Matinee credential backend.")
}
