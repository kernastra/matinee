//! Session startup, persistence, and sign-out.
//!
//! Network authentication stays in `matinee-jellyfin`. This module decides
//! what a vault result means for the application and refuses to treat a
//! failed save or a failed removal as success.

use matinee_jellyfin::{JellyfinError, Session, load_session, remove_session, save_session};
use matinee_secrets::CredentialStore;

/// Jellyfin said the saved session no longer works (401, 403, or a
/// rejected token). Every authenticated screen treats this the same way: it
/// reports it, and the shell ends the session. Unreachable servers, timeouts,
/// and unreadable answers are not this.
pub(crate) fn session_ended(error: &JellyfinError) -> bool {
    matches!(
        error,
        JellyfinError::Unauthorized | JellyfinError::AuthRejected
    )
}

/// A request found that the session has ended. Carries nothing on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SessionEnded;

#[derive(Debug)]
pub enum Startup {
    Unauthenticated,
    Authenticated(Session),
    /// A stored payload could not become a session. The vault entry is left in place.
    Corrupt,
    VaultUnavailable,
}

pub fn restore_session(store: &impl CredentialStore) -> Startup {
    match load_session(store) {
        Ok(None) => Startup::Unauthenticated,
        Ok(Some(session)) => Startup::Authenticated(session),
        Err(JellyfinError::CorruptSession) => Startup::Corrupt,
        Err(_) => Startup::VaultUnavailable,
    }
}

/// Persist a successful authentication. A vault error drops the session and
/// returns the user-facing failure. The caller must not enter the shell.
pub fn accept_authentication(
    store: &impl CredentialStore,
    result: Result<Session, JellyfinError>,
) -> Result<Session, String> {
    match result {
        Ok(session) => match save_session(store, &session) {
            Ok(()) => Ok(session),
            Err(error) => Err(error.to_string()),
        },
        Err(error) => Err(error.to_string()),
    }
}

/// Remove the persisted session. Failure leaves the entry in the vault.
pub fn forget_session(store: &impl CredentialStore) -> Result<(), String> {
    remove_session(store).map_err(|error| error.to_string())
}

