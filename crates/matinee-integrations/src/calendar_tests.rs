//! The native calendar contract, on a scripted transport: the typed
//! per-provider fetch, what a request carries, how each failure is typed, the
//! timing facts the app places days from, and the artwork fetch.
//!
//! No socket opens here. Every request is recorded, so the query and headers
//! are inspected directly.

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use matinee_secrets::{MemoryStore, Secret};
use serde_json::json;

use crate::behavior::{Script, json_response, status_body};
use crate::service::Integrations;
use crate::transport::{IntegrationRequest, IntegrationResponse, TransportError};
use crate::{IntegrationError, IntegrationProvider, ReleaseTiming, UpcomingQuery};

const RADARR: &str = "http://192.168.1.20:7878";
const SONARR: &str = "http://127.0.0.1:8989";
const RADARR_KEY: &str = "radarr-key-123";
const SONARR_KEY: &str = "sonarr-key-123";
const START: &str = "2026-08-05T00:00:00Z";
const END: &str = "2026-12-03T00:00:00Z";

type Log = Arc<Mutex<Vec<IntegrationRequest>>>;
type Answer = Result<IntegrationResponse, TransportError>;

/// Answers the connection probe, records every calendar request, and then
/// answers it with `calendar`.
fn scripted(
    log: &Log,
    calendar: impl Fn(&IntegrationRequest) -> Answer + Send + 'static,
) -> Script {
    let log = Arc::clone(log);
    Script::new(move |request| {
        if let Some(response) = status_body(request) {
            return Ok(response);
        }
        log.lock().unwrap().push(request.clone());
        calendar(request)
    })
}

/// Radarr connected at [`RADARR`] and Sonarr at [`SONARR`], both with keys.
fn connected(
    log: &Log,
    calendar: impl Fn(&IntegrationRequest) -> Answer + Send + 'static,
) -> Integrations<MemoryStore, Script> {
    let integrations = Integrations::new(MemoryStore::new(), scripted(log, calendar));
    block_on(integrations.test_and_save(
        IntegrationProvider::Radarr,
        RADARR,
        Some(Secret::new(RADARR_KEY)),
    ))
    .unwrap();
    block_on(integrations.test_and_save(
        IntegrationProvider::Sonarr,
        SONARR,
        Some(Secret::new(SONARR_KEY)),
    ))
    .unwrap();
    integrations
}

fn body(status: u16, value: serde_json::Value) -> Answer {
    Ok(json_response(status, value))
}

fn raw(status: u16, text: &str) -> Answer {
    Ok(IntegrationResponse {
        status,
        body: text.as_bytes().to_vec(),
    })
}

fn query_value<'a>(request: &'a IntegrationRequest, key: &str) -> Option<&'a str> {
    request
        .query
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn calendar_requests(log: &Log) -> Vec<IntegrationRequest> {
    log.lock().unwrap().clone()
}

fn movie_body() -> serde_json::Value {
    json!([{
        "id": 7,
        "title": "Future Feature",
        "monitored": true,
        "hasFile": false,
        "inCinemas": "2026-08-20T00:00:00Z",
        "digitalRelease": "2026-09-12T00:00:00Z"
    }])
}

#[test]
fn radarr_calendar_sends_the_range_unchanged_and_the_key_only_in_a_header() {
    let log = Log::default();
    let integrations = connected(&log, |_| body(200, movie_body()));
    let events = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, START, END))
        .expect("calendar");
    assert_eq!(events.len(), 2);

    let requests = calendar_requests(&log);
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "GET");
    assert_eq!(request.url, format!("{RADARR}/api/v3/calendar"));
    assert!(!request.url.contains(RADARR_KEY), "no key in the address");
    assert_eq!(query_value(request, "start"), Some(START));
    assert_eq!(query_value(request, "end"), Some(END));
    assert_eq!(query_value(request, "unmonitored"), Some("false"));
    assert_eq!(query_value(request, "includeSeries"), None);
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| name == "X-Api-Key" && value == RADARR_KEY)
    );
    assert!(
        !format!("{request:?}").contains(RADARR_KEY),
        "debug is redacted"
    );
    assert_eq!(
        request.max_body, None,
        "a calendar body is read as the shipping app has always read it"
    );
}

