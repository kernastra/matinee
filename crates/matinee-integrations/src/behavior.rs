use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::channel::oneshot;
use futures::executor::{LocalPool, block_on};
use futures::task::LocalSpawnExt;
use matinee_secrets::{CredentialKey, CredentialNamespace, CredentialStore, MemoryStore, Secret};
use serde_json::{Value, json};

use crate::provider::IntegrationProvider;
use crate::service::{Clock, Integrations};
use crate::transport::{IntegrationRequest, IntegrationResponse, Transport, TransportError};
use crate::{IntegrationError, UpcomingQuery};

type ScriptHandler =
    Box<dyn Fn(&IntegrationRequest) -> Result<IntegrationResponse, TransportError> + Send>;

pub(super) struct Script {
    hits: AtomicUsize,
    handler: Mutex<ScriptHandler>,
}

impl Script {
    pub(super) fn new<F>(handler: F) -> Self
    where
        F: Fn(&IntegrationRequest) -> Result<IntegrationResponse, TransportError> + Send + 'static,
    {
        Self {
            hits: AtomicUsize::new(0),
            handler: Mutex::new(Box::new(handler)),
        }
    }
}

impl Transport for Script {
    async fn send(
        &self,
        request: IntegrationRequest,
    ) -> Result<IntegrationResponse, TransportError> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        let handler = self
            .handler
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        handler(&request)
    }
}

pub(super) fn json_response(status: u16, body: Value) -> IntegrationResponse {
    IntegrationResponse {
        status,
        body: body.to_string().into_bytes(),
    }
}

pub(super) fn status_body(request: &IntegrationRequest) -> Option<IntegrationResponse> {
    if request.url.contains("/system/status") {
        let app = if request.url.contains("8989") {
            "Sonarr"
        } else {
            "Radarr"
        };
        Some(json_response(
            200,
            json!({"appName": app, "version": "5.14.0"}),
        ))
    } else {
        None
    }
}

#[derive(Clone)]
struct ManualClock {
    base: Instant,
    extra_ms: Arc<AtomicUsize>,
}

