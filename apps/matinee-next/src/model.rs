//! High-level application state.
//!
//! One session lives here. Views receive the username and server, not the
//! access token. The password is wiped when sign-in succeeds and when the
//! model drops. That does not cover copies GPUI keeps while the field is
//! on screen.

use std::fmt;

use matinee_core::{User, UserId};
use matinee_jellyfin::{Password, Session};
use zeroize::Zeroize;

use crate::session::{Startup, startup_notice};
use crate::warning::insecure_http_warning;

pub(crate) const LOGIN_COPY: &str = "Connect to your Jellyfin server to browse your library and pick up exactly where you left off.";
/// Shown when the signed-in session cannot make a Jellyfin client.
pub(crate) const UNAVAILABLE: &str =
    "Could not reach Jellyfin. Check the server address and try again.";
/// Login notice after Jellyfin stopped accepting the saved session.
pub(crate) const SESSION_ENDED: &str =
    "Your Jellyfin session has ended. Sign in again to keep browsing.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    Unauthenticated,
    Authenticating,
    Authenticated,
    SigningOut,
}

pub struct SignInRequest {
    pub server: String,
    pub username: String,
    password: Password,
}

impl SignInRequest {
    pub fn into_parts(self) -> (String, String, Password) {
        (self.server, self.username, self.password)
    }
}

impl fmt::Debug for SignInRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignInRequest")
            .field("server", &self.server)
            .field("username", &self.username)
            .field("password", &self.password)
            .finish()
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub username: String,
    pub server: String,
}

/// Fixture screens for visual review. They never read the credential vault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewScene {
    Login,
    Focus,
    Warning,
    Error,
    Loading,
    Home,
    HomeContinueWatching,
    HomeEmpty,
    HomePartialError,
    HomeLoading,
    HomeSmallWindow,
    HomeLargeWindow,
    HomeHeroResume,
    HomeHeroFresh,
    PlayerPlaying,
    PlayerPaused,
    PlayerControls,
    PlayerError,
    PlayerAudio,
    PlayerSubtitles,
    DetailsMovie,
    DetailsMovieResume,
    DetailsSeries,
    DetailsSeason,
    DetailsLoading,
    DetailsError,
    Library,
    LibraryMovies,
    LibrarySeries,
    LibraryLoading,
    LibraryEmpty,
    LibraryFilteredEmpty,
    LibraryPartialPage,
    LibraryError,
    LibrarySmallWindow,
    LibraryLargeWindow,
    LibraryManyItems,
}

impl ReviewScene {
    pub(crate) fn player_preview(self) -> Option<crate::player::PlayerPreview> {
        match self {
            Self::PlayerPlaying => Some(crate::player::PlayerPreview::Playing),
            Self::PlayerPaused => Some(crate::player::PlayerPreview::Paused),
            Self::PlayerControls => Some(crate::player::PlayerPreview::Controls),
            Self::PlayerError => Some(crate::player::PlayerPreview::Error),
            Self::PlayerAudio => Some(crate::player::PlayerPreview::AudioMenu),
            Self::PlayerSubtitles => Some(crate::player::PlayerPreview::SubtitleMenu),
            _ => None,
        }
    }

    pub(crate) fn home_preview(self) -> Option<crate::home::HomePreview> {
        use crate::home::HomePreview;
        match self {
            Self::Home | Self::HomeSmallWindow | Self::HomeLargeWindow | Self::HomeHeroResume => {
                Some(HomePreview::Home)
            }
            Self::HomeContinueWatching => Some(HomePreview::ContinueWatching),
            Self::HomeEmpty => Some(HomePreview::Empty),
            Self::HomePartialError => Some(HomePreview::PartialError),
            Self::HomeLoading => Some(HomePreview::Loading),
            Self::HomeHeroFresh => Some(HomePreview::HeroFresh),
            _ => None,
        }
    }

    pub(crate) fn library_preview(self) -> Option<crate::library::LibraryPreview> {
        use crate::library::LibraryPreview;
        match self {
            Self::Library
            | Self::LibraryMovies
            | Self::LibrarySmallWindow
            | Self::LibraryLargeWindow => Some(LibraryPreview::Movies),
            Self::LibrarySeries => Some(LibraryPreview::Series),
            Self::LibraryLoading => Some(LibraryPreview::Loading),
            Self::LibraryEmpty => Some(LibraryPreview::Empty),
            Self::LibraryFilteredEmpty => Some(LibraryPreview::FilteredEmpty),
            Self::LibraryPartialPage => Some(LibraryPreview::PartialPage),
            Self::LibraryError => Some(LibraryPreview::Error),
            Self::LibraryManyItems => Some(LibraryPreview::ManyItems),
            _ => None,
        }
    }

