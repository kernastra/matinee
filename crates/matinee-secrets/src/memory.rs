//! Deterministic vault for tests. It never touches the OS keyring.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use zeroize::Zeroize;

use crate::error::CredentialError;
use crate::names::{CredentialKey, CredentialNamespace};
use crate::secret::Secret;
use crate::store::CredentialStore;

#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<HashMap<(String, String), String>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), String>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl CredentialStore for MemoryStore {
    fn get(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
    ) -> Result<Option<Secret>, CredentialError> {
        let entries = self.lock();
        Ok(entries
            .get(&(namespace.as_str().to_string(), key.as_str().to_string()))
            .map(|value| Secret::new(value.clone())))
    }

    fn set(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
        secret: &Secret,
    ) -> Result<(), CredentialError> {
        let mut entries = self.lock();
        let map_key = (namespace.as_str().to_string(), key.as_str().to_string());
        if let Some(previous) = entries.insert(map_key, secret.expose().to_string()) {
            let mut previous = previous;
            previous.zeroize();
        }
        Ok(())
    }

    fn remove(
        &self,
        namespace: &CredentialNamespace,
        key: &CredentialKey,
    ) -> Result<(), CredentialError> {
        let mut entries = self.lock();
        if let Some(mut previous) =
            entries.remove(&(namespace.as_str().to_string(), key.as_str().to_string()))
        {
            previous.zeroize();
        }
        Ok(())
    }
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.lock().len();
        f.debug_struct("MemoryStore")
            .field("entries", &count)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> (CredentialNamespace, CredentialKey) {
        (
            CredentialNamespace::media_integrations(),
            CredentialKey::new("radarr").unwrap(),
        )
    }

    #[test]
    fn missing_save_replace_remove_and_exists() {
        let store = MemoryStore::new();
        let (namespace, key) = names();
        assert!(!store.exists(&namespace, &key).unwrap());
        assert!(store.get(&namespace, &key).unwrap().is_none());

        store
            .set(&namespace, &key, &Secret::new("first-secret"))
            .unwrap();
        assert_eq!(
            store.get(&namespace, &key).unwrap().unwrap().expose(),
            "first-secret"
        );
        assert!(store.exists(&namespace, &key).unwrap());

        store
            .set(
                &namespace,
                &key,
                &Secret::new(
                    r#"{"serverUrl":"http://radarr.local:7878","apiKey":"second-secret"}"#,
                ),
            )
            .unwrap();
        let stored = store.get(&namespace, &key).unwrap().unwrap();
        assert!(stored.expose().contains("second-secret"));
        assert!(!stored.expose().contains("first-secret"));

        store.remove(&namespace, &key).unwrap();
        store.remove(&namespace, &key).unwrap();
        assert!(store.get(&namespace, &key).unwrap().is_none());
    }

    #[test]
    fn debug_does_not_include_the_secret() {
        let store = MemoryStore::new();
        let (namespace, key) = names();
        store
            .set(&namespace, &key, &Secret::new("visible-if-leaked"))
            .unwrap();
        assert!(!format!("{store:?}").contains("visible-if-leaked"));
    }

    #[test]
    fn namespaces_do_not_collide() {
        let store = MemoryStore::new();
        let media = CredentialNamespace::media_integrations();
        let images = CredentialNamespace::image_generation();
        let key = CredentialKey::new("fal").unwrap();
        store.set(&images, &key, &Secret::new("image-key")).unwrap();
        assert!(store.get(&media, &key).unwrap().is_none());
        assert_eq!(
            store.get(&images, &key).unwrap().unwrap().expose(),
            "image-key"
        );
    }
}
