//! A Jellyfin stand-in on loopback for tests that use the real HTTP client.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use matinee_core::{User, UserId};
use matinee_jellyfin::{JellyfinClient, ReqwestTransport, Session};

pub(crate) enum Reply {
    /// Answer with this status and body, then close the connection.
    Status(u16, Vec<u8>),
    /// Read the request and never answer.
    Hang,
}

/// Serve one reply per connection, in order. Each request's head and body
/// arrive on the receiver, decoded lossily as text.
pub(crate) fn fake_jellyfin(replies: Vec<Reply>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for reply in replies {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let Some(request) = read_request(&mut stream) else {
                return;
            };
            let _ = tx.send(request);
            match reply {
                Reply::Status(status, body) => {
                    let mut response = format!(
                        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes();
                    response.extend_from_slice(&body);
                    let _ = stream.write_all(&response);
                }
                Reply::Hang => {
                    std::thread::sleep(Duration::from_secs(30));
                    return;
                }
            }
        }
    });
    (address, rx)
}

fn read_request(stream: &mut std::net::TcpStream) -> Option<String> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&request);
        if let Some(head_end) = text.find("\r\n\r\n") {
            let length = text[..head_end]
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if request.len() >= head_end + 4 + length {
                break;
            }
        }
    }
    Some(String::from_utf8_lossy(&request).into_owned())
}

pub(crate) const FIXTURE_TOKEN: &str = "fixture-token";

pub(crate) fn client(address: &str) -> Arc<JellyfinClient<ReqwestTransport>> {
    let session = Session::new(
        address,
        FIXTURE_TOKEN,
        User::new(UserId::parse("user-1").unwrap(), "alex", None),
    )
    .unwrap();
    Arc::new(JellyfinClient::new(
        session,
        ReqwestTransport::new().unwrap(),
    ))
}
