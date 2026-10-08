//! The Calendar's HTTP boundary, against a loopback server that stands in for
//! Radarr or Sonarr. These run the real transport, the real vault, and the
//! real request path, and check what crosses the wire: the path, the range,
//! the key (in a header only), and how each failure comes back to the model.

use std::sync::Arc;

use chrono::NaiveDate;
use matinee_integrations::IntegrationProvider;
use matinee_secrets::{CredentialKey, CredentialNamespace, CredentialStore, MemoryStore, Secret};

use super::grid::Window;
use super::load::{self, CalendarService};
use super::model::{Request, Response, SourceFailure, Ticket};
use crate::runtime::ServiceRuntime;
use crate::store::SharedStore;
use crate::test_support::{Reply, fake_jellyfin, integrations_transport};

const KEY: &str = "radarr-key-42";

fn runtime() -> ServiceRuntime {
    ServiceRuntime::new().expect("runtime")
}

/// A vault with the given provider connections, saved the way the shipping
/// app saves them: one JSON payload per provider.
fn vault(connections: &[(&str, &str, &str)]) -> SharedStore {
    let store = MemoryStore::new();
    for (provider, url, key) in connections {
        let payload = format!(r#"{{"serverUrl":"{url}","apiKey":"{key}"}}"#);
        store
            .set(
                &CredentialNamespace::media_integrations(),
                &CredentialKey::new(*provider).expect("provider key"),
                &Secret::new(payload),
            )
            .expect("seed");
    }
    SharedStore::new(store)
}

fn service(store: SharedStore) -> Arc<CalendarService> {
    Arc::new(CalendarService::new(store, integrations_transport()))
}

fn answer(runtime: &ServiceRuntime, service: &Arc<CalendarService>, request: Request) -> Response {
    let (_task, receiver) = load::spawn(runtime, service, request);
    receiver.blocking_recv().expect("the request answered")
}

fn august() -> Window {
    Window::for_month(NaiveDate::from_ymd_opt(2026, 8, 1).expect("date"))
}

fn releases(provider: IntegrationProvider, window: Window) -> Request {
    Request::Releases {
        ticket: Ticket::for_test(7),
        provider,
        window,
    }
}

fn outcome(
    response: Response,
) -> Result<Vec<matinee_integrations::UpcomingRelease>, SourceFailure> {
    match response {
        Response::Releases { result, .. } => result,
        other => panic!("expected releases, got {other:?}"),
    }
}

/// The request line and lower-cased headers of a request the server saw.
fn parts(request: &str) -> (String, Vec<(String, String)>) {
    let mut lines = request.split("\r\n");
    let line = lines.next().unwrap_or_default().to_string();
    let headers = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect();
    (line, headers)
}

fn calendar_body() -> Vec<u8> {
    br#"[{"id":7,"title":"Future Feature","monitored":true,"hasFile":false,"inCinemas":"2026-08-20T00:00:00Z"}]"#
        .to_vec()
}

#[test]
fn a_connection_check_reads_the_vault_and_answers_connected() {
    let runtime = runtime();
    let service = service(vault(&[("radarr", "http://127.0.0.1:1", KEY)]));
    let response = answer(
        &runtime,
        &service,
        Request::Link {
            ticket: Ticket::for_test(1),
            provider: IntegrationProvider::Radarr,
        },
    );
    assert!(matches!(
        response,
        Response::Link {
            result: Ok(true),
            provider: IntegrationProvider::Radarr,
            ..
        }
    ));
}

#[test]
fn a_source_with_no_saved_connection_answers_not_connected_and_asks_nothing() {
    let runtime = runtime();
    let service = service(vault(&[]));
    let link = answer(
        &runtime,
        &service,
        Request::Link {
            ticket: Ticket::for_test(1),
            provider: IntegrationProvider::Sonarr,
        },
    );
    assert!(matches!(
        link,
        Response::Link {
            result: Ok(false),
            ..
        }
    ));
    let months = outcome(answer(
        &runtime,
        &service,
        releases(IntegrationProvider::Sonarr, august()),
    ));
    assert_eq!(months.unwrap_err(), SourceFailure::Unlinked);
}

#[test]
fn a_month_request_asks_for_the_grid_plus_a_day_each_side_with_the_key_in_a_header() {
    let (address, seen) = fake_jellyfin(vec![Reply::Status(200, b"[]".to_vec())]);
    let runtime = runtime();
    let service = service(vault(&[("radarr", &address, KEY)]));
    let result = outcome(answer(
        &runtime,
        &service,
        releases(IntegrationProvider::Radarr, august()),
    ));
    assert_eq!(result.expect("an empty calendar"), Vec::new());

    let request = seen.recv().expect("the server saw the request");
    let (line, headers) = parts(&request);
    // The query is percent-encoded on the wire; decode the colons to compare.
    let line = line.replace("%3A", ":");
    assert!(line.starts_with("GET /api/v3/calendar?"), "{line}");
    assert!(
        line.contains("start=2026-07-25T00:00:00Z"),
        "the grid starts on Sunday 26 July, padded to the 25th: {line}"
    );
    assert!(
        line.contains("end=2026-09-07T00:00:00Z"),
        "the grid ends on Saturday 5 September, padded to the 7th: {line}"
    );
    assert!(line.contains("unmonitored=false"), "{line}");
    assert!(
        !line.contains("includeSeries"),
        "Radarr is not asked for series: {line}"
    );
    assert!(!line.contains(KEY), "the key is never in the address");
    assert!(
        headers
            .iter()
            .any(|(name, value)| name == "x-api-key" && value == KEY),
        "the key is sent as the X-Api-Key header: {headers:?}"
    );
}

#[test]
fn a_sonarr_month_asks_for_series_and_episode_images() {
    let (address, seen) = fake_jellyfin(vec![Reply::Status(200, b"[]".to_vec())]);
    let runtime = runtime();
    let service = service(vault(&[("sonarr", &address, "sonarr-key-42")]));
    let _ = answer(
        &runtime,
        &service,
        releases(IntegrationProvider::Sonarr, august()),
    );
    let (line, _) = parts(&seen.recv().expect("request"));
    assert!(line.contains("includeSeries=true"), "{line}");
    assert!(line.contains("includeEpisodeImages=true"), "{line}");
}

#[test]
fn a_calendar_answer_comes_back_as_releases_in_the_order_it_was_given() {
    let (address, _seen) = fake_jellyfin(vec![Reply::Status(200, calendar_body())]);
    let runtime = runtime();
    let service = service(vault(&[("radarr", &address, KEY)]));
    let result = outcome(answer(
        &runtime,
        &service,
        releases(IntegrationProvider::Radarr, august()),
    ))
    .expect("releases");
    assert_eq!(
        result
            .iter()
            .map(|release| release.id.as_str())
            .collect::<Vec<_>>(),
        vec!["radarr-7-theatrical"]
    );
}

#[test]
fn a_rejected_key_comes_back_as_unauthorized() {
    let (address, _seen) = fake_jellyfin(vec![Reply::Status(401, b"{}".to_vec())]);
    let runtime = runtime();
    let service = service(vault(&[("radarr", &address, KEY)]));
    let failure = outcome(answer(
        &runtime,
        &service,
        releases(IntegrationProvider::Radarr, august()),
    ))
    .expect_err("401");
    assert_eq!(failure, SourceFailure::Unauthorized);
}

#[test]
fn a_server_error_comes_back_as_unavailable() {
    let (address, _seen) = fake_jellyfin(vec![Reply::Status(503, Vec::new())]);
    let runtime = runtime();
    let service = service(vault(&[("radarr", &address, KEY)]));
    let failure = outcome(answer(
        &runtime,
        &service,
        releases(IntegrationProvider::Radarr, august()),
    ))
    .expect_err("503");
    assert_eq!(failure, SourceFailure::Unavailable);
}

#[test]
fn a_page_that_is_not_a_calendar_comes_back_as_malformed() {
    let (address, _seen) = fake_jellyfin(vec![Reply::Status(200, b"<html>login</html>".to_vec())]);
    let runtime = runtime();
    let service = service(vault(&[("radarr", &address, KEY)]));
    let failure = outcome(answer(
        &runtime,
        &service,
        releases(IntegrationProvider::Radarr, august()),
    ))
    .expect_err("html");
    assert_eq!(failure, SourceFailure::Malformed);
}

#[test]
fn a_server_that_is_not_listening_comes_back_as_unavailable() {
    // Bind, learn the port, and let the listener go: nothing answers there.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.local_addr().expect("address").port()
    };
    let runtime = runtime();
    let service = service(vault(&[(
        "radarr",
        &format!("http://127.0.0.1:{port}"),
        KEY,
    )]));
    let failure = outcome(answer(
        &runtime,
        &service,
        releases(IntegrationProvider::Radarr, august()),
    ))
    .expect_err("refused");
    assert_eq!(failure, SourceFailure::Unavailable);
}

#[test]
fn an_abandoned_request_never_answers_so_it_cannot_apply() {
    let (address, _seen) = fake_jellyfin(vec![Reply::Hang]);
    let runtime = runtime();
    let service = service(vault(&[("radarr", &address, KEY)]));
    let (task, receiver) = load::spawn(
        &runtime,
        &service,
        releases(IntegrationProvider::Radarr, august()),
    );
    // The screen aborts a request the model no longer waits on.
    std::thread::sleep(std::time::Duration::from_millis(50));
    task.abort();
    assert!(
        receiver.blocking_recv().is_err(),
        "the receiver closes with the task, so no late answer can land"
    );
}
