//! Where artwork may go, on the real HTTP client.
//!
//! Loopback servers stand in for hosts: `[::1]` plays a public cover host and
//! `127.0.0.1` a private one, on the same port, so an address that should
//! never be dialled has a server waiting on it. A scripted DNS answer stands
//! in for the resolver. Every refusal is checked where it would have landed:
//! the server that would have been reached records no connection.

use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use url::Url;

use crate::destination::{Lookup, public_ip, public_url};
use crate::transport::{
    IntegrationRequest, IntegrationResponse, ReqwestTransport, Transport, TransportError,
};

/// The public cover host in these tests.
const PUBLIC: IpAddr = IpAddr::V6(Ipv6Addr::LOCALHOST);
/// A private host: what a hostile answer points at.
const PRIVATE: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const CAP: usize = 16 * 1024 * 1024;

fn stand_in_policy(address: IpAddr) -> bool {
    address == PUBLIC
}

/// How a test server answers every connection.
#[derive(Clone)]
enum Reply {
    Ok(&'static [u8]),
    Redirect(String),
    /// Announce a body far over the cap, then trickle.
    Announced,
    /// A chunked body that never ends.
    Endless,
    /// Read the request and never answer.
    Hang,
}

struct Server {
    port: u16,
    connections: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl Server {
    fn hits(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn serve(listener: TcpListener, reply: Reply) -> Server {
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let connections = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let (count, log, done) = (
        Arc::clone(&connections),
        Arc::clone(&requests),
        Arc::clone(&stop),
    );
    std::thread::spawn(move || {
        while !done.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    count.fetch_add(1, Ordering::SeqCst);
                    answer(stream, &reply, &log, &done);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return,
            }
        }
    });
    Server {
        port,
        connections,
        requests,
        stop,
    }
}

fn answer(mut stream: TcpStream, reply: &Reply, log: &Mutex<Vec<String>>, done: &AtomicBool) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut head = Vec::new();
    let mut buffer = [0u8; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => head.extend_from_slice(&buffer[..read]),
        }
    }
    log.lock()
        .unwrap()
        .push(String::from_utf8_lossy(&head).into_owned());
    let _ = match reply {
        Reply::Ok(body) => stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .and_then(|()| stream.write_all(body)),
        Reply::Redirect(location) => stream.write_all(
            format!(
                "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        ),
        Reply::Announced => stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1073741824\r\n\r\n")
            .and_then(|()| {
                let chunk = [7u8; 1024];
                for _ in 0..4096 {
                    stream.write_all(&chunk)?;
                }
                Ok(())
            }),
        Reply::Endless => stream
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .and_then(|()| {
                let chunk = vec![7u8; 64 * 1024];
                for _ in 0..2048 {
                    stream.write_all(format!("{:x}\r\n", chunk.len()).as_bytes())?;
                    stream.write_all(&chunk)?;
                    stream.write_all(b"\r\n")?;
                }
                Ok(())
            }),
        Reply::Hang => {
            let until = Instant::now() + Duration::from_secs(10);
            while Instant::now() < until && !done.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        }
    };
}

/// A public stand-in on `[::1]` and a private host on `127.0.0.1`, both on
/// one port, so a URL's port reaches whichever address the client dials.
fn public_and_private(public: Reply, private: Reply) -> (Server, Server) {
    for _ in 0..50 {
        let first = TcpListener::bind(SocketAddr::new(PUBLIC, 0)).expect("IPv6 loopback");
        let port = first.local_addr().unwrap().port();
        if let Ok(second) = TcpListener::bind(SocketAddr::new(PRIVATE, port)) {
            return (serve(first, public), serve(second, private));
        }
    }
    panic!("no port free on both loopback addresses");
}

/// DNS that answers from a script, counting lookups.
fn scripted_dns(
    script: impl Fn(&str, usize) -> Vec<IpAddr> + Send + Sync + 'static,
) -> (Lookup, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let lookup: Lookup = Arc::new(move |host: &str| {
        let call = counter.fetch_add(1, Ordering::SeqCst);
        Ok(script(host, call))
    });
    (lookup, calls)
}

fn transport(lookup: Lookup, allow: fn(IpAddr) -> bool) -> ReqwestTransport {
    ReqwestTransport::with_public_lookup(lookup, allow, Duration::from_secs(2))
}

fn fetch(
    transport: &ReqwestTransport,
    url: String,
    public_only: bool,
) -> Result<IntegrationResponse, TransportError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(transport.send(IntegrationRequest {
            method: "GET",
            url,
            headers: Vec::new(),
            query: Vec::new(),
            max_body: Some(CAP),
            public_only,
        }))
}

fn refused(result: Result<IntegrationResponse, TransportError>) {
    match result {
        Err(error) => assert!(error.is_forbidden_destination(), "{error:?}"),
        Ok(response) => panic!("reached a host: HTTP {}", response.status),
    }
}

#[test]
fn only_public_addresses_pass_the_policy() {
    let public = [
        "8.8.8.8",
        "93.184.216.34",
        "172.32.0.1",
        "100.128.0.1",
        "2606:4700::6810:84e5",
        "::ffff:8.8.8.8",
        "64:ff9b::808:808",
        "2002:808:808::1",
    ];
    let forbidden = [
        "127.0.0.1",
        "127.255.255.254",
        "10.1.2.3",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.1.20",
        "169.254.169.254",
        "100.64.0.1",
        "100.127.255.255",
        "0.0.0.0",
        "0.1.2.3",
        "255.255.255.255",
        "224.0.0.1",
        "240.0.0.1",
        "192.0.0.1",
        "192.0.2.1",
        "198.18.0.1",
        "198.51.100.1",
        "203.0.113.1",
        "::",
        "::1",
        "::127.0.0.1",
        "fe80::1",
        "fec0::1",
        "fc00::1",
        "fd00::1",
        "ff02::1",
        "2001:db8::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "::ffff:169.254.169.254",
        "64:ff9b::a00:1",
        "2002:a00:1::1",
    ];
    for address in public {
        assert!(public_ip(address.parse().unwrap()), "{address} is public");
    }
    for address in forbidden {
        assert!(!public_ip(address.parse().unwrap()), "{address} is not");
    }
    for (url, allowed) in [
        ("https://image.tmdb.org/t/p/original/a.jpg", true),
        ("http://artworks.thetvdb.com/banners/a.jpg", true),
        ("http://localhost/a.jpg", false),
        ("http://LOCALHOST./a.jpg", false),
        ("http://covers.localhost/a.jpg", false),
        ("http://2130706433/a.jpg", false),
        ("http://0x7f000001/a.jpg", false),
        ("http://[::ffff:7f00:1]/a.jpg", false),
        ("https://user:pass@image.tmdb.org/a.jpg", false),
        ("ftp://image.tmdb.org/a.jpg", false),
        ("file:///etc/passwd", false),
    ] {
        assert_eq!(public_url(&Url::parse(url).unwrap()), allowed, "{url}");
    }
}

#[test]
fn a_public_host_is_reached_once_per_connection_with_no_credential() {
    let (public, private) = public_and_private(Reply::Ok(b"cover"), Reply::Ok(b"private"));
    let (lookup, lookups) = scripted_dns(|_, _| vec![PUBLIC]);
    let client = transport(lookup, stand_in_policy);
    let response = fetch(
        &client,
        format!("http://covers.test:{}/poster.jpg", public.port),
        true,
    )
    .expect("the public host answers");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"cover");
    assert_eq!(public.hits(), 1);
    assert_eq!(private.hits(), 0);
    assert_eq!(
        lookups.load(Ordering::SeqCst),
        1,
        "one lookup, no second check"
    );
    let head = public.requests().join("\n").to_ascii_lowercase();
    assert!(head.starts_with("get /poster.jpg http/1.1"), "{head}");
    assert!(head.contains(&format!("host: covers.test:{}", public.port)));
    assert!(!head.contains("x-api-key") && !head.contains("authorization"));
}

