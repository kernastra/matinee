//! The vault operations callers use. The OS backend stays inside this crate.

use crate::error::CredentialError;
use crate::names::{CredentialKey, CredentialNamespace};
use crate::secret::Secret;

/// Get, set, remove, and check a secret.
///
/// `get` returns `Ok(None)` when the entry is absent. `remove` succeeds when
/// the entry is already absent. Implementations must not log `secret`.
pub trait CredentialStore: Send + Sync {
    fn get(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
    ) -> Result<Option<Secret>, CredentialError>;

    fn set(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
        secret: &Secret,
    ) -> Result<(), CredentialError>;

    fn remove(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
    ) -> Result<(), CredentialError>;

    fn exists(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
    ) -> Result<bool, CredentialError> {
        Ok(self.get(namespace, key)?.is_some())
    }
}
