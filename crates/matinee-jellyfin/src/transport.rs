//! HTTP transport.
//!
//! [`ReqwestTransport`] is the production client: HTTP and HTTPS, timeouts,
//! and same-origin redirects. Tests inject a [`Transport`] and never open a
//! socket. Dropping a [`ReqwestTransport`] request future cancels it.

use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::CLIENT_VERSION;
use crate::error::JellyfinError;
use crate::redact::redact_header_value;
use crate::url::same_origin;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Delete => "DELETE",
        }
    }
}

/// Stops a call that has not started.
///
/// [`CancelFlag::cancel`] is read once, before the transport builds the HTTP
/// call. It does not interrupt a request already on the wire. Dropping the
/// future from [`ReqwestTransport`] cancels that in-flight call; native
/// Search stops a superseded page that way, by aborting its runtime task.
#[derive(Clone, Debug, Default)]
pub struct CancelFlag {
    cancelled: Arc<AtomicBool>,
}

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub cancel: Option<CancelFlag>,
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<(String, String)> = self
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), redact_header_value(name, value)))
            .collect();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &crate::redact::redact_url(&self.url))
            .field("headers", &headers)
            .field("body", &self.body.as_ref().map(|_| "<omitted>"))
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum TransportError {
    Unreachable(String),
    Cancelled,
}

pub trait Transport: Send + Sync {
    fn send(
        &self,
        request: HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send;
}

#[derive(Clone, Debug)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, JellyfinError> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::custom(
                |attempt| match follow_redirect(attempt.previous(), attempt.url()) {
                    Ok(()) => attempt.follow(),
                    Err(reason) => attempt.error(RedirectReject(reason)),
                },
            ))
            .user_agent(format!("Matinee/{CLIENT_VERSION}"))
            .build()
            .map_err(|_| JellyfinError::unreachable("The HTTP client could not be created."))?;
        Ok(Self { client })
    }
}

impl Transport for ReqwestTransport {
    fn send(
        &self,
        request: HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send {
        let client = self.client.clone();
        async move { execute(client, request).await }
    }
}

async fn execute(
    client: reqwest::Client,
    request: HttpRequest,
) -> Result<HttpResponse, TransportError> {
    if request
        .cancel
        .as_ref()
        .is_some_and(CancelFlag::is_cancelled)
    {
        return Err(TransportError::Cancelled);
    }
    let method = match request.method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Delete => reqwest::Method::DELETE,
    };
    let mut builder = client.request(method, &request.url);
    for (name, value) in &request.headers {
        let name = reqwest::header::HeaderName::try_from(name.as_str())
            .map_err(|_| TransportError::Unreachable("invalid header".into()))?;
        let value = reqwest::header::HeaderValue::try_from(value.as_str())
            .map_err(|_| TransportError::Unreachable("invalid header".into()))?;
        builder = builder.header(name, value);
    }
    if let Some(body) = request.body {
        builder = builder.body(body);
    }
    let response = builder.send().await.map_err(map_reqwest)?;
    let status = response.status().as_u16();
    let body = response.bytes().await.map_err(map_reqwest)?.to_vec();
    Ok(HttpResponse { status, body })
}

pub(crate) fn follow_redirect(previous: &[url::Url], next: &url::Url) -> Result<(), &'static str> {
    if previous.len() >= 5 {
        return Err("too many redirects");
    }
    if let Some(start) = previous.first()
        && !same_origin(start, next)
    {
        return Err("cross-origin redirect");
    }
    Ok(())
}

fn map_reqwest(error: reqwest::Error) -> TransportError {
    if error.is_timeout() {
        return TransportError::Unreachable("timed out".into());
    }
    let text = crate::redact::redact_freeform(&error.to_string());
    let lower = text.to_ascii_lowercase();
    if lower.contains("cancel") {
        return TransportError::Cancelled;
    }
    if lower.contains("cross-origin") || lower.contains("too many redirects") {
        return TransportError::Unreachable("redirect rejected".into());
    }
    TransportError::Unreachable(text)
}

#[derive(Debug)]
struct RedirectReject(&'static str);

impl fmt::Display for RedirectReject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for RedirectReject {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_stay_on_the_origin_and_stop_after_five() {
        let start = url::Url::parse("http://jellyfin.local:8096/jellyfin").unwrap();
        let same = url::Url::parse("http://jellyfin.local:8096/Videos/1").unwrap();
        let other = url::Url::parse("https://evil.example/steal").unwrap();
        assert!(follow_redirect(std::slice::from_ref(&start), &same).is_ok());
        assert_eq!(
            follow_redirect(std::slice::from_ref(&start), &other),
            Err("cross-origin redirect")
        );
        let chain = vec![
            start.clone(),
            same.clone(),
            same.clone(),
            same.clone(),
            same,
        ];
        assert_eq!(chain.len(), 5);
        assert_eq!(follow_redirect(&chain, &start), Err("too many redirects"));
    }
}