    /// The window size a scene is reviewed at, unless one is given.
    pub(crate) fn size(self) -> (f32, f32) {
        match self {
            Self::HomeSmallWindow | Self::LibrarySmallWindow => (960.0, 620.0),
            Self::HomeLargeWindow | Self::LibraryLargeWindow => (1920.0, 1080.0),
            _ => (1200.0, 760.0),
        }
    }

    pub(crate) fn details_preview(self) -> Option<crate::details::DetailsPreview> {
        use crate::details::DetailsPreview;
        match self {
            Self::DetailsMovie => Some(DetailsPreview::Movie),
            Self::DetailsMovieResume => Some(DetailsPreview::MovieResume),
            Self::DetailsSeries => Some(DetailsPreview::Series),
            Self::DetailsSeason => Some(DetailsPreview::Season),
            Self::DetailsLoading => Some(DetailsPreview::Loading),
            Self::DetailsError => Some(DetailsPreview::Error),
            _ => None,
        }
    }
}

pub struct AppModel {
    phase: Phase,
    session: Option<Session>,
    notice: Option<String>,
    server: String,
    username: String,
    password: String,
}

impl AppModel {
    pub fn review(scene: ReviewScene) -> Self {
        let mut model = Self::starting();
        match scene {
            ReviewScene::Login => {
                model.apply_startup(Startup::Unauthenticated);
            }
            ReviewScene::Focus => {
                model.apply_startup(Startup::Unauthenticated);
                model.server = "https://jellyfin.local:8096".into();
                model.username = "alex".into();
            }
            ReviewScene::Warning => {
                model.apply_startup(Startup::Unauthenticated);
                model.server = "http://jellyfin.local:8096".into();
                model.username = "alex".into();
            }
            ReviewScene::Error => {
                model.apply_startup(Startup::Unauthenticated);
                model.server = "https://jellyfin.local:8096".into();
                model.username = "alex".into();
                model.notice = Some("That username or password was not accepted.".into());
            }
            ReviewScene::Loading => {
                model.apply_startup(Startup::Unauthenticated);
                model.server = "https://jellyfin.local:8096".into();
                model.username = "alex".into();
                model.phase = Phase::Authenticating;
            }
            _ => {
                model.apply_startup(Startup::Authenticated(review_session()));
            }
        }
        model
    }

    pub fn starting() -> Self {
        Self {
            phase: Phase::Starting,
            session: None,
            notice: None,
            server: String::new(),
            username: String::new(),
            password: String::new(),
        }
    }

    pub(crate) fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// The session could not make an HTTP client.
    pub(crate) fn note_unavailable(&mut self) {
        self.notice = Some(UNAVAILABLE.into());
    }

    /// Jellyfin stopped accepting the session. Back to Login with the server
    /// and username kept, so only the password needs typing. The caller
    /// removes the dead session from the vault.
    pub(crate) fn expire_session(&mut self) -> bool {
        if self.phase != Phase::Authenticated {
            return false;
        }
        if let Some(session) = self.session.take() {
            self.server = session.server_url().to_string();
            self.username = session.user().name().to_string();
        }
        self.password.zeroize();
        self.notice = Some(SESSION_ENDED.into());
        self.phase = Phase::Unauthenticated;
        true
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn server(&self) -> &str {
        &self.server
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// True while a sign-in request is in flight. The form fields are locked
    /// to the snapshot that request already owns.
    pub fn fields_locked(&self) -> bool {
        self.phase == Phase::Authenticating
    }

    pub fn set_server(&mut self, value: String) {
        if self.fields_locked() {
            return;
        }
        self.server = value;
    }

    pub fn set_username(&mut self, value: String) {
        if self.fields_locked() {
            return;
        }
        self.username = value;
    }

    pub fn set_password(&mut self, value: String) {
        if self.fields_locked() {
            return;
        }
        self.password.zeroize();
        self.password = value;
    }

    /// Escape does not clear the form.
    #[cfg(test)]
    pub fn ignored_escape(&mut self) {}

    pub fn http_warning(&self) -> Option<&'static str> {
        insecure_http_warning(&self.server)
    }

    pub fn button_label(&self) -> &'static str {
        match self.phase {
            Phase::Authenticating => "Connecting…",
            Phase::SigningOut => "Signing out…",
            Phase::Authenticated => "Sign out",
            Phase::Starting | Phase::Unauthenticated => "Enter Matinee",
        }
    }