#[test]
fn a_public_name_with_a_private_answer_is_refused_before_any_connection() {
    let (public, private) = public_and_private(Reply::Ok(b"cover"), Reply::Ok(b"private"));
    // Production policy: every loopback answer is forbidden, IPv4 or IPv6,
    // mapped or not, and so are private, link-local, and metadata answers.
    for answer in [
        "127.0.0.1",
        "::1",
        "::ffff:127.0.0.1",
        "10.0.0.5",
        "192.168.1.20",
        "169.254.169.254",
        "fd00::1",
        "fe80::1",
    ] {
        let address: IpAddr = answer.parse().unwrap();
        let (lookup, lookups) = scripted_dns(move |_, _| vec![address]);
        let client = transport(lookup, public_ip);
        refused(fetch(
            &client,
            format!("http://innocent.test:{}/poster.jpg", public.port),
            true,
        ));
        assert_eq!(lookups.load(Ordering::SeqCst), 1, "{answer}");
    }
    assert_eq!(public.hits(), 0, "nothing reached [::1]");
    assert_eq!(private.hits(), 0, "nothing reached 127.0.0.1");
}

#[test]
fn a_name_with_public_and_private_answers_is_refused_whole() {
    let (public, private) = public_and_private(Reply::Ok(b"cover"), Reply::Ok(b"private"));
    for answers in [vec![PUBLIC, PRIVATE], vec![PRIVATE, PUBLIC]] {
        let (lookup, _) = scripted_dns(move |_, _| answers.clone());
        let client = transport(lookup, stand_in_policy);
        refused(fetch(
            &client,
            format!("http://mixed.test:{}/poster.jpg", public.port),
            true,
        ));
    }
    assert_eq!(public.hits(), 0, "not even the public answer is used");
    assert_eq!(private.hits(), 0);
}

