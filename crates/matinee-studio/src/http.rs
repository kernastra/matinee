//! HTTP for reference images and fal. Redirects are not followed here.
//!
//! The caller decides whether a 3xx `Location` is safe to request next. A
//! provider URL cannot become an open-ended fetch: scheme, credentials, size,
//! and content type are checked around this call.

use std::fmt;
use std::future::Future;
use std::time::Duration;

use crate::error::StudioError;
use crate::redact::redact;

#[derive(Clone)]
pub struct StudioRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
    pub max_bytes: usize,
}

impl fmt::Debug for StudioRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<(String, &str)> = self
            .headers
            .iter()
            .map(|(name, _)| {
                if name.eq_ignore_ascii_case("authorization")
                    || name.eq_ignore_ascii_case("x-api-key")
                {
                    (name.clone(), "[redacted]")
                } else {
                    (name.clone(), "[present]")
                }
            })
            .collect();
        formatter
            .debug_struct("StudioRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("max_bytes", &self.max_bytes)
            .finish()
    }
}

pub struct StudioResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl fmt::Debug for StudioResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StudioResponse")
            .field("status", &self.status)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

pub trait StudioHttp: Send + Sync {
    fn send(
        &self,
        request: StudioRequest,
    ) -> impl Future<Output = Result<StudioResponse, StudioError>> + Send;
}

pub struct ReqwestStudio {
    client: reqwest::Client,
}

impl ReqwestStudio {
    pub fn new() -> Result<Self, StudioError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| StudioError::provider(error.to_string()))?;
        Ok(Self { client })
    }
}

impl StudioHttp for ReqwestStudio {
    async fn send(&self, request: StudioRequest) -> Result<StudioResponse, StudioError> {
        let timeout = request.timeout;
        let max_bytes = request.max_bytes;
        let method = request.method;
        let url = request.url.clone();
        let mut builder = match method {
            "POST" => self.client.post(&request.url),
            _ => self.client.get(&request.url),
        };
        builder = builder.timeout(timeout);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let response = builder
            .send()
            .await
            .map_err(|error| StudioError::provider(redact(&error.to_string())))?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes as u64)
        {
            return Err(StudioError::ImageTooLarge);
        }
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
            })
            .collect();
        let mut body = Vec::new();
        let mut response = response;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) if body.len() + chunk.len() <= max_bytes => {
                    body.extend_from_slice(&chunk)
                }
                Ok(Some(_)) => return Err(StudioError::ImageTooLarge),
                Ok(None) => break,
                Err(error) => return Err(StudioError::provider(redact(&error.to_string()))),
            }
        }
        log::debug!("studio {method} {url} -> {status} ({} bytes)", body.len());
        Ok(StudioResponse {
            status,
            headers,
            body,
        })
    }
}