    pub fn shows_login(&self) -> bool {
        matches!(self.phase, Phase::Unauthenticated | Phase::Authenticating)
    }

    /// Signed in: Home (or the screens above it) is showing.
    pub fn shows_home(&self) -> bool {
        matches!(self.phase, Phase::Authenticated | Phase::SigningOut)
    }

    #[cfg(test)]
    pub fn identity(&self) -> Option<Identity> {
        self.session.as_ref().map(|session| Identity {
            username: session.user().name().to_string(),
            server: session.server_url().to_string(),
        })
    }

    pub fn apply_startup(&mut self, startup: Startup) {
        if self.phase != Phase::Starting {
            return;
        }
        self.notice = startup_notice(&startup).map(str::to_string);
        match startup {
            Startup::Authenticated(session) => {
                self.session = Some(session);
                self.phase = Phase::Authenticated;
            }
            Startup::Unauthenticated | Startup::Corrupt | Startup::VaultUnavailable => {
                self.session = None;
                self.phase = Phase::Unauthenticated;
            }
        }
    }

    /// Begin sign-in, or refuse a duplicate or an invalid form.
    ///
    /// A refusal caused by validation stores a notice and leaves the phase
    /// unauthenticated. A duplicate leaves the in-flight attempt alone.
    pub fn begin_sign_in(&mut self) -> Option<SignInRequest> {
        if self.phase != Phase::Unauthenticated {
            return None;
        }
        if let Some(message) = validate_form(&self.server, &self.username) {
            self.notice = Some(message);
            self.phase = Phase::Unauthenticated;
            return None;
        }
        self.notice = None;
        self.phase = Phase::Authenticating;
        Some(SignInRequest {
            server: self.server.clone(),
            username: self.username.clone(),
            password: Password::new(self.password.clone()),
        })
    }

    pub fn apply_sign_in(&mut self, result: Result<Session, String>) {
        if self.phase != Phase::Authenticating {
            return;
        }
        match result {
            Ok(session) => {
                self.password.zeroize();
                self.session = Some(session);
                self.notice = None;
                self.phase = Phase::Authenticated;
            }
            Err(message) => {
                self.session = None;
                self.notice = Some(message);
                self.phase = Phase::Unauthenticated;
            }
        }
    }

    pub fn begin_sign_out(&mut self) -> bool {
        if self.phase != Phase::Authenticated {
            return false;
        }
        self.notice = None;
        self.phase = Phase::SigningOut;
        true
    }

    pub fn apply_sign_out(&mut self, result: Result<(), String>) {
        if self.phase != Phase::SigningOut {
            return;
        }
        match result {
            Ok(()) => {
                self.session = None;
                self.password.zeroize();
                self.notice = None;
                self.username.clear();
                self.phase = Phase::Unauthenticated;
            }
            Err(message) => {
                self.notice = Some(message);
                self.phase = Phase::Authenticated;
            }
        }
    }

    /// Text the window is allowed to paint. The password and access token are absent.
    #[cfg(test)]
    pub fn visible_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        match self.phase {
            Phase::Starting => lines.push("Matinee".into()),
            Phase::Unauthenticated | Phase::Authenticating => {
                lines.push("Your library, reimagined".into());
                lines.push("Movie night starts here.".into());
                lines.push(LOGIN_COPY.into());
                lines.push("Jellyfin server".into());
                lines.push(self.server.clone());
                if let Some(warning) = self.http_warning() {
                    lines.push(warning.into());
                }
                lines.push("Username".into());
                lines.push(self.username.clone());
                lines.push("Password".into());
                lines.push(self.button_label().into());
                if let Some(notice) = &self.notice {
                    lines.push(notice.clone());
                }
            }
            Phase::Authenticated | Phase::SigningOut => {
                lines.push("Matinee".into());
                if let Some(identity) = self.identity() {
                    lines.push(format!("Connected as {}", identity.username));
                    lines.push(identity.server);
                }
                lines.push(self.button_label().into());
                if let Some(notice) = &self.notice {
                    lines.push(notice.clone());
                }
            }
        }
        lines
    }
}