impl ManualClock {
    fn new() -> Self {
        Self {
            base: Instant::now(),
            extra_ms: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn advance(&self, duration: Duration) {
        self.extra_ms
            .fetch_add(duration.as_millis() as usize, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.base + Duration::from_millis(self.extra_ms.load(Ordering::SeqCst) as u64)
    }
}

#[test]
fn connection_test_stores_the_shipping_credential_shape() {
    let script = Script::new(|request| {
        let rendered = format!("{request:?}");
        assert!(rendered.contains("[redacted]"));
        assert!(!rendered.contains("radarr-key-123"));
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name == "X-Api-Key" && value == "radarr-key-123")
        );
        assert!(request.url.ends_with("/api/v3/system/status"));
        Ok(json_response(
            200,
            json!({"appName": "Radarr", "version": "5.14.0"}),
        ))
    });
    let integrations = Integrations::new(MemoryStore::new(), script);
    let connection = block_on(integrations.test_and_save(
        IntegrationProvider::Radarr,
        "http://192.168.1.20:7878/",
        Some(Secret::new("  radarr-key-123  ")),
    ))
    .unwrap();
    assert_eq!(connection.server_url, "http://192.168.1.20:7878");
    assert_eq!(connection.version.as_deref(), Some("5.14.0"));
    assert!(
        integrations
            .key_status(IntegrationProvider::Radarr)
            .unwrap()
            .configured
    );

    let stored = integrations
        .credential_store()
        .get(
            &CredentialNamespace::media_integrations(),
            &CredentialKey::new("radarr").unwrap(),
        )
        .unwrap()
        .unwrap();
    let payload: Value = serde_json::from_str(stored.expose()).unwrap();
    assert_eq!(payload["serverUrl"], "http://192.168.1.20:7878");
    assert_eq!(payload["apiKey"], "radarr-key-123");
    assert_eq!(
        CredentialNamespace::media_integrations().as_str(),
        "dev.sean.matinee.media-integrations"
    );

    let error = block_on(integrations.test_and_save(
        IntegrationProvider::Radarr,
        "http://other.local:7878",
        None,
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        IntegrationError::KeyBoundToOtherServer { .. }
    ));
    assert!(error.to_string().contains("different server"));
}

#[test]
fn wrong_provider_and_rejected_key_stay_typed() {
    let script = Script::new(|request| {
        if request.url.contains("wrong") {
            Ok(json_response(
                200,
                json!({"appName": "Sonarr", "version": "4"}),
            ))
        } else {
            Ok(json_response(401, json!("apiKey=radarr-key-123 no")))
        }
    });
    let integrations = Integrations::new(MemoryStore::new(), script);
    let wrong = block_on(integrations.test_and_save(
        IntegrationProvider::Radarr,
        "http://wrong.local:7878",
        Some(Secret::new("radarr-key-123")),
    ))
    .unwrap_err();
    assert!(matches!(wrong, IntegrationError::WrongProvider { .. }));
    let rejected = block_on(integrations.test_and_save(
        IntegrationProvider::Radarr,
        "http://radarr.local:7878",
        Some(Secret::new("radarr-key-123")),
    ))
    .unwrap_err();
    let rendered = rejected.to_string();
    assert!(matches!(
        rejected,
        IntegrationError::AuthenticationRejected { .. }
    ));
    assert!(rendered.contains("HTTP 401"));
    assert!(!rendered.contains("radarr-key-123"));
}

#[test]
fn partial_provider_failure_keeps_the_other_calendar() {
    let script = Script::new(|request| {
        if let Some(response) = status_body(request) {
            return Ok(response);
        }
        if request.url.contains("7878") {
            return Err(TransportError {
                detail: "connection refused".into(),
            });
        }
        assert!(
            request
                .query
                .iter()
                .any(|(key, value)| key == "unmonitored" && value == "false")
        );
        assert!(request.query.iter().any(|(key, _)| key == "includeSeries"));
        assert!(
            request
                .query
                .iter()
                .any(|(key, _)| key == "includeEpisodeImages")
        );
        Ok(json_response(
            200,
            json!([{
                "id": 22,
                "title": "The Return",
                "airDateUtc": "2026-08-10T03:00:00Z",
                "monitored": true,
                "seriesId": 4,
                "seasonNumber": 2,
                "episodeNumber": 3,
                "series": { "id": 4, "title": "Night Shift", "monitored": true, "genres": ["Drama"] }
            }]),
        ))
    });
    let integrations = Integrations::new(MemoryStore::new(), script);
    block_on(integrations.test_and_save(
        IntegrationProvider::Radarr,
        "http://192.168.1.20:7878",
        Some(Secret::new("radarr-key-123")),
    ))
    .unwrap();
    block_on(integrations.test_and_save(
        IntegrationProvider::Sonarr,
        "http://127.0.0.1:8989",
        Some(Secret::new("sonarr-key-123")),
    ))
    .unwrap();
    let result = block_on(integrations.upcoming(UpcomingQuery::new(
        "http://192.168.1.20:7878",
        "http://127.0.0.1:8989",
        "2026-08-05T00:00:00Z",
        "2026-12-03T00:00:00Z",
    )));
    assert_eq!(result.events.len(), 1);
    assert_eq!(result.events[0].id, "sonarr-22");
    assert!(result.error_message(IntegrationProvider::Radarr).is_some());
    assert!(result.error_message(IntegrationProvider::Sonarr).is_none());
    assert_eq!(result.home.len(), 1);
}

#[test]
fn cache_collapses_repeats_until_invalidation_or_ttl() {
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&hits);
    let script = Script::new(move |request| {
        if let Some(response) = status_body(request) {
            return Ok(response);
        }
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(json_response(200, json!([])))
    });
    let clock = ManualClock::new();
    let integrations = Integrations::with_clock(
        MemoryStore::new(),
        script,
        clock.clone(),
        Duration::from_secs(300),
    );
    block_on(integrations.test_and_save(
        IntegrationProvider::Sonarr,
        "http://localhost:8989",
        Some(Secret::new("sonarr-key-123")),
    ))
    .unwrap();
    let query = UpcomingQuery::new(
        "",
        "http://localhost:8989",
        "2026-08-05T00:00:00Z",
        "2026-12-03T00:00:00Z",
    );
    let _ = block_on(integrations.upcoming(query.clone()));
    let after_first = hits.load(Ordering::SeqCst);
    let _ = block_on(integrations.upcoming(query.clone()));
    assert_eq!(hits.load(Ordering::SeqCst), after_first);
    integrations.invalidate_cache();
    let _ = block_on(integrations.upcoming(query.clone()));
    assert_eq!(hits.load(Ordering::SeqCst), after_first + 1);
    clock.advance(Duration::from_secs(301));
    let _ = block_on(integrations.upcoming(query));
    assert_eq!(hits.load(Ordering::SeqCst), after_first + 2);
}

