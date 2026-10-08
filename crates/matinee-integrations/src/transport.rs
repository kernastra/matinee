//! HTTP boundary. Redirects stay off. The API key is a header, never a query.
//! A request can cap its body: the cap is checked against the announced
//! length and again while the body streams, so an oversized answer is never
//! held in memory.

use std::fmt;
use std::future::Future;
use std::time::Duration;

use crate::error::IntegrationError;
use crate::redact::redact;

#[derive(Clone)]
pub struct IntegrationRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    /// The largest body accepted, in bytes. `None` reads the whole body, as
    /// every Radarr and Sonarr API call does.
    pub max_body: Option<usize>,
}

impl fmt::Debug for IntegrationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<(String, String)> = self
            .headers
            .iter()
            .map(|(name, value)| {
                if name.eq_ignore_ascii_case("x-api-key")
                    || name.eq_ignore_ascii_case("authorization")
                {
                    (name.clone(), "[redacted]".to_string())
                } else {
                    (name.clone(), value.clone())
                }
            })
            .collect();
        formatter
            .debug_struct("IntegrationRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("query", &self.query)
            .field("max_body", &self.max_body)
            .finish()
    }
}

pub struct IntegrationResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl fmt::Debug for IntegrationResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IntegrationResponse")
            .field("status", &self.status)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

#[derive(Debug)]
pub struct TransportError {
    pub detail: String,
}

/// Detail of the error for a body over its request's `max_body`.
const BODY_TOO_LARGE: &str = "response body is larger than the request allows";

impl TransportError {
    pub(crate) fn body_too_large() -> Self {
        Self {
            detail: BODY_TOO_LARGE.to_string(),
        }
    }

    /// The body was refused for its size, not lost in transit.
    pub fn is_body_too_large(&self) -> bool {
        self.detail == BODY_TOO_LARGE
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

pub trait Transport: Send + Sync {
    fn send(
        &self,
        request: IntegrationRequest,
    ) -> impl Future<Output = Result<IntegrationResponse, TransportError>> + Send;
}

/// Production transport. The caller polls it on a Tokio runtime.
///
/// Redirects are disabled, matching the shipping client. A 3xx response is
/// returned as a failed status and is not followed, so a key or a request
/// never moves to another host. The timeout is 20 seconds and covers the
/// body. A request's `max_body` is enforced before and while the body is
/// read. This type does not create a runtime.
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, IntegrationError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| IntegrationError::Client {
                detail: redact(&error.to_string()),
            })?;
        Ok(Self { client })
    }
}

impl Transport for ReqwestTransport {
    async fn send(
        &self,
        request: IntegrationRequest,
    ) -> Result<IntegrationResponse, TransportError> {
        let mut builder = self.client.get(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if !request.query.is_empty() {
            builder = builder.query(&request.query);
        }
        let mut response = builder.send().await.map_err(|error| TransportError {
            detail: redact(&error.to_string()),
        })?;
        let status = response.status().as_u16();
        let body = match request.max_body {
            None => response
                .bytes()
                .await
                .map_err(|error| TransportError {
                    detail: redact(&error.to_string()),
                })?
                .to_vec(),
            Some(limit) => read_capped(&mut response, limit).await?,
        };
        log::debug!(
            "integration {method} {url} -> {status}",
            method = request.method,
            url = crate::redact::log_target(&request.url)
        );
        Ok(IntegrationResponse { status, body })
    }
}

/// Read at most `limit` bytes of body. A larger announced length is refused
/// before any of it is read, and a stream that runs past the limit is dropped
/// at the chunk that crosses it, which closes the connection.
async fn read_capped(
    response: &mut reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, TransportError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(TransportError::body_too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| TransportError {
        detail: redact(&error.to_string()),
    })? {
        if chunk.len() > limit - body.len() {
            return Err(TransportError::body_too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