#[test]
fn a_rebinding_answer_cannot_move_a_connection_to_a_private_host() {
    // The name is public on its first lookup and private on its second. The
    // connection dials the addresses its own lookup checked, and the next
    // connection's lookup is checked again, so the private answer is never
    // dialled.
    let (public, private) = public_and_private(Reply::Ok(b"cover"), Reply::Ok(b"private"));
    let (lookup, lookups) = scripted_dns(|_, call| {
        if call == 0 {
            vec![PUBLIC]
        } else {
            vec![PRIVATE]
        }
    });
    let client = transport(lookup, stand_in_policy);
    let url = format!("http://rebind.test:{}/poster.jpg", public.port);
    assert_eq!(
        fetch(&client, url.clone(), true).expect("first").body,
        b"cover"
    );
    // The server closed that connection, so this is a new one.
    refused(fetch(&client, url, true));
    assert_eq!(
        lookups.load(Ordering::SeqCst),
        2,
        "one lookup per connection"
    );
    assert_eq!(public.hits(), 1);
    assert_eq!(private.hits(), 0, "the rebound answer was never dialled");
}

#[test]
fn a_redirect_toward_a_private_host_is_returned_not_followed() {
    let (public, private) =
        public_and_private(Reply::Redirect(String::new()), Reply::Ok(b"private"));
    drop(public);
    for target in [
        format!("http://127.0.0.1:{}/", private.port),
        format!("http://private.test:{}/", private.port),
    ] {
        let listener = TcpListener::bind(SocketAddr::new(PUBLIC, 0)).unwrap();
        let redirector = serve(listener, Reply::Redirect(target.clone()));
        let (lookup, _) = scripted_dns(|host, _| {
            if host == "private.test" {
                vec![PRIVATE]
            } else {
                vec![PUBLIC]
            }
        });
        let client = transport(lookup, stand_in_policy);
        let response = fetch(
            &client,
            format!("http://covers.test:{}/poster.jpg", redirector.port),
            true,
        )
        .expect("the redirect itself is the answer");
        assert_eq!(response.status, 302, "{target}");
        assert_eq!(redirector.hits(), 1);
    }
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(private.hits(), 0, "no redirect was followed");
}

#[test]
fn an_ip_literal_is_checked_although_no_resolver_sees_it() {
    let (public, private) = public_and_private(Reply::Ok(b"cover"), Reply::Ok(b"private"));
    // Even with a resolver that would allow [::1], a literal is judged by the
    // production rule, because the connector dials literals directly.
    let (lookup, lookups) = scripted_dns(|_, _| vec![PUBLIC]);
    let client = transport(lookup, stand_in_policy);
    for url in [
        format!("http://127.0.0.1:{}/poster.jpg", private.port),
        format!("http://[::1]:{}/poster.jpg", public.port),
        format!("http://[::ffff:127.0.0.1]:{}/poster.jpg", private.port),
        format!("http://localhost:{}/poster.jpg", private.port),
        format!("http://user:secret@covers.test:{}/poster.jpg", public.port),
    ] {
        refused(fetch(&client, url, true));
    }
    assert_eq!(
        lookups.load(Ordering::SeqCst),
        0,
        "refused before resolution"
    );
    assert_eq!(public.hits() + private.hits(), 0);
}