#[test]
fn missing_and_removed_credentials_are_not_configured() {
    let script = Script::new(|_| Ok(json_response(200, json!({}))));
    let integrations = Integrations::new(MemoryStore::new(), script);
    assert!(
        !integrations
            .key_status(IntegrationProvider::Radarr)
            .unwrap()
            .configured
    );
    let missing = block_on(integrations.fetch_calendar(
        IntegrationProvider::Radarr,
        "2026-08-05T00:00:00Z",
        "2026-12-03T00:00:00Z",
    ))
    .unwrap_err();
    assert!(matches!(missing, IntegrationError::NotConfigured { .. }));
    block_on(integrations.test_and_save(
        IntegrationProvider::Radarr,
        "http://radarr.local:7878",
        Some(Secret::new("radarr-key-123")),
    ))
    .unwrap_err();
    // The script returns an empty object, so the provider check fails and nothing is saved.
    assert!(
        !integrations
            .key_status(IntegrationProvider::Radarr)
            .unwrap()
            .configured
    );
}

struct Gate {
    hits: Arc<AtomicUsize>,
    receiver: Mutex<Option<oneshot::Receiver<()>>>,
}

impl Transport for Gate {
    async fn send(
        &self,
        request: IntegrationRequest,
    ) -> Result<IntegrationResponse, TransportError> {
        if let Some(response) = status_body(&request) {
            return Ok(response);
        }
        self.hits.fetch_add(1, Ordering::SeqCst);
        let receiver = self
            .receiver
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(receiver) = receiver {
            let _ = receiver.await;
        }
        Ok(json_response(200, json!([])))
    }
}