#[test]
fn sonarr_calendar_asks_for_series_and_episode_images_with_the_same_range() {
    let log = Log::default();
    let integrations = connected(&log, |_| body(200, json!([])));
    let events = block_on(integrations.fetch_calendar(IntegrationProvider::Sonarr, START, END))
        .expect("calendar");
    assert!(events.is_empty(), "an empty calendar is not an error");

    let requests = calendar_requests(&log);
    assert_eq!(requests[0].url, format!("{SONARR}/api/v3/calendar"));
    assert_eq!(query_value(&requests[0], "start"), Some(START));
    assert_eq!(query_value(&requests[0], "end"), Some(END));
    assert_eq!(query_value(&requests[0], "includeSeries"), Some("true"));
    assert_eq!(
        query_value(&requests[0], "includeEpisodeImages"),
        Some("true")
    );
    assert!(
        requests[0]
            .headers
            .iter()
            .any(|(name, value)| name == "X-Api-Key" && value == SONARR_KEY),
        "Sonarr's key goes to Sonarr only"
    );
}

#[test]
fn an_unauthorized_provider_is_typed_and_never_names_a_key() {
    let log = Log::default();
    let integrations = connected(&log, |_| body(401, json!({"message": "Unauthorized"})));
    let error = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, START, END))
        .expect_err("401");
    assert!(matches!(
        error,
        IntegrationError::AuthenticationRejected {
            provider: IntegrationProvider::Radarr,
            ..
        }
    ));
    let message = error.to_string();
    assert_eq!(message, "radarr returned HTTP 401.");
    assert!(!message.contains(RADARR_KEY));
}

#[test]
fn a_server_failure_keeps_its_status_and_provider() {
    let log = Log::default();
    let integrations = connected(&log, |_| body(503, json!({})));
    let error = block_on(integrations.fetch_calendar(IntegrationProvider::Sonarr, START, END))
        .expect_err("503");
    assert!(matches!(
        error,
        IntegrationError::Server {
            provider: IntegrationProvider::Sonarr,
            status: 503,
            ..
        }
    ));
    assert_eq!(error.to_string(), "sonarr returned HTTP 503.");
}

#[test]
fn a_timeout_or_refused_connection_is_unreachable_with_the_key_redacted() {
    let log = Log::default();
    let integrations = connected(&log, |_| {
        Err(TransportError {
            detail: format!(
                "error sending request for url ({RADARR}/api/v3/calendar?apikey={RADARR_KEY}): operation timed out"
            ),
        })
    });
    let error = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, START, END))
        .expect_err("timeout");
    assert!(matches!(
        error,
        IntegrationError::Unreachable {
            provider: IntegrationProvider::Radarr,
            ..
        }
    ));
    let message = error.to_string();
    assert!(message.starts_with("Matinee could not reach radarr:"));
    assert!(
        !message.contains(RADARR_KEY),
        "the key never reaches a message"
    );
}

#[test]
fn a_body_that_is_not_json_is_malformed_not_an_empty_month() {
    let log = Log::default();
    let integrations = connected(&log, |_| raw(200, "<html>proxy login</html>"));
    let error = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, START, END))
        .expect_err("html");
    assert!(matches!(
        error,
        IntegrationError::MalformedResponse {
            provider: IntegrationProvider::Radarr
        }
    ));
    assert_eq!(error.to_string(), "radarr returned an unreadable response.");
}

#[test]
fn a_json_object_where_a_calendar_belongs_is_malformed_for_the_native_path() {
    let log = Log::default();
    let integrations = connected(&log, |_| body(200, json!({"records": []})));
    let error = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, START, END))
        .expect_err("object");
    assert!(matches!(error, IntegrationError::MalformedResponse { .. }));
}

