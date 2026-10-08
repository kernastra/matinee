//! Radarr and Sonarr operations shared by Tauri and, later, the native app.
//!
//! The upcoming cache lives here. Its key is the configured provider addresses
//! plus the requested range. Entries last five minutes. Identical concurrent
//! requests share one fetch. Dropping every waiter drops that fetch.
//! `test_and_save`, `remove`, and [`Integrations::invalidate_cache`] drop
//! cached results. A fetch that finishes after invalidation is not stored.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures::FutureExt;
use futures::future::Shared;
use serde::Deserialize;
use serde_json::Value;
use url::Url;
use zeroize::Zeroize;

use matinee_secrets::{CredentialKey, CredentialNamespace, CredentialStore, Secret};

use crate::error::IntegrationError;
use crate::home::home_upcoming;
use crate::model::{
    IntegrationConnection, IntegrationKeyStatus, UpcomingQuery, UpcomingRelease, UpcomingResult,
};
use crate::normalize::normalize_calendar;
use crate::provider::IntegrationProvider;
use crate::time::{in_window, parse_instant};
use crate::transport::{IntegrationRequest, Transport};
use crate::url::normalize_server_url;

pub const UPCOMING_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const MIN_API_KEY_CHARS: usize = 8;
/// Largest artwork body accepted: 16 MiB. Cover art is far smaller.
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

type UpcomingFuture = Shared<Pin<Box<dyn Future<Output = UpcomingResult> + Send>>>;

struct Inflight {
    generation: u64,
    future: UpcomingFuture,
}

struct CacheEntry {
    stored_at: Instant,
    generation: u64,
    result: UpcomingResult,
}