#[test]
fn concurrent_upcoming_requests_share_one_fetch() {
    let hits = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = oneshot::channel::<()>();
    let integrations = Arc::new(Integrations::new(
        MemoryStore::new(),
        Gate {
            hits: Arc::clone(&hits),
            receiver: Mutex::new(Some(receiver)),
        },
    ));
    block_on(integrations.test_and_save(
        IntegrationProvider::Sonarr,
        "http://localhost:8989",
        Some(Secret::new("sonarr-key-123")),
    ))
    .unwrap();
    let query = UpcomingQuery::new(
        "",
        "http://localhost:8989",
        "2026-08-05T00:00:00Z",
        "2026-12-03T00:00:00Z",
    );
    let left = Arc::clone(&integrations);
    let right = Arc::clone(&integrations);
    let left_query = query.clone();
    let mut pool = LocalPool::new();
    let spawner = pool.spawner();
    spawner
        .spawn_local(async move {
            left.upcoming(left_query).await;
        })
        .unwrap();
    let started = Instant::now();
    while hits.load(Ordering::SeqCst) == 0 {
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "calendar fetch did not start"
        );
        pool.run_until_stalled();
    }
    spawner
        .spawn_local(async move {
            right.upcoming(query).await;
        })
        .unwrap();
    pool.run_until_stalled();
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    sender.send(()).unwrap();
    pool.run();
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn cache_keys_and_invalidation_do_not_reuse_the_wrong_result() {
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&hits);
    let script = Script::new(move |request| {
        if let Some(response) = status_body(request) {
            return Ok(response);
        }
        counted.fetch_add(1, Ordering::SeqCst);
        if request.url.contains("7878") {
            return Err(TransportError {
                detail: "connection refused apikey=echoed-secret".into(),
            });
        }
        Ok(json_response(200, json!([])))
    });
    let clock = ManualClock::new();
    let integrations = Integrations::with_clock(
        MemoryStore::new(),
        script,
        clock.clone(),
        Duration::from_secs(300),
    );
    block_on(integrations.test_and_save(
        IntegrationProvider::Radarr,
        "http://radarr.local:7878",
        Some(Secret::new("radarr-key-123")),
    ))
    .unwrap();
    block_on(integrations.test_and_save(
        IntegrationProvider::Sonarr,
        "http://sonarr.local:8989",
        Some(Secret::new("sonarr-key-123")),
    ))
    .unwrap();
    let query = UpcomingQuery::new(
        "http://radarr.local:7878",
        "http://sonarr.local:8989",
        "2026-08-05T00:00:00Z",
        "2026-12-03T00:00:00Z",
    );
    let first = block_on(integrations.upcoming(query.clone()));
    let after_first = hits.load(Ordering::SeqCst);
    assert!(after_first >= 2);
    assert!(first.error_message(IntegrationProvider::Radarr).is_some());
    assert!(
        !first
            .error_message(IntegrationProvider::Radarr)
            .unwrap()
            .contains("echoed-secret")
    );
    let second = block_on(integrations.upcoming(query.clone()));
    assert_eq!(hits.load(Ordering::SeqCst), after_first);
    assert_eq!(
        second.error_message(IntegrationProvider::Radarr),
        first.error_message(IntegrationProvider::Radarr)
    );

    let other_range = UpcomingQuery::new(
        "http://radarr.local:7878",
        "http://sonarr.local:8989",
        "2026-08-05T00:00:00Z",
        "2026-09-01T00:00:00Z",
    );
    let _ = block_on(integrations.upcoming(other_range));
    assert!(hits.load(Ordering::SeqCst) > after_first);
    let after_range = hits.load(Ordering::SeqCst);

    let other_radarr = UpcomingQuery::new(
        "http://other-radarr.local:7878",
        "http://sonarr.local:8989",
        "2026-08-05T00:00:00Z",
        "2026-12-03T00:00:00Z",
    );
    let _ = block_on(integrations.upcoming(other_radarr));
    assert!(hits.load(Ordering::SeqCst) > after_range);
    let after_radarr = hits.load(Ordering::SeqCst);

    let other_sonarr = UpcomingQuery::new(
        "http://radarr.local:7878",
        "http://other-sonarr.local:8989",
        "2026-08-05T00:00:00Z",
        "2026-12-03T00:00:00Z",
    );
    let _ = block_on(integrations.upcoming(other_sonarr));
    assert!(hits.load(Ordering::SeqCst) > after_radarr);
    let after_sonarr = hits.load(Ordering::SeqCst);

    integrations.remove(IntegrationProvider::Radarr).unwrap();
    let _ = block_on(integrations.upcoming(query.clone()));
    assert!(hits.load(Ordering::SeqCst) > after_sonarr);
    let after_remove = hits.load(Ordering::SeqCst);

    block_on(integrations.test_and_save(
        IntegrationProvider::Sonarr,
        "http://sonarr.local:8989",
        Some(Secret::new("sonarr-key-123")),
    ))
    .unwrap();
    let _ = block_on(integrations.upcoming(query.clone()));
    assert!(hits.load(Ordering::SeqCst) > after_remove);
    let after_save = hits.load(Ordering::SeqCst);

    integrations.invalidate_cache();
    let _ = block_on(integrations.upcoming(query.clone()));
    assert!(hits.load(Ordering::SeqCst) > after_save);
    let after_clear = hits.load(Ordering::SeqCst);

    clock.advance(Duration::from_secs(301));
    let _ = block_on(integrations.upcoming(query));
    assert!(hits.load(Ordering::SeqCst) > after_clear);
}