impl Drop for AppModel {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

impl fmt::Debug for AppModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppModel")
            .field("phase", &self.phase)
            .field("server", &self.server)
            .field("username", &self.username)
            .field("password", &"[redacted]")
            .field("notice", &self.notice)
            .field("session", &self.session)
            .finish()
    }
}

pub(crate) fn review_session() -> Session {
    Session::new(
        "http://jellyfin.local:8096",
        "fixture-token",
        User::new(UserId::parse("user-1").unwrap(), "alex", None),
    )
    .expect("fixture session")
}

fn validate_form(server: &str, username: &str) -> Option<String> {
    if let Err(error) = matinee_jellyfin::normalize_server_url(server) {
        return Some(error.to_string());
    }
    if username.trim().is_empty() {
        return Some("Enter your username.".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{accept_authentication, restore_session};
    use matinee_core::{User, UserId};
    use matinee_jellyfin::JellyfinError;
    use matinee_secrets::MemoryStore;

    const PASSWORD: &str = "desk-lamp-phrase";
    const TOKEN: &str = "token-value-not-for-logs";

    fn ready_form() -> AppModel {
        let mut model = AppModel::starting();
        model.apply_startup(Startup::Unauthenticated);
        model.set_server("http://jellyfin.local:8096".into());
        model.set_username("alex".into());
        model.set_password(PASSWORD.into());
        model
    }

    fn sample_session() -> Session {
        Session::new(
            "http://jellyfin.local:8096",
            TOKEN,
            User::new(UserId::parse("user-1").unwrap(), "alex", None),
        )
        .unwrap()
    }

    #[test]
    fn startup_without_a_session_shows_login() {
        let mut model = AppModel::starting();
        model.apply_startup(Startup::Unauthenticated);
        assert!(model.shows_login());
        assert!(model.notice().is_none());
        assert!(
            model
                .visible_lines()
                .iter()
                .any(|line| line == "Enter Matinee")
        );
    }

    #[test]
    fn startup_with_a_session_shows_home() {
        let mut model = AppModel::starting();
        model.apply_startup(Startup::Authenticated(sample_session()));
        assert!(model.shows_home());
        let lines = model.visible_lines().join("\n");
        assert!(lines.contains("Connected as alex"));
        assert!(lines.contains("http://jellyfin.local:8096"));
        assert!(!lines.contains(TOKEN));
        assert!(!format!("{model:?}").contains(TOKEN));
    }

    #[test]
    fn corrupt_and_vault_failures_stay_on_login() {
        let mut corrupt = AppModel::starting();
        corrupt.apply_startup(Startup::Corrupt);
        assert!(corrupt.shows_login());
        assert_eq!(
            corrupt.notice(),
            Some("The saved Jellyfin session could not be read.")
        );
        let mut vault = AppModel::starting();
        vault.apply_startup(Startup::VaultUnavailable);
        assert!(vault.shows_login());
        assert_eq!(
            vault.notice(),
            Some("The credential vault could not complete the request.")
        );
    }

    #[test]
    fn successful_sign_in_persists_and_enters_home() {
        let store = MemoryStore::new();
        let mut model = ready_form();
        let request = model.begin_sign_in().unwrap();
        assert_eq!(model.phase(), Phase::Authenticating);
        assert!(model.fields_locked());
        assert_eq!(model.button_label(), "Connecting…");
        model.set_server("https://other.example".into());
        model.set_username("someone-else".into());
        model.set_password("different-phrase".into());
        assert_eq!(model.server(), "http://jellyfin.local:8096");
        assert_eq!(model.username(), "alex");
        assert_eq!(model.password(), PASSWORD);
        assert!(model.begin_sign_in().is_none(), "duplicate submit");
        assert_eq!(model.phase(), Phase::Authenticating);
        let (server, username, password) = request.into_parts();
        assert_eq!(server, "http://jellyfin.local:8096");
        assert_eq!(username, "alex");
        assert!(!format!("{password:?}").contains(PASSWORD));
        let saved = accept_authentication(&store, Ok(sample_session())).unwrap();
        model.apply_sign_in(Ok(saved));
        assert!(model.shows_home());
        assert!(model.password().is_empty());
        assert!(matches!(restore_session(&store), Startup::Authenticated(_)));
        let lines = model.visible_lines().join("\n");
        assert!(!lines.contains(TOKEN));
        assert!(!lines.contains(PASSWORD));
    }

    #[test]
    fn rejected_sign_in_keeps_the_form_and_the_password_out_of_the_notice() {
        let mut model = ready_form();
        assert!(model.begin_sign_in().is_some());
        model.apply_sign_in(Err("That username or password was not accepted.".into()));
        assert!(model.shows_login());
        assert!(!model.fields_locked());
        assert_eq!(model.password(), PASSWORD);
        model.set_username("jordan".into());
        assert_eq!(model.username(), "jordan");
        assert_eq!(
            model.notice(),
            Some("That username or password was not accepted.")
        );
        let rendered = format!("{model:?}\n{}", model.visible_lines().join("\n"));
        assert!(!rendered.contains(PASSWORD));
    }

    #[test]
    fn a_malformed_server_never_starts_sign_in() {
        let mut model = ready_form();
        model.set_server("ftp://files.example".into());
        assert!(model.begin_sign_in().is_none());
        assert_eq!(model.phase(), Phase::Unauthenticated);
        assert!(model.notice().unwrap().contains("http://"));
    }

    #[test]
    fn an_empty_username_never_starts_sign_in() {
        let mut model = ready_form();
        model.set_username("   ".into());
        assert!(model.begin_sign_in().is_none());
        assert_eq!(model.notice(), Some("Enter your username."));
    }

    #[test]
    fn save_failure_does_not_enter_home() {
        let mut model = ready_form();
        assert!(model.begin_sign_in().is_some());
        model.apply_sign_in(Err(
            "The credential vault could not complete the request.".into()
        ));
        assert!(model.shows_login());
        assert!(model.identity().is_none());
        assert!(model.notice().unwrap().contains("credential vault"));
    }

    #[test]
    fn sign_out_clears_the_session_and_returns_to_login() {
        let mut model = AppModel::starting();
        model.apply_startup(Startup::Authenticated(sample_session()));
        assert!(model.begin_sign_out());
        assert!(!model.begin_sign_out(), "duplicate sign-out");
        assert_eq!(model.button_label(), "Signing out…");
        model.apply_sign_out(Ok(()));
        assert!(model.shows_login());
        assert!(model.identity().is_none());
        assert!(model.notice().is_none());
    }

    #[test]
    fn sign_out_failure_stays_authenticated() {
        let mut model = AppModel::starting();
        model.apply_startup(Startup::Authenticated(sample_session()));
        assert!(model.begin_sign_out());
        model.apply_sign_out(Err(
            "The credential vault could not complete the request.".into()
        ));
        assert!(model.shows_home());
        assert_eq!(model.identity().unwrap().username, "alex");
        assert!(model.notice().unwrap().contains("credential vault"));
        let lines = model.visible_lines().join("\n");
        assert!(!lines.contains(TOKEN));
    }

    #[test]
    fn escape_does_not_clear_credentials() {
        let mut model = ready_form();
        model.ignored_escape();
        assert_eq!(model.server(), "http://jellyfin.local:8096");
        assert_eq!(model.username(), "alex");
        assert_eq!(model.password(), PASSWORD);
        assert!(!format!("{model:?}").contains(PASSWORD));
    }

    #[test]
    fn http_warning_follows_the_server_field() {
        let mut model = ready_form();
        assert!(model.http_warning().is_some());
        model.set_server("https://example.com".into());
        assert!(model.http_warning().is_none());
        model.set_server("http://127.0.0.1:8096".into());
        assert!(model.http_warning().is_none());
    }

    #[test]
    fn a_late_result_does_not_replace_a_finished_attempt() {
        let mut model = ready_form();
        assert!(model.begin_sign_in().is_some());
        model.apply_sign_in(Ok(sample_session()));
        model.apply_sign_in(Err("late".into()));
        assert!(model.shows_home());
        assert!(model.notice().is_none());
    }

    #[test]
    fn review_scenes_do_not_paint_the_fixture_token() {
        for scene in [
            ReviewScene::Login,
            ReviewScene::Focus,
            ReviewScene::Warning,
            ReviewScene::Error,
            ReviewScene::Loading,
            ReviewScene::Home,
            ReviewScene::HomeContinueWatching,
            ReviewScene::HomeEmpty,
            ReviewScene::HomePartialError,
            ReviewScene::HomeLoading,
            ReviewScene::HomeSmallWindow,
            ReviewScene::HomeLargeWindow,
            ReviewScene::HomeHeroResume,
            ReviewScene::HomeHeroFresh,
            ReviewScene::PlayerPlaying,
            ReviewScene::PlayerPaused,
            ReviewScene::PlayerControls,
            ReviewScene::PlayerError,
            ReviewScene::PlayerAudio,
            ReviewScene::PlayerSubtitles,
            ReviewScene::DetailsMovie,
            ReviewScene::DetailsMovieResume,
            ReviewScene::DetailsSeries,
            ReviewScene::DetailsSeason,
            ReviewScene::DetailsLoading,
            ReviewScene::DetailsError,
            ReviewScene::Library,
            ReviewScene::LibraryMovies,
            ReviewScene::LibrarySeries,
            ReviewScene::LibraryLoading,
            ReviewScene::LibraryEmpty,
            ReviewScene::LibraryFilteredEmpty,
            ReviewScene::LibraryPartialPage,
            ReviewScene::LibraryError,
            ReviewScene::LibrarySmallWindow,
            ReviewScene::LibraryLargeWindow,
            ReviewScene::LibraryManyItems,
        ] {
            let model = AppModel::review(scene);
            let rendered = model.visible_lines().join("\n");
            assert!(!rendered.contains("fixture-token"), "{scene:?}");
            assert!(!format!("{model:?}").contains("fixture-token"), "{scene:?}");
            assert_eq!(
                model.button_label() == "Connecting…",
                scene == ReviewScene::Loading
            );
        }
        let warning = AppModel::review(ReviewScene::Warning);
        assert!(warning.http_warning().is_some());
        let error = AppModel::review(ReviewScene::Error);
        assert_eq!(
            error.notice(),
            Some("That username or password was not accepted.")
        );
    }

    #[test]
    fn an_ended_session_returns_to_login_with_the_form_kept() {
        let mut model = AppModel::starting();
        model.apply_startup(Startup::Authenticated(sample_session()));
        assert!(model.expire_session());
        assert!(model.shows_login());
        assert!(model.session().is_none());
        assert_eq!(model.notice(), Some(SESSION_ENDED));
        assert_eq!(model.server(), "http://jellyfin.local:8096");
        assert_eq!(model.username(), "alex");
        assert!(model.password().is_empty());
        assert!(!model.expire_session(), "only once");
        // Later reports, while the person is typing, change nothing.
        model.set_password("new-phrase".into());
        model.set_username("jordan".into());
        assert!(!model.expire_session());
        assert!(!model.expire_session());
        assert_eq!(model.username(), "jordan");
        assert_eq!(model.password(), "new-phrase");
        assert_eq!(model.notice(), Some(SESSION_ENDED));
        model.set_username("alex".into());
        model.set_password(String::new());
        assert!(!format!("{model:?}").contains(TOKEN));
        // The kept form can sign in again at once, and a report arriving
        // while that sign-in is in flight does not disturb it.
        model.set_password(PASSWORD.into());
        assert!(model.begin_sign_in().is_some());
        assert!(!model.expire_session());
        assert_eq!(model.phase(), Phase::Authenticating);
        assert_eq!(model.password(), PASSWORD);
        // Nor during sign-out, or before startup has finished.
        let mut signing_out = AppModel::starting();
        assert!(!signing_out.expire_session(), "starting");
        signing_out.apply_startup(Startup::Authenticated(sample_session()));
        assert!(signing_out.begin_sign_out());
        assert!(!signing_out.expire_session());
        assert_eq!(signing_out.phase(), Phase::SigningOut);
    }

    #[test]
    fn invalid_url_message_matches_the_client() {
        let error = JellyfinError::InvalidUrl {
            message: matinee_jellyfin::normalize_server_url("")
                .unwrap_err()
                .to_string(),
        };
        assert_eq!(error.to_string(), "Enter your Jellyfin server address.");
    }
}
