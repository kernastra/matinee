//! HTTP boundary. Redirects stay off. The API key is a header, never a query.

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
/// returned as a failed status and is not followed. The timeout is 20 seconds.
/// This type does not create a runtime.
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
        let response = builder.send().await.map_err(|error| TransportError {
            detail: redact(&error.to_string()),
        })?;
        let status = response.status().as_u16();
        let body = response.bytes().await.map_err(|error| TransportError {
            detail: redact(&error.to_string()),
        })?;
        log::debug!(
            "integration {method} {url} -> {status}",
            method = request.method,
            url = request.url
        );
        Ok(IntegrationResponse {
            status,
            body: body.to_vec(),
        })
    }
}