#[test]
fn a_stale_request_does_not_repopulate_the_cache() {
    let hits = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = oneshot::channel::<()>();
    let integrations = Arc::new(Integrations::new(
        MemoryStore::new(),
        Gate {
            hits: Arc::clone(&hits),
            receiver: Mutex::new(Some(receiver)),
        },
    ));
    block_on(integrations.test_and_save(
        IntegrationProvider::Sonarr,
        "http://localhost:8989",
        Some(Secret::new("sonarr-key-123")),
    ))
    .unwrap();
    let query = UpcomingQuery::new(
        "",
        "http://localhost:8989",
        "2026-08-05T00:00:00Z",
        "2026-12-03T00:00:00Z",
    );
    let mut pool = LocalPool::new();
    let spawner = pool.spawner();
    let waiting = Arc::clone(&integrations);
    let waiting_query = query.clone();
    spawner
        .spawn_local(async move {
            waiting.upcoming(waiting_query).await;
        })
        .unwrap();
    let started = Instant::now();
    while hits.load(Ordering::SeqCst) == 0 {
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "calendar fetch did not start"
        );
        pool.run_until_stalled();
    }
    integrations.invalidate_cache();
    sender.send(()).unwrap();
    pool.run();
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let _ = block_on(integrations.upcoming(query));
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[test]
fn provider_status_errors_do_not_include_response_bodies() {
    let script = Script::new(|request| {
        let status = if request.url.contains("forbidden") {
            403
        } else if request.url.contains("missing") {
            404
        } else if request.url.contains("broken") {
            500
        } else {
            401
        };
        Ok(json_response(
            status,
            json!(
                "<html>api_key=echoed-secret authorization=Bearer bearer-secret password=hunter2 token=abc\u{0001}</html>"
            ),
        ))
    });
    let integrations = Integrations::new(MemoryStore::new(), script);
    for (address, status) in [
        ("http://radarr.local:7878", "401"),
        ("http://forbidden.local:7878", "403"),
        ("http://missing.local:7878", "404"),
        ("http://broken.local:7878", "500"),
    ] {
        let error = block_on(integrations.test_and_save(
            IntegrationProvider::Radarr,
            address,
            Some(Secret::new("radarr-key-123")),
        ))
        .unwrap_err();
        let rendered = format!("{error} {error:?}");
        assert!(rendered.contains(status), "{rendered}");
        for secret in ["echoed-secret", "bearer-secret", "hunter2", "abc"] {
            assert!(!rendered.contains(secret), "{rendered}");
        }
        assert!(!rendered.contains('\u{0001}'));
        assert!(std::error::Error::source(&error).is_none());
    }
    let unreachable = Integrations::new(
        MemoryStore::new(),
        Script::new(|_| {
            Err(TransportError {
                detail: "error for url (http://radarr.local/api?apikey=query-secret)".into(),
            })
        }),
    );
    let error = block_on(unreachable.test_and_save(
        IntegrationProvider::Radarr,
        "http://radarr.local:7878",
        Some(Secret::new("radarr-key-123")),
    ))
    .unwrap_err();
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains("query-secret"));
    assert!(rendered.contains("could not reach"));
}