#[test]
fn the_shipping_upcoming_path_still_reads_that_object_as_no_releases() {
    let log = Log::default();
    let integrations = connected(&log, |_| body(200, json!({"records": []})));
    let result = block_on(integrations.upcoming(UpcomingQuery::new(RADARR, SONARR, START, END)));
    assert!(result.events.is_empty());
    assert!(result.errors.is_empty(), "shipping shows no error here");
}

#[test]
fn an_unconfigured_provider_is_reported_without_a_request() {
    let log = Log::default();
    let integrations =
        Integrations::new(MemoryStore::new(), scripted(&log, |_| body(200, json!([]))));
    let error = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, START, END))
        .expect_err("no key");
    assert!(matches!(
        error,
        IntegrationError::NotConfigured {
            provider: IntegrationProvider::Radarr
        }
    ));
    assert!(calendar_requests(&log).is_empty());
}

#[test]
fn an_invalid_window_is_rejected_before_any_request() {
    let log = Log::default();
    let integrations = connected(&log, |_| body(200, json!([])));
    for (start, end) in [
        (END, START),
        (START, START),
        ("not-a-time", END),
        (START, ""),
    ] {
        let error = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, start, end))
            .expect_err("window");
        assert!(
            matches!(error, IntegrationError::InvalidWindow),
            "{start} {end}"
        );
    }
    assert!(calendar_requests(&log).is_empty());
}

#[test]
fn rows_the_contract_cannot_read_are_skipped_and_the_rest_arrive() {
    let log = Log::default();
    let integrations = connected(&log, |_| {
        body(
            200,
            json!([
                { "id": 1, "title": "Broken", "inCinemas": "not-a-date" },
                "not an object",
                { "id": 2, "title": "Good", "inCinemas": "2026-08-20T00:00:00Z" }
            ]),
        )
    });
    let events = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, START, END))
        .expect("partial body");
    assert_eq!(
        events
            .iter()
            .map(|event| event.id.as_str())
            .collect::<Vec<_>>(),
        vec!["radarr-2-theatrical"]
    );
}

#[test]
fn results_are_ordered_by_instant_then_id_whatever_the_server_sent() {
    let log = Log::default();
    let integrations = connected(&log, |_| {
        body(
            200,
            json!([
                { "id": 9, "title": "Late", "inCinemas": "2026-09-01T00:00:00Z" },
                { "id": 5, "title": "Same day B", "inCinemas": "2026-08-20T00:00:00Z" },
                { "id": 3, "title": "Same day A", "inCinemas": "2026-08-20T00:00:00Z" }
            ]),
        )
    });
    let events = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, START, END))
        .expect("calendar");
    assert_eq!(
        events
            .iter()
            .map(|event| event.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "radarr-3-theatrical",
            "radarr-5-theatrical",
            "radarr-9-theatrical"
        ]
    );
}