pub fn startup_notice(startup: &Startup) -> Option<&'static str> {
    match startup {
        Startup::Corrupt => Some("The saved Jellyfin session could not be read."),
        Startup::VaultUnavailable => Some("The credential vault could not complete the request."),
        Startup::Unauthenticated | Startup::Authenticated(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matinee_core::{ImageTag, User, UserId};
    use matinee_jellyfin::{Password, SESSION_ACCOUNT, SESSION_NAMESPACE};
    use matinee_secrets::{CredentialKey, CredentialNamespace, MemoryStore, Secret};

    fn session() -> Session {
        Session::new(
            "http://jellyfin.local:8096",
            "token-value-not-for-logs",
            User::new(UserId::parse("user-1").unwrap(), "alex", None::<ImageTag>),
        )
        .unwrap()
    }

    struct Fallible {
        inner: MemoryStore,
        fail_get: bool,
        fail_set: bool,
        fail_remove: bool,
    }

    impl CredentialStore for Fallible {
        fn get(
            &self,
            namespace: &CredentialNamespace,
            key: &CredentialKey,
        ) -> Result<Option<Secret>, matinee_secrets::CredentialError> {
            if self.fail_get {
                return Err(matinee_secrets::CredentialError::Backend {
                    detail: "unavailable".into(),
                });
            }
            self.inner.get(namespace, key)
        }

        fn set(
            &self,
            namespace: &CredentialNamespace,
            key: &CredentialKey,
            secret: &Secret,
        ) -> Result<(), matinee_secrets::CredentialError> {
            if self.fail_set {
                return Err(matinee_secrets::CredentialError::Backend {
                    detail: "unavailable".into(),
                });
            }
            self.inner.set(namespace, key, secret)
        }

        fn remove(
            &self,
            namespace: &CredentialNamespace,
            key: &CredentialKey,
        ) -> Result<(), matinee_secrets::CredentialError> {
            if self.fail_remove {
                return Err(matinee_secrets::CredentialError::Backend {
                    detail: "unavailable".into(),
                });
            }
            self.inner.remove(namespace, key)
        }
    }

    fn namespace() -> CredentialNamespace {
        CredentialNamespace::new(SESSION_NAMESPACE).unwrap()
    }

    fn account() -> CredentialKey {
        CredentialKey::new(SESSION_ACCOUNT).unwrap()
    }

    #[test]
    fn missing_session_is_unauthenticated() {
        let store = MemoryStore::new();
        assert!(matches!(restore_session(&store), Startup::Unauthenticated));
    }

    #[test]
    fn a_saved_session_restores() {
        let store = MemoryStore::new();
        accept_authentication(&store, Ok(session())).unwrap();
        match restore_session(&store) {
            Startup::Authenticated(loaded) => {
                assert_eq!(loaded.user().name(), "alex");
                assert_eq!(loaded.server_url(), "http://jellyfin.local:8096");
                assert!(!format!("{loaded:?}").contains("token-value-not-for-logs"));
            }
            other => panic!("expected a session, got {other:?}"),
        }
    }

    #[test]
    fn a_corrupt_session_stays_stored() {
        let store = MemoryStore::new();
        store
            .set(
                &namespace(),
                &account(),
                &Secret::new("{\"accessToken\":\"still-secret\""),
            )
            .unwrap();
        assert!(matches!(restore_session(&store), Startup::Corrupt));
        let still = store.get(&namespace(), &account()).unwrap().unwrap();
        assert!(still.expose().contains("still-secret"));
        assert_eq!(
            startup_notice(&Startup::Corrupt),
            Some("The saved Jellyfin session could not be read.")
        );
    }

    #[test]
    fn a_vault_read_failure_is_visible() {
        let store = Fallible {
            inner: MemoryStore::new(),
            fail_get: true,
            fail_set: false,
            fail_remove: false,
        };
        assert!(matches!(restore_session(&store), Startup::VaultUnavailable));
        assert_eq!(
            startup_notice(&Startup::VaultUnavailable),
            Some("The credential vault could not complete the request.")
        );
    }

    #[test]
    fn save_failure_does_not_report_success() {
        let store = Fallible {
            inner: MemoryStore::new(),
            fail_get: false,
            fail_set: true,
            fail_remove: false,
        };
        let error = accept_authentication(&store, Ok(session())).unwrap_err();
        assert_eq!(
            error,
            "The credential vault could not complete the request."
        );
        assert!(!error.contains("token-value-not-for-logs"));
        assert!(load_session(&store).unwrap().is_none());
    }

    #[test]
    fn auth_errors_are_readable_and_omit_secrets() {
        let store = MemoryStore::new();
        let rejected = accept_authentication(&store, Err(JellyfinError::AuthRejected)).unwrap_err();
        assert_eq!(rejected, "That username or password was not accepted.");
        assert!(!rejected.contains("desk-lamp-phrase"));
        let unreachable = accept_authentication(
            &store,
            Err(JellyfinError::Unreachable {
                detail: "dial secret".into(),
            }),
        )
        .unwrap_err();
        assert_eq!(
            unreachable,
            "Could not reach Jellyfin. Check the server address and try again."
        );
        assert!(!unreachable.contains("secret"));
        let server = accept_authentication(
            &store,
            Err(JellyfinError::Server {
                status: 500,
                context: "HTTP 500".into(),
            }),
        )
        .unwrap_err();
        assert_eq!(server, "Jellyfin returned 500.");
        let malformed = accept_authentication(
            &store,
            Err(JellyfinError::Malformed {
                context: "authentication".into(),
            }),
        )
        .unwrap_err();
        assert_eq!(
            malformed,
            "Jellyfin returned a response Matinee could not read."
        );
        let password = Password::new("desk-lamp-phrase");
        assert!(!format!("{password:?}").contains("desk-lamp-phrase"));
        assert!(load_session(&store).unwrap().is_none());
    }

    #[test]
    fn remove_failure_does_not_report_success() {
        let store = Fallible {
            inner: MemoryStore::new(),
            fail_get: false,
            fail_set: false,
            fail_remove: true,
        };
        accept_authentication(&store, Ok(session())).unwrap();
        let error = forget_session(&store).unwrap_err();
        assert_eq!(
            error,
            "The credential vault could not complete the request."
        );
        assert!(load_session(&store).unwrap().is_some());
    }

    #[test]
    fn sign_out_removes_the_session() {
        let store = MemoryStore::new();
        accept_authentication(&store, Ok(session())).unwrap();
        forget_session(&store).unwrap();
        assert!(load_session(&store).unwrap().is_none());
    }
}
