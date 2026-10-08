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

// The transport under hostile answers. These call the real client directly:
// `fetch_image` refuses loopback hosts, so a cover request cannot reach this
// stand-in, but every cover goes through this same `send` with its cap.

/// What a raw loopback server did with one connection.
struct Served {
    /// Body bytes written before the client went away or the plan ended.
    sent: usize,
}

/// Serve one connection: read the request, write `head`, then write the
/// body in 64 KiB chunks until `body` bytes are out or the client hangs up.
/// `chunked` frames the body; otherwise it is written raw after the head.
fn raw_server(
    head: String,
    body: usize,
    chunked: bool,
) -> (String, std::thread::JoinHandle<Served>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = format!("http://{}", listener.local_addr().expect("address"));
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut buffer = [0u8; 4096];
        let _ = stream.read(&mut buffer);
        if stream.write_all(head.as_bytes()).is_err() {
            return Served { sent: 0 };
        }
        let chunk = vec![7u8; 64 * 1024];
        let mut sent = 0;
        while sent < body {
            let length = chunk.len().min(body - sent);
            let written = if chunked {
                stream
                    .write_all(format!("{length:x}\r\n").as_bytes())
                    .and_then(|()| stream.write_all(&chunk[..length]))
                    .and_then(|()| stream.write_all(b"\r\n"))
            } else {
                stream.write_all(&chunk[..length])
            };
            if written.is_err() {
                break;
            }
            sent += length;
        }
        if chunked && sent == body {
            let _ = stream.write_all(b"0\r\n\r\n");
        }
        Served { sent }
    });
    (address, handle)
}

fn get(
    runtime: &ServiceRuntime,
    url: String,
    headers: Vec<(String, String)>,
    max_body: Option<usize>,
) -> Result<matinee_integrations::IntegrationResponse, matinee_integrations::TransportError> {
    use matinee_integrations::{IntegrationRequest, Transport};
    let transport = integrations_transport();
    let (_task, receiver) = runtime.spawn(async move {
        transport
            .send(IntegrationRequest {
                method: "GET",
                url,
                headers,
                query: Vec::new(),
                max_body,
            })
            .await
    });
    receiver.blocking_recv().expect("the request answered")
}

const CAP: usize = 1024 * 1024;

#[test]
fn an_announced_body_over_the_cap_is_refused_before_it_is_read() {
    let runtime = runtime();
    // A gigabyte is announced and then trickles out.
    let (address, server) = raw_server(
        "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: 1073741824\r\n\r\n"
            .to_string(),
        32 * CAP,
        false,
    );
    let started = std::time::Instant::now();
    let error = get(
        &runtime,
        format!("{address}/poster.jpg"),
        Vec::new(),
        Some(CAP),
    )
    .expect_err("refused");
    assert!(error.is_body_too_large(), "{error:?}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "refused at once, not at the 20 second timeout"
    );
    let served = server.join().expect("server");
    assert!(
        served.sent < 32 * CAP,
        "the client hung up instead of reading the body ({} bytes sent)",
        served.sent
    );
}

#[test]
fn a_streamed_body_is_dropped_where_it_crosses_the_cap() {
    let runtime = runtime();
    // No length: the body streams until the client stops it. 64 MiB is far
    // past the cap, and the original client read all of it before refusing.
    let (address, server) = raw_server(
        "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nTransfer-Encoding: chunked\r\n\r\n"
            .to_string(),
        64 * CAP,
        true,
    );
    let error = get(
        &runtime,
        format!("{address}/poster.jpg"),
        Vec::new(),
        Some(CAP),
    )
    .expect_err("refused");
    assert!(error.is_body_too_large(), "{error:?}");
    let served = server.join().expect("server");
    assert!(
        served.sent < 16 * CAP,
        "the client stopped near the cap ({} bytes sent)",
        served.sent
    );
}

#[test]
fn a_body_within_the_cap_arrives_whole_with_or_without_a_length() {
    let runtime = runtime();
    let (address, server) = raw_server(
        format!("HTTP/1.1 200 OK\r\nContent-Length: {CAP}\r\nConnection: close\r\n\r\n"),
        CAP,
        false,
    );
    let response = get(
        &runtime,
        format!("{address}/poster.jpg"),
        Vec::new(),
        Some(CAP),
    )
    .expect("at the cap");
    assert_eq!(response.status, 200);
    assert_eq!(response.body.len(), CAP);
    server.join().expect("server");

    let (address, server) = raw_server(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_string(),
        CAP - 1,
        true,
    );
    let response = get(
        &runtime,
        format!("{address}/poster.jpg"),
        Vec::new(),
        Some(CAP),
    )
    .expect("under the cap");
    assert_eq!(response.body.len(), CAP - 1);
    server.join().expect("server");
}