#[test]
fn radarr_dates_are_civil_days_and_sonarr_air_times_are_instants() {
    let log = Log::default();
    let integrations = connected(&log, |request| {
        if request.url.ends_with("/api/v3/calendar") && request.url.contains("8989") {
            body(
                200,
                json!([
                    {
                        "id": 22,
                        "title": "The Return",
                        "airDateUtc": "2026-08-10T03:00:00Z",
                        "monitored": true,
                        "seriesId": 4,
                        "series": { "id": 4, "title": "Night Shift", "monitored": true }
                    },
                    {
                        "id": 23,
                        "airDate": "2026-08-16",
                        "monitored": true,
                        "seriesId": 4,
                        "series": { "id": 4, "title": "Night Shift", "monitored": true }
                    },
                    {
                        "id": 24,
                        "airDateUtc": "not-a-date",
                        "airDate": "2026-08-17",
                        "monitored": true,
                        "series": { "id": 4, "title": "Night Shift", "monitored": true }
                    }
                ]),
            )
        } else {
            body(200, movie_body())
        }
    });
    let movies = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, START, END))
        .expect("movies");
    assert!(
        movies
            .iter()
            .all(|event| event.timing == ReleaseTiming::CivilDay)
    );
    assert_eq!(
        movies[0].civil_day().map(|day| day.to_string()),
        Some("2026-08-20".to_string())
    );

    let episodes = block_on(integrations.fetch_calendar(IntegrationProvider::Sonarr, START, END))
        .expect("episodes");
    // A present but malformed airDateUtc never falls back to airDate.
    assert_eq!(
        episodes
            .iter()
            .map(|event| event.id.as_str())
            .collect::<Vec<_>>(),
        vec!["sonarr-22", "sonarr-23"],
        "sorted by instant; the 17th is dropped, not moved to airDate"
    );
    let instant = &episodes[0];
    let date_only = &episodes[1];
    assert_eq!(date_only.timing, ReleaseTiming::CivilDay);
    assert_eq!(
        date_only.civil_day().map(|day| day.to_string()),
        Some("2026-08-16".to_string())
    );
    assert_eq!(instant.timing, ReleaseTiming::Instant);
    assert_eq!(
        instant.civil_day(),
        None,
        "an instant names no day by itself"
    );
}

#[test]
fn a_civil_day_ignores_the_time_and_zone_the_source_wrote() {
    let log = Log::default();
    let integrations = connected(&log, |_| {
        body(
            200,
            json!([{ "id": 4, "title": "Late Zone", "inCinemas": "2026-08-20T23:30:00-04:00" }]),
        )
    });
    let events = block_on(integrations.fetch_calendar(IntegrationProvider::Radarr, START, END))
        .expect("calendar");
    assert_eq!(
        events[0].civil_day().map(|day| day.to_string()),
        Some("2026-08-20".to_string()),
        "the written day, not its UTC conversion"
    );
}

#[test]
fn artwork_is_fetched_over_http_or_https_only_and_never_with_a_key() {
    let log = Log::default();
    let image_log: Log = Arc::default();
    let image_sink = Arc::clone(&image_log);
    let integrations = connected(&log, move |request| {
        image_sink.lock().unwrap().push(request.clone());
        Ok(IntegrationResponse {
            status: 200,
            body: vec![1, 2, 3],
        })
    });
    for address in [
        "file:///etc/passwd",
        "ftp://images.example/poster.jpg",
        "https://user:pass@images.example/poster.jpg",
        "not a url",
    ] {
        let error = block_on(integrations.fetch_image(IntegrationProvider::Radarr, address))
            .expect_err(address);
        assert!(
            matches!(error, IntegrationError::InvalidImage { .. }),
            "{address}"
        );
    }
    assert!(
        image_log.lock().unwrap().is_empty(),
        "no request for a bad address"
    );

    let bytes = block_on(integrations.fetch_image(
        IntegrationProvider::Radarr,
        "https://image.example/poster.jpg",
    ))
    .expect("image");
    assert_eq!(bytes, vec![1, 2, 3]);
    let images = image_log.lock().unwrap();
    assert_eq!(images.len(), 1);
    assert!(images[0].headers.is_empty(), "no X-Api-Key to a cover host");
    assert!(images[0].query.is_empty());
    assert!(!format!("{:?}", images[0]).contains(RADARR_KEY));
    assert_eq!(
        images[0].max_body,
        Some(16 * 1024 * 1024),
        "the transport caps a cover's body while it streams"
    );
}

/// Fetch one artwork address through a transport that answers `answer`.
fn image_with(answer: impl Fn() -> Answer + Send + 'static) -> Result<Vec<u8>, IntegrationError> {
    let integrations = Integrations::new(MemoryStore::new(), Script::new(move |_| answer()));
    block_on(integrations.fetch_image(
        IntegrationProvider::Sonarr,
        "https://image.example/poster.jpg",
    ))
}