#[test]
fn the_configured_server_path_still_reaches_a_private_address() {
    // API calls are not `public_only`: Radarr and Sonarr usually live on the
    // person's own network.
    let (_, private) = public_and_private(Reply::Ok(b"cover"), Reply::Ok(b"calendar"));
    let (lookup, lookups) = scripted_dns(|_, _| vec![PUBLIC]);
    let client = transport(lookup, stand_in_policy);
    let response = fetch(
        &client,
        format!("http://127.0.0.1:{}/api/v3/calendar", private.port),
        false,
    )
    .expect("a configured server on loopback");
    assert_eq!(response.body, b"calendar");
    assert_eq!(
        lookups.load(Ordering::SeqCst),
        0,
        "the artwork resolver is not used"
    );
}

#[test]
fn an_allowed_host_is_still_held_to_the_size_cap_and_the_timeout() {
    let (lookup, _) = scripted_dns(|_, _| vec![PUBLIC]);
    let client = transport(lookup, stand_in_policy);
    for reply in [Reply::Announced, Reply::Endless] {
        let listener = TcpListener::bind(SocketAddr::new(PUBLIC, 0)).unwrap();
        let server = serve(listener, reply);
        let started = Instant::now();
        let error = fetch(
            &client,
            format!("http://covers.test:{}/a.jpg", server.port),
            true,
        )
        .expect_err("over the cap");
        assert!(error.is_body_too_large(), "{error:?}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "cut, not timed out"
        );
    }
    let listener = TcpListener::bind(SocketAddr::new(PUBLIC, 0)).unwrap();
    let silent = serve(listener, Reply::Hang);
    let started = Instant::now();
    let error = fetch(
        &client,
        format!("http://covers.test:{}/a.jpg", silent.port),
        true,
    )
    .expect_err("no answer");
    assert!(!error.is_forbidden_destination() && !error.is_body_too_large());
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_millis(1500) && waited < Duration::from_secs(5),
        "the request timeout ended it ({waited:?})"
    );
}

#[test]
fn a_cancelled_fetch_closes_its_connection_at_once() {
    // Leaving a day aborts its cover tasks; dropping the request future must
    // hang up, not leave a download running until the timeout.
    let listener = TcpListener::bind(SocketAddr::new(PUBLIC, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (closed, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buffer = [0u8; 1024];
        let _ = stream.read(&mut buffer);
        let started = Instant::now();
        // Never answer; report when the client hangs up (an end of stream
        // or a reset).
        let outcome = stream.read(&mut buffer);
        let hung_up = matches!(&outcome, Ok(0))
            || outcome
                .as_ref()
                .is_err_and(|error| error.kind() == io::ErrorKind::ConnectionReset);
        let _ = closed.send((hung_up, started.elapsed(), format!("{outcome:?}")));
    });
    let (lookup, _) = scripted_dns(|_, _| vec![PUBLIC]);
    let client =
        ReqwestTransport::with_public_lookup(lookup, stand_in_policy, Duration::from_secs(20));
    // As in the app, the runtime keeps running after the caller gives up, so
    // the connection's own task sees the cancellation.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let cancelled = runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_millis(200),
            client.send(IntegrationRequest {
                method: "GET",
                url: format!("http://covers.test:{port}/a.jpg"),
                headers: Vec::new(),
                query: Vec::new(),
                max_body: Some(CAP),
                public_only: true,
            }),
        )
        .await
    });
    assert!(
        cancelled.is_err(),
        "the fetch was still waiting when dropped"
    );
    let (hung_up, after, outcome) = wait
        .recv_timeout(Duration::from_secs(5))
        .expect("the server saw the end");
    assert!(hung_up, "the connection was closed: {outcome}");
    assert!(
        after < Duration::from_secs(1),
        "closed on cancel, not at the timeout"
    );
}
