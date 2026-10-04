//! One shared handle for the credential vault the application uses.
//!
//! Production wraps the OS-backed store. Tests wrap [`matinee_secrets::MemoryStore`].

use std::sync::Arc;

use matinee_secrets::{
    CredentialError, CredentialKey, CredentialNamespace, CredentialStore, Secret,
};

#[derive(Clone)]
pub struct SharedStore(Arc<dyn CredentialStore>);

impl SharedStore {
    pub fn new(store: impl CredentialStore + 'static) -> Self {
        Self(Arc::new(store))
    }
}

impl CredentialStore for SharedStore {
    fn get(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
    ) -> Result<Option<Secret>, CredentialError> {
        self.0.get(namespace, key)
    }

    fn set(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
        secret: &Secret,
    ) -> Result<(), CredentialError> {
        self.0.set(namespace, key, secret)
    }

    fn remove(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
    ) -> Result<(), CredentialError> {
        self.0.remove(namespace, key)
    }
}