#[test]
fn artwork_failures_are_typed_by_their_cause() {
    let missing = image_with(|| raw(404, "")).expect_err("404");
    assert!(matches!(
        missing,
        IntegrationError::ImageUnavailable {
            provider: IntegrationProvider::Sonarr,
            status: 404
        }
    ));
    assert_eq!(missing.to_string(), "sonarr artwork returned HTTP 404.");

    let timed_out = image_with(|| {
        Err(TransportError {
            detail: "operation timed out".into(),
        })
    })
    .expect_err("timeout");
    assert!(matches!(timed_out, IntegrationError::Unreachable { .. }));

    let refused = image_with(|| Err(TransportError::body_too_large())).expect_err("refused");
    assert!(
        matches!(refused, IntegrationError::InvalidImage { .. }),
        "a body the transport refused for its size is an invalid image, not an outage"
    );

    let oversized = image_with(|| {
        Ok(IntegrationResponse {
            status: 200,
            body: vec![0; 16 * 1024 * 1024 + 1],
        })
    })
    .expect_err("oversized");
    assert!(matches!(oversized, IntegrationError::InvalidImage { .. }));

    let at_the_limit = image_with(|| {
        Ok(IntegrationResponse {
            status: 200,
            body: vec![0; 16 * 1024 * 1024],
        })
    })
    .expect("the limit itself is accepted");
    assert_eq!(at_the_limit.len(), 16 * 1024 * 1024);
}

#[test]
fn artwork_is_never_fetched_from_this_machine_or_a_private_network() {
    // Cover addresses come from the server's JSON. A hostile or confused
    // server must not turn Matinee into a client for local services.
    let image_log: Log = Arc::default();
    let sink = Arc::clone(&image_log);
    let integrations = Integrations::new(
        MemoryStore::new(),
        Script::new(move |request| {
            sink.lock().unwrap().push(request.clone());
            Ok(IntegrationResponse {
                status: 200,
                body: vec![1, 2, 3],
            })
        }),
    );
    for address in [
        "http://localhost/poster.jpg",
        "http://LOCALHOST./poster.jpg",
        "http://images.localhost/poster.jpg",
        "http://127.0.0.1:7878/MediaCover/1/poster.jpg",
        "http://127.1.2.3/poster.jpg",
        "http://10.0.0.5/poster.jpg",
        "http://172.16.4.4/poster.jpg",
        "http://192.168.1.20:7878/poster.jpg",
        "http://169.254.169.254/latest/meta-data",
        "http://100.100.100.100/poster.jpg",
        "http://0.0.0.0/poster.jpg",
        "http://255.255.255.255/poster.jpg",
        "http://224.0.0.1/poster.jpg",
        "http://[::1]/poster.jpg",
        "http://[::]/poster.jpg",
        "http://[fe80::1]/poster.jpg",
        "http://[fd00::1]/poster.jpg",
        "http://[::ffff:127.0.0.1]/poster.jpg",
        "http://[::ffff:192.168.1.1]/poster.jpg",
        "http://2130706433/poster.jpg",
        "http://0x7f000001/poster.jpg",
    ] {
        let error = block_on(integrations.fetch_image(IntegrationProvider::Radarr, address))
            .expect_err(address);
        assert!(
            matches!(error, IntegrationError::InvalidImage { .. }),
            "{address}: {error:?}"
        );
    }
    assert!(
        image_log.lock().unwrap().is_empty(),
        "no request left for a local or private host"
    );
    for address in [
        "https://image.tmdb.org/t/p/original/poster.jpg",
        "https://artworks.thetvdb.com/banners/poster.jpg",
        "http://93.184.216.34/poster.jpg",
        "http://[2606:4700::6810:84e5]/poster.jpg",
    ] {
        block_on(integrations.fetch_image(IntegrationProvider::Radarr, address))
            .unwrap_or_else(|error| panic!("{address}: {error:?}"));
    }
    assert_eq!(
        image_log.lock().unwrap().len(),
        4,
        "public hosts are fetched"
    );
}