struct CacheInner {
    generation: u64,
    entries: HashMap<String, CacheEntry>,
    inflight: HashMap<String, Weak<Inflight>>,
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

pub struct Integrations<S, T, C = SystemClock> {
    store: Arc<S>,
    transport: Arc<T>,
    cache: Arc<Mutex<CacheInner>>,
    ttl: Duration,
    clock: C,
}

impl<S, T> Integrations<S, T, SystemClock>
where
    S: CredentialStore + 'static,
    T: Transport + 'static,
{
    pub fn new(store: S, transport: T) -> Self {
        Self::with_clock(store, transport, SystemClock, UPCOMING_CACHE_TTL)
    }
}

impl<S, T, C> Integrations<S, T, C>
where
    S: CredentialStore + 'static,
    T: Transport + 'static,
    C: Clock + 'static,
{
    pub fn with_clock(store: S, transport: T, clock: C, ttl: Duration) -> Self {
        Self {
            store: Arc::new(store),
            transport: Arc::new(transport),
            cache: Arc::new(Mutex::new(CacheInner {
                generation: 0,
                entries: HashMap::new(),
                inflight: HashMap::new(),
            })),
            ttl,
            clock,
        }
    }

    #[cfg(test)]
    pub(crate) fn credential_store(&self) -> &S {
        &self.store
    }

    pub fn invalidate_cache(&self) {
        let mut cache = lock(&self.cache);
        cache.generation = cache.generation.saturating_add(1);
        cache.entries.clear();
        cache.inflight.clear();
    }

    pub fn key_status(
        &self,
        provider: IntegrationProvider,
    ) -> Result<IntegrationKeyStatus, IntegrationError> {
        let configured = match load_stored(&*self.store, provider) {
            Ok(Some(stored)) => {
                !stored.server_url.trim().is_empty() && !stored.api_key.trim().is_empty()
            }
            Ok(None) | Err(IntegrationError::CorruptCredential { .. }) => false,
            Err(error) => return Err(error),
        };
        Ok(IntegrationKeyStatus {
            provider,
            configured,
        })
    }

    pub async fn test_and_save(
        &self,
        provider: IntegrationProvider,
        server_url: &str,
        api_key: Option<Secret>,
    ) -> Result<IntegrationConnection, IntegrationError> {
        let server_url = normalize_server_url(server_url)?;
        let supplied = api_key.and_then(|key| {
            let trimmed = key.expose().trim().to_string();
            (!trimmed.is_empty()).then_some(Secret::new(trimmed))
        });
        let key = if let Some(key) = supplied {
            key
        } else {
            let mut stored = load_stored(&*self.store, provider)?
                .ok_or(IntegrationError::NotConfigured { provider })?;
            if stored.server_url != server_url {
                return Err(IntegrationError::KeyBoundToOtherServer { provider });
            }
            Secret::new(std::mem::take(&mut stored.api_key))
        };
        if key.expose().len() < MIN_API_KEY_CHARS {
            return Err(IntegrationError::IncompleteKey);
        }
        let body = self
            .status_document(provider, &server_url, key.expose())
            .await?;
        let app_name = body
            .get("appName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !app_name.contains(provider.as_str()) {
            return Err(IntegrationError::WrongProvider { provider });
        }
        save_stored(&*self.store, provider, &server_url, key.expose())?;
        self.invalidate_cache();
        log::info!("saved {provider} integration");
        Ok(IntegrationConnection {
            provider,
            configured: true,
            server_url,
            version: body
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    pub fn remove(
        &self,
        provider: IntegrationProvider,
    ) -> Result<IntegrationKeyStatus, IntegrationError> {
        let (namespace, key) = credential_name(provider)?;
        self.store.remove(&namespace, &key)?;
        self.invalidate_cache();
        Ok(IntegrationKeyStatus {
            provider,
            configured: false,
        })
    }

    /// One provider's calendar for `[start, end)`, with no cache.
    ///
    /// This is the native path. It fails on a body that is not a JSON array,
    /// where [`Integrations::upcoming`] reports no releases, so a broken server
    /// is not shown as an empty month. Results are ordered by instant, then by
    /// id.
    pub async fn fetch_calendar(
        &self,
        provider: IntegrationProvider,
        start: &str,
        end: &str,
    ) -> Result<Vec<UpcomingRelease>, IntegrationError> {
        let start_instant = parse_instant(start).ok_or(IntegrationError::InvalidWindow)?;
        let end_instant = parse_instant(end).ok_or(IntegrationError::InvalidWindow)?;
        if end_instant <= start_instant {
            return Err(IntegrationError::InvalidWindow);
        }
        let body = request_calendar(&*self.store, &*self.transport, provider, start, end).await?;
        if !body.is_array() {
            return Err(IntegrationError::MalformedResponse { provider });
        }
        let mut events = normalize_in_window(provider, &body, start_instant, end_instant);
        events.sort_by(|left, right| {
            left.instant
                .cmp(&right.instant)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(events)
    }

    /// Bytes of one artwork address that a calendar response named.
    ///
    /// Only http and https addresses are fetched, and no credential is sent:
    /// the image host is a public cover host, not the Radarr or Sonarr server.
    /// Redirects are not followed (the shared transport does not follow them).
    /// A body over 16 MiB is refused after it arrives.
    pub async fn fetch_image(
        &self,
        provider: IntegrationProvider,
        url: &str,
    ) -> Result<Vec<u8>, IntegrationError> {
        let parsed = Url::parse(url).map_err(|_| IntegrationError::InvalidImage { provider })?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(IntegrationError::InvalidImage { provider });
        }
        let response = self
            .transport
            .send(IntegrationRequest {
                method: "GET",
                url: url.to_string(),
                headers: Vec::new(),
                query: Vec::new(),
            })
            .await
            .map_err(|error| IntegrationError::unreachable(provider, error.detail))?;
        if !(200..300).contains(&response.status) {
            return Err(IntegrationError::ImageUnavailable {
                provider,
                status: response.status,
            });
        }
        if response.body.len() > MAX_IMAGE_BYTES {
            return Err(IntegrationError::InvalidImage { provider });
        }
        Ok(response.body)
    }

    pub async fn upcoming(&self, query: UpcomingQuery) -> UpcomingResult {
        let key = query.cache_key();
        let now = self.clock.now();
        let task = {
            let mut cache = lock(&self.cache);
            if let Some(entry) = cache.entries.get(&key)
                && entry.generation == cache.generation
                && now.saturating_duration_since(entry.stored_at) < self.ttl
            {
                return entry.result.clone();
            }
            if let Some(existing) = cache.inflight.get(&key).and_then(Weak::upgrade)
                && existing.generation == cache.generation
            {
                Arc::clone(&existing)
            } else {
                let generation = cache.generation;
                let future = self.spawn_upcoming(query).boxed().shared();
                let task = Arc::new(Inflight { generation, future });
                cache.inflight.insert(key.clone(), Arc::downgrade(&task));
                task
            }
        };
        let generation = task.generation;
        let result = task.future.clone().await;
        let mut cache = lock(&self.cache);
        let same_task = cache
            .inflight
            .get(&key)
            .and_then(Weak::upgrade)
            .is_some_and(|current| current.generation == generation);
        if generation == cache.generation {
            cache.entries.insert(
                key.clone(),
                CacheEntry {
                    stored_at: self.clock.now(),
                    generation,
                    result: result.clone(),
                },
            );
        }
        if same_task {
            cache.inflight.remove(&key);
        }
        result
    }

    fn spawn_upcoming(
        &self,
        query: UpcomingQuery,
    ) -> impl Future<Output = UpcomingResult> + Send + 'static {
        let store = Arc::clone(&self.store);
        let transport = Arc::clone(&self.transport);
        async move { collect_upcoming(store.as_ref(), transport.as_ref(), query).await }
    }

    async fn status_document(
        &self,
        provider: IntegrationProvider,
        server_url: &str,
        api_key: &str,
    ) -> Result<Value, IntegrationError> {
        let response = send(
            &*self.transport,
            provider,
            format!("{server_url}/api/v3/system/status"),
            api_key,
            Vec::new(),
        )
        .await?;
        decode_json(provider, response.status, &response.body)
    }
}

async fn collect_upcoming<S, T>(store: &S, transport: &T, query: UpcomingQuery) -> UpcomingResult
where
    S: CredentialStore,
    T: Transport,
{
    let mut events = Vec::new();
    let mut errors = std::collections::BTreeMap::new();
    let Some(start_instant) = parse_instant(&query.start) else {
        return window_failure(&query);
    };
    let Some(end_instant) = parse_instant(&query.end) else {
        return window_failure(&query);
    };
    if end_instant <= start_instant {
        return window_failure(&query);
    }
    for (provider, configured) in [
        (
            IntegrationProvider::Radarr,
            !query.radarr_url.trim().is_empty(),
        ),
        (
            IntegrationProvider::Sonarr,
            !query.sonarr_url.trim().is_empty(),
        ),
    ] {
        if !configured {
            continue;
        }
        match fetch_provider(
            store,
            transport,
            provider,
            &query.start,
            &query.end,
            start_instant,
            end_instant,
        )
        .await
        {
            Ok(mut provider_events) => events.append(&mut provider_events),
            Err(error) => {
                errors.insert(provider.as_str().to_string(), error.to_string());
            }
        }
    }
    events.sort_by(|left, right| left.instant.cmp(&right.instant));
    let home = home_upcoming(&events, query.home_limit);
    UpcomingResult {
        events,
        errors,
        home,
    }
}

fn window_failure(query: &UpcomingQuery) -> UpcomingResult {
    let mut errors = std::collections::BTreeMap::new();
    let message = IntegrationError::InvalidWindow.to_string();
    if !query.radarr_url.trim().is_empty() {
        errors.insert("radarr".to_string(), message.clone());
    }
    if !query.sonarr_url.trim().is_empty() {
        errors.insert("sonarr".to_string(), message);
    }
    UpcomingResult {
        events: Vec::new(),
        errors,
        home: Vec::new(),
    }
}

/// The shared upcoming path. A body that is not an array yields no releases,
/// as the shipping UI has always seen it.
async fn fetch_provider<S, T>(
    store: &S,
    transport: &T,
    provider: IntegrationProvider,
    start: &str,
    end: &str,
    start_instant: DateTime<Utc>,
    end_instant: DateTime<Utc>,
) -> Result<Vec<UpcomingRelease>, IntegrationError>
where
    S: CredentialStore,
    T: Transport,
{
    let body = request_calendar(store, transport, provider, start, end).await?;
    Ok(normalize_in_window(
        provider,
        &body,
        start_instant,
        end_instant,
    ))
}

fn normalize_in_window(
    provider: IntegrationProvider,
    body: &Value,
    start_instant: DateTime<Utc>,
    end_instant: DateTime<Utc>,
) -> Vec<UpcomingRelease> {
    let mut events = normalize_calendar(provider, body, start_instant, end_instant);
    events.retain(|event| in_window(event.instant, start_instant, end_instant));
    events
}

/// `GET /api/v3/calendar` for one provider. The key is a header. The range
/// goes in the query exactly as the caller wrote it.
async fn request_calendar<S, T>(
    store: &S,
    transport: &T,
    provider: IntegrationProvider,
    start: &str,
    end: &str,
) -> Result<Value, IntegrationError>
where
    S: CredentialStore,
    T: Transport,
{
    let stored =
        load_stored(store, provider)?.ok_or(IntegrationError::NotConfigured { provider })?;
    if stored.api_key.trim().is_empty() || stored.server_url.trim().is_empty() {
        return Err(IntegrationError::NotConfigured { provider });
    }
    let server_url = normalize_server_url(&stored.server_url)?;
    let mut query = vec![
        ("start".to_string(), start.to_string()),
        ("end".to_string(), end.to_string()),
        ("unmonitored".to_string(), "false".to_string()),
    ];
    if provider == IntegrationProvider::Sonarr {
        query.push(("includeSeries".to_string(), "true".to_string()));
        query.push(("includeEpisodeImages".to_string(), "true".to_string()));
    }
    let response = send(
        transport,
        provider,
        format!("{server_url}/api/v3/calendar"),
        &stored.api_key,
        query,
    )
    .await?;
    decode_json(provider, response.status, &response.body)
}

async fn send<T: Transport>(
    transport: &T,
    provider: IntegrationProvider,
    url: String,
    api_key: &str,
    query: Vec<(String, String)>,
) -> Result<crate::transport::IntegrationResponse, IntegrationError> {
    transport
        .send(IntegrationRequest {
            method: "GET",
            url,
            headers: vec![("X-Api-Key".to_string(), api_key.to_string())],
            query,
        })
        .await
        .map_err(|error| IntegrationError::unreachable(provider, error.detail))
}

fn decode_json(
    provider: IntegrationProvider,
    status: u16,
    body: &[u8],
) -> Result<Value, IntegrationError> {
    if !(200..300).contains(&status) {
        return Err(IntegrationError::server(provider, status));
    }
    serde_json::from_slice(body).map_err(|_| IntegrationError::MalformedResponse { provider })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredIntegration {
    server_url: String,
    api_key: String,
}

impl Drop for StoredIntegration {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}

fn credential_name(
    provider: IntegrationProvider,
) -> Result<(CredentialNamespace, CredentialKey), IntegrationError> {
    Ok((
        CredentialNamespace::media_integrations(),
        CredentialKey::new(provider.as_str())?,
    ))
}

fn load_stored<S: CredentialStore>(
    store: &S,
    provider: IntegrationProvider,
) -> Result<Option<StoredIntegration>, IntegrationError> {
    let (namespace, key) = credential_name(provider)?;
    let Some(secret) = store.get(&namespace, &key)? else {
        return Ok(None);
    };
    let mut raw = secret.expose().to_string();
    let parsed = serde_json::from_str::<StoredIntegration>(&raw);
    raw.zeroize();
    match parsed {
        Ok(stored) => Ok(Some(stored)),
        Err(_) => Err(IntegrationError::CorruptCredential { provider }),
    }
}

fn save_stored<S: CredentialStore>(
    store: &S,
    provider: IntegrationProvider,
    server_url: &str,
    api_key: &str,
) -> Result<(), IntegrationError> {
    let (namespace, key) = credential_name(provider)?;
    let payload = serde_json::json!({
        "serverUrl": server_url,
        "apiKey": api_key,
    });
    let encoded = serde_json::to_string(&payload)
        .map_err(|_| IntegrationError::CorruptCredential { provider })?;
    store.set(&namespace, &key, &Secret::new(encoded))?;
    Ok(())
}

fn lock(cache: &Mutex<CacheInner>) -> std::sync::MutexGuard<'_, CacheInner> {
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
