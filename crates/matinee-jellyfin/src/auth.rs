//! Authentication header and the username/password exchange.
//!
//! The password is not stored on the session. The JSON body that carries it
//! is zeroed after the request is built.

use std::fmt;

use serde::Serialize;
use zeroize::Zeroize;

use crate::convert::user_from_dto;
use crate::dto::AuthDto;
use crate::error::JellyfinError;
use crate::redact::describe_exchange;
use crate::session::Session;
use crate::transport::{HttpRequest, Method, Transport};
use crate::url::{join_server, normalize_server_url};
use crate::{CLIENT_NAME, CLIENT_VERSION, DEVICE_ID, DEVICE_NAME};

/// A password that is wiped when dropped.
pub struct Password(String);

impl Password {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for Password {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password([redacted])")
    }
}

struct AuthBody {
    username: String,
    password: String,
}

impl Drop for AuthBody {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

impl Serialize for AuthBody {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AuthBody", 2)?;
        state.serialize_field("Username", &self.username)?;
        state.serialize_field("Pw", &self.password)?;
        state.end()
    }
}

pub(crate) fn authorization_header(token: Option<&str>) -> Result<String, JellyfinError> {
    if let Some(token) = token {
        validate_token(token)?;
    }
    let mut header = format!(
        "MediaBrowser Client=\"{CLIENT_NAME}\", Device=\"{DEVICE_NAME}\", DeviceId=\"{DEVICE_ID}\", Version=\"{CLIENT_VERSION}\""
    );
    if let Some(token) = token {
        header.push_str(", Token=\"");
        header.push_str(token);
        header.push('"');
    }
    Ok(header)
}

pub(crate) fn validate_token(token: &str) -> Result<(), JellyfinError> {
    if token.is_empty()
        || token.len() > 4096
        || token
            .chars()
            .any(|character| character.is_control() || character == '"' || character == '\\')
    {
        return Err(JellyfinError::malformed("access token"));
    }
    Ok(())
}

pub async fn authenticate(
    transport: &impl Transport,
    server_url: &str,
    username: &str,
    password: Password,
) -> Result<Session, JellyfinError> {
    let server_url = normalize_server_url(server_url)?;
    let body = AuthBody {
        username: username.to_string(),
        password: password.expose().to_string(),
    };
    drop(password);
    let encoded = serde_json::to_string(&body)
        .map_err(|_| JellyfinError::malformed("authentication request"))?;
    let url = join_server(&server_url, "/Users/AuthenticateByName")?;
    let header = authorization_header(None)?;
    let response = transport
        .send(HttpRequest {
            method: Method::Post,
            url: url.clone(),
            headers: vec![
                ("Authorization".to_string(), header),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: Some(encoded),
            cancel: None,
        })
        .await
        .map_err(JellyfinError::from_transport);
    drop(body);
    let response = response?;
    log::info!(
        target: "matinee_jellyfin",
        "{}",
        describe_exchange("auth", "POST", &url, Some(response.status))
    );
    if !(200..300).contains(&response.status) {
        return Err(JellyfinError::for_status(response.status, true));
    }
    let dto: AuthDto = serde_json::from_slice(&response.body)
        .map_err(|_| JellyfinError::malformed("authentication"))?;
    if dto.access_token.trim().is_empty() {
        return Err(JellyfinError::malformed("authentication"));
    }
    let user = user_from_dto(dto.user)?;
    Session::new(server_url, dto.access_token, user)
}
