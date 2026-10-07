//! Authenticated Jellyfin requests.
//!
//! There is no background timer. Playback progress is reported only when the
//! application calls [`JellyfinClient::report_playback`].

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::JellyfinError;
use crate::redact::describe_exchange;
use crate::session::Session;
use crate::transport::{CancelFlag, HttpRequest, HttpResponse, Method, Transport};
use crate::url::join_server;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Endpoint {
    System,
    Home,
    Library,
    Item,
    Search,
    Series,
    UserData,
    Playback,
    Artwork,
}

impl Endpoint {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Home => "home",
            Self::Library => "library",
            Self::Item => "item",
            Self::Search => "search",
            Self::Series => "series",
            Self::UserData => "user-data",
            Self::Playback => "playback",
            Self::Artwork => "artwork",
        }
    }
}

pub struct JellyfinClient<T: Transport> {
    session: Session,
    transport: T,
}

impl<T: Transport> JellyfinClient<T> {
    pub fn new(session: Session, transport: T) -> Self {
        Self { session, transport }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub(crate) async fn get_json<D: DeserializeOwned>(
        &self,
        endpoint: Endpoint,
        path: &str,
        context: &str,
    ) -> Result<D, JellyfinError> {
        let response = self
            .request(endpoint, Method::Get, path, None, None)
            .await?;
        serde_json::from_slice(&response.body).map_err(|_| JellyfinError::malformed(context))
    }

    pub(crate) async fn request(
        &self,
        endpoint: Endpoint,
        method: Method,
        path: &str,
        body: Option<String>,
        cancel: Option<&CancelFlag>,
    ) -> Result<HttpResponse, JellyfinError> {
        if cancel.is_some_and(CancelFlag::is_cancelled) {
            return Err(JellyfinError::Cancelled);
        }
        let url = join_server(self.session.server_url(), path)?;
        let mut headers = vec![(
            "Authorization".to_string(),
            self.session.authorization_header()?,
        )];
        if body.is_some() {
            headers.push(("Content-Type".to_string(), "application/json".to_string()));
        }
        let response = self
            .transport
            .send(HttpRequest {
                method,
                url: url.clone(),
                headers,
                body,
                cancel: cancel.cloned(),
            })
            .await
            .map_err(JellyfinError::from_transport)?;
        log::info!(
            target: "matinee_jellyfin",
            "{}",
            describe_exchange(endpoint.as_str(), method.as_str(), &url, Some(response.status))
        );
        if !(200..300).contains(&response.status) {
            return Err(JellyfinError::for_status(response.status, false));
        }
        Ok(response)
    }

    pub(crate) async fn post_json(
        &self,
        endpoint: Endpoint,
        path: &str,
        body: &Value,
        cancel: Option<&CancelFlag>,
    ) -> Result<HttpResponse, JellyfinError> {
        let encoded =
            serde_json::to_string(body).map_err(|_| JellyfinError::malformed("request"))?;
        self.request(endpoint, Method::Post, path, Some(encoded), cancel)
            .await
    }
}
