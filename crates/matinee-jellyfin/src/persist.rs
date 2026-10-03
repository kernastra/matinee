//! Persistent Jellyfin session stored through [`matinee_secrets`].
//!
//! This module does not call `keyring` and does not change
//! [`crate::authenticate`]. The token lives in the vault payload, not in a
//! settings file. A corrupt payload is left in place; [`remove_session`] is
//! the logout path.
//!
//! The namespace is `dev.sean.matinee.jellyfin-session` and the account is
//! `default`. Shipping v0.5.6 does not write this entry. The browser session
//! in `sessionStorage` is a separate store and is not migrated here.

use std::fmt;

use matinee_core::{ImageTag, User, UserId};
use matinee_secrets::{CredentialKey, CredentialNamespace, CredentialStore, Secret};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::error::JellyfinError;
use crate::session::Session;

/// Keyring service name for the native Jellyfin session.
pub const SESSION_NAMESPACE: &str = "dev.sean.matinee.jellyfin-session";

/// Single account inside [`SESSION_NAMESPACE`].
pub const SESSION_ACCOUNT: &str = "default";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSession {
    server_url: String,
    user_id: String,
    user_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    avatar_tag: Option<String>,
    access_token: String,
}

impl Drop for StoredSession {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}

impl fmt::Debug for StoredSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredSession")
            .field("server_url", &self.server_url)
            .field("user_id", &self.user_id)
            .field("access_token", &"[redacted]")
            .finish()
    }
}

/// Write `session` into `store`. Replaces a previous native session.
pub fn save_session(store: &impl CredentialStore, session: &Session) -> Result<(), JellyfinError> {
    let stored = StoredSession {
        server_url: session.server_url().to_string(),
        user_id: session.user().id().as_str().to_string(),
        user_name: session.user().name().to_string(),
        avatar_tag: session
            .user()
            .avatar
            .as_ref()
            .map(|tag| tag.as_str().to_string()),
        access_token: session.access_token().to_string(),
    };
    let payload = serde_json::to_string(&stored).map_err(|_| JellyfinError::CorruptSession)?;
    store
        .set(&namespace()?, &account()?, &Secret::new(payload))
        .map_err(|_| JellyfinError::CredentialFailure)
}

/// Read a session previously written by [`save_session`].
///
/// `Ok(None)` means the entry is absent. A payload that cannot become a
/// [`Session`] is [`JellyfinError::CorruptSession`] and is not deleted.
pub fn load_session(store: &impl CredentialStore) -> Result<Option<Session>, JellyfinError> {
    let Some(secret) = store
        .get(&namespace()?, &account()?)
        .map_err(|_| JellyfinError::CredentialFailure)?
    else {
        return Ok(None);
    };
    let stored: StoredSession =
        serde_json::from_str(secret.expose()).map_err(|_| JellyfinError::CorruptSession)?;
    session_from_stored(stored).map(Some)
}

/// Remove the native session. Missing entries succeed.
pub fn remove_session(store: &impl CredentialStore) -> Result<(), JellyfinError> {
    store
        .remove(&namespace()?, &account()?)
        .map_err(|_| JellyfinError::CredentialFailure)
}

fn session_from_stored(mut stored: StoredSession) -> Result<Session, JellyfinError> {
    let user_id = UserId::parse(&stored.user_id).map_err(|_| JellyfinError::CorruptSession)?;
    let avatar = match stored.avatar_tag.as_deref() {
        None => None,
        Some(tag) if tag.trim().is_empty() => None,
        Some(tag) => Some(ImageTag::parse(tag).ok_or(JellyfinError::CorruptSession)?),
    };
    let user_name = std::mem::take(&mut stored.user_name);
    let server_url = std::mem::take(&mut stored.server_url);
    let access_token = std::mem::take(&mut stored.access_token);
    let user = User::new(user_id, user_name, avatar);
    Session::new(server_url, access_token, user).map_err(|_| JellyfinError::CorruptSession)
}

fn namespace() -> Result<CredentialNamespace, JellyfinError> {
    CredentialNamespace::new(SESSION_NAMESPACE).map_err(|_| JellyfinError::CredentialFailure)
}

fn account() -> Result<CredentialKey, JellyfinError> {
    CredentialKey::new(SESSION_ACCOUNT).map_err(|_| JellyfinError::CredentialFailure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use matinee_secrets::MemoryStore;

    fn session() -> Session {
        let avatar = ImageTag::parse("avatar-tag");
        Session::new(
            "http://jellyfin.local:8096",
            "token-value-not-for-logs",
            User::new(UserId::parse("user-1").unwrap(), "Ada", avatar),
        )
        .unwrap()
    }

    #[test]
    fn save_load_and_remove_round_trip_on_the_memory_vault() {
        let store = MemoryStore::new();
        assert!(load_session(&store).unwrap().is_none());
        save_session(&store, &session()).unwrap();
        let loaded = load_session(&store).unwrap().unwrap();
        assert_eq!(loaded.server_url(), "http://jellyfin.local:8096");
        assert_eq!(loaded.user().name(), "Ada");
        assert_eq!(loaded.user().id().as_str(), "user-1");
        assert_eq!(loaded.access_token(), "token-value-not-for-logs");
        let rendered = format!("{loaded:?}");
        assert!(!rendered.contains("token-value-not-for-logs"));
        remove_session(&store).unwrap();
        assert!(load_session(&store).unwrap().is_none());
        remove_session(&store).unwrap();
    }

    #[test]
    fn corrupt_payloads_stay_in_the_vault() {
        let store = MemoryStore::new();
        store
            .set(
                &CredentialNamespace::new(SESSION_NAMESPACE).unwrap(),
                &CredentialKey::new(SESSION_ACCOUNT).unwrap(),
                &Secret::new("{\"accessToken\":\"still-secret\",\"serverUrl\":"),
            )
            .unwrap();
        assert!(matches!(
            load_session(&store),
            Err(JellyfinError::CorruptSession)
        ));
        let still = store
            .get(
                &CredentialNamespace::new(SESSION_NAMESPACE).unwrap(),
                &CredentialKey::new(SESSION_ACCOUNT).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert!(still.expose().contains("still-secret"));
        assert!(!format!("{still:?}").contains("still-secret"));
    }
}
