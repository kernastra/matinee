//! HTTP boundary. Redirects stay off. The API key is a header, never a query.
//! A request can cap its body: the cap is checked against the announced
//! length and again while the body streams, so an oversized answer is never
//! held in memory. A request marked `public_only` (artwork) goes through a
//! separate client that reaches only public addresses; see
//! [`crate::destination`].

use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use url::Url;

use crate::destination::{self, PublicResolver};
use crate::error::IntegrationError;
use crate::redact::redact;

/// Every request's limit, connection and body included.
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct IntegrationRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    /// The largest body accepted, in bytes. `None` reads the whole body, as
    /// every Radarr and Sonarr API call does.
    pub max_body: Option<usize>,
    /// Reach only a public host. Artwork addresses come from the server's
    /// JSON and set this; API calls go to the configured server and do not.
    pub public_only: bool,
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
            .field("public_only", &self.public_only)
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
/// Detail of the error for a `public_only` request to a non-public host.
const FORBIDDEN_DESTINATION: &str = "the host is not a public address";

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

    pub(crate) fn forbidden_destination() -> Self {
        Self {
            detail: FORBIDDEN_DESTINATION.to_string(),
        }
    }

    /// A `public_only` request named, or resolved to, a host that is not
    /// public. Nothing was sent.
    pub fn is_forbidden_destination(&self) -> bool {
        self.detail == FORBIDDEN_DESTINATION
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
///
/// Two clients share these rules. `client` reaches the configured Radarr and
/// Sonarr servers, wherever they are. `public` serves `public_only` requests:
/// its resolver refuses any name with a non-public answer and hands the
/// connector only the addresses it checked, it ignores proxy settings (a
/// proxy would resolve the name itself), and an IP literal is checked before
/// the request because the connector does not resolve it.
pub struct ReqwestTransport {
    client: reqwest::Client,
    public: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, IntegrationError> {
        Self::build(PublicResolver::system(), TIMEOUT)
    }

    fn build(resolver: PublicResolver, public_timeout: Duration) -> Result<Self, IntegrationError> {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(client_error)?;
        let public = reqwest::Client::builder()
            .timeout(public_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .dns_resolver(Arc::new(resolver))
            .build()
            .map_err(client_error)?;
        Ok(Self { client, public })
    }

    /// A transport whose artwork client resolves with `lookup` and allows
    /// what `allow` allows, so tests can stand loopback servers in for public
    /// hosts and private ones.
    #[cfg(test)]
    pub(crate) fn with_public_lookup(
        lookup: destination::Lookup,
        allow: fn(std::net::IpAddr) -> bool,
        timeout: Duration,
    ) -> Self {
        Self::build(PublicResolver::new(lookup, allow), timeout).expect("test transport")
    }
}

fn client_error(error: reqwest::Error) -> IntegrationError {
    IntegrationError::Client {
        detail: redact(&error.to_string()),
    }
}

impl Transport for ReqwestTransport {
    async fn send(
        &self,
        request: IntegrationRequest,
    ) -> Result<IntegrationResponse, TransportError> {
        let client = if request.public_only {
            let allowed = Url::parse(&request.url)
                .ok()
                .is_some_and(|url| destination::public_url(&url));
            if !allowed {
                return Err(TransportError::forbidden_destination());
            }
            &self.public
        } else {
            &self.client
        };
        let mut builder = client.get(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if !request.query.is_empty() {
            builder = builder.query(&request.query);
        }
        let mut response = builder.send().await.map_err(|error| {
            if destination::is_forbidden(&error) {
                TransportError::forbidden_destination()
            } else {
                TransportError {
                    detail: redact(&error.to_string()),
                }
            }
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