#[test]
fn a_redirect_is_answered_as_a_status_and_the_key_never_reaches_its_target() {
    let runtime = runtime();
    // The target would record any request that reached it.
    let target = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    target.set_nonblocking(true).expect("nonblocking");
    let location = format!("http://{}/stolen", target.local_addr().expect("address"));
    let (address, server) = raw_server(
        format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\n\r\n"),
        0,
        false,
    );
    let response = get(
        &runtime,
        format!("{address}/api/v3/calendar"),
        vec![("X-Api-Key".to_string(), KEY.to_string())],
        None,
    )
    .expect("an answer");
    assert_eq!(response.status, 302, "the redirect is the answer");
    server.join().expect("server");
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        target.accept().is_err(),
        "nothing connected to the redirect target"
    );
}

#[test]
fn releases_on_the_first_and_last_moments_of_the_grid_survive_the_request_in_every_zone() {
    // The production path end to end: the range the request sends, the
    // server's answer, the integration's filter, and the model's day rule.
    // An air time at the first local moment of the first grid day and at
    // the last local moment of the last one, and Radarr days on both.
    use super::model::CalendarModel;
    use super::model_tests::local_midnight;
    use chrono::{Datelike, Days, FixedOffset, TimeDelta};

    let runtime = runtime();
    let zones = [
        -12 * 60,
        -9 * 60 - 30,
        -4 * 60,
        0,
        5 * 60 + 45,
        9 * 60,
        13 * 60,
        14 * 60,
    ]
    .map(|minutes| FixedOffset::east_opt(minutes * 60).expect("offset"));
    let months = [
        "2026-08-01",
        "2026-12-01",
        "2027-01-01",
        "2028-02-01",
        "2026-04-01",
    ]
    .map(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("month"));
    for zone in zones {
        for month in months {
            let window = Window::for_month(month);
            let after_last = window.last().checked_add_days(Days::new(1)).expect("day");
            let first = local_midnight(&zone, window.start());
            let last = local_midnight(&zone, after_last) - TimeDelta::seconds(1);
            let episode = |id: i64, air: chrono::DateTime<chrono::Utc>| {
                format!(
                    r#"{{"id":{id},"title":"Edge","airDateUtc":"{}","monitored":true,"seasonNumber":1,"episodeNumber":{id},"seriesId":9,"series":{{"id":9,"title":"Night Desk","monitored":true}}}}"#,
                    air.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                )
            };
            let sonarr = format!("[{},{}]", episode(1, first), episode(2, last));
            let radarr = format!(
                r#"[{{"id":5,"title":"Edge Film","monitored":true,"inCinemas":"{}T00:00:00Z","digitalRelease":"{}T00:00:00Z"}}]"#,
                window.start(),
                window.last()
            );
            let (address, _) = fake_jellyfin(vec![
                Reply::Status(200, radarr.into_bytes()),
                Reply::Status(200, sonarr.into_bytes()),
            ]);
            let service = service(vault(&[
                ("radarr", &address, KEY),
                ("sonarr", &address, KEY),
            ]));
            // Noon UTC on the 15th is the 15th or the 16th everywhere.
            let noon = month
                .with_day(15)
                .expect("15th")
                .and_hms_opt(12, 0, 0)
                .expect("noon")
                .and_utc();
            let mut model = CalendarModel::new(zone, noon);
            for request in model.plan() {
                model.apply(answer(&runtime, &service, request));
            }
            // Radarr is planned first, and the stand-in answers in order.
            for request in model.plan() {
                model.apply(answer(&runtime, &service, request));
            }
            let ids = |day| -> Vec<String> {
                model
                    .day_events(day)
                    .iter()
                    .map(|event| event.id.clone())
                    .collect()
            };
            assert_eq!(
                ids(window.start()),
                vec!["radarr-5-theatrical", "sonarr-1"],
                "{month} at {zone}: the first grid day"
            );
            assert_eq!(
                ids(window.last()),
                vec!["radarr-5-digital", "sonarr-2"],
                "{month} at {zone}: the last grid day"
            );
        }
    }
}
