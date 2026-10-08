//! Where an artwork request may go.
//!
//! Artwork addresses come from Radarr's and Sonarr's JSON, so the server
//! names the host. Matinee fetches them only from the public internet:
//!
//! - [`public_url`] refuses other schemes, credentials in the address, the
//!   `localhost` names, and any IP literal that is not public.
//! - [`PublicResolver`] is the DNS resolver of the artwork client. It looks a
//!   name up once and refuses the whole name if any answer is not public.
//!   The client connects only to the addresses it returns, so there is no
//!   second lookup a rebinding server could answer differently.
//!
//! An IP literal never reaches a resolver (the connector dials it directly),
//! which is why both checks exist. Radarr and Sonarr API calls do not use
//! either: those servers are the ones the person configured, usually on
//! their own network.

use std::error::Error;
use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use url::{Host, Url};

/// Whether `url` may be fetched as artwork, as far as the address says:
/// http or https, no user or password, and a host that is not `localhost`
/// and not a non-public IP literal. Names are checked again when resolved.
pub(crate) fn public_url(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }
    match url.host() {
        Some(Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            !name.is_empty() && name != "localhost" && !name.ends_with(".localhost")
        }
        Some(Host::Ipv4(address)) => public_ip(IpAddr::V4(address)),
        Some(Host::Ipv6(address)) => public_ip(IpAddr::V6(address)),
        None => false,
    }
}

/// Whether an address is on the public internet. Loopback, private,
/// link-local (including cloud metadata at 169.254.169.254), shared
/// (100.64.0.0/10), unspecified, broadcast, multicast, documentation,
/// benchmarking, and reserved ranges are not. IPv6 is allowed only in global
/// unicast (2000::/3), and an IPv4 address carried inside IPv6 (mapped,
/// NAT64, or 6to4) is judged as that IPv4 address.
pub(crate) fn public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
    }
}

fn public_v4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_multicast()
        || address.is_documentation()
        || a == 0
        || a >= 240
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19)))
}

fn public_v6(address: Ipv6Addr) -> bool {
    if let Some(v4) = address.to_ipv4_mapped() {
        return public_v4(v4);
    }
    let segments = address.segments();
    let embedded = |high: u16, low: u16| {
        Ipv4Addr::new((high >> 8) as u8, high as u8, (low >> 8) as u8, low as u8)
    };
    // NAT64 (64:ff9b::/96): an IPv6-only network's route to IPv4.
    if segments[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        return public_v4(embedded(segments[6], segments[7]));
    }
    // Only global unicast, and not documentation (2001:db8::/32).
    if segments[0] & 0xe000 != 0x2000 || (segments[0] == 0x2001 && segments[1] == 0x0db8) {
        return false;
    }
    // 6to4 (2002::/16) carries the IPv4 address it reaches.
    if segments[0] == 0x2002 {
        return public_v4(embedded(segments[1], segments[2]));
    }
    true
}

/// A name resolved to an address artwork may not reach.
#[derive(Debug)]
pub(crate) struct ForbiddenDestination;

impl fmt::Display for ForbiddenDestination {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the artwork host is not a public address")
    }
}

impl Error for ForbiddenDestination {}

/// Looks a host name up. The system resolver in production; a fixed answer
/// in tests.
pub(crate) type Lookup = Arc<dyn Fn(&str) -> io::Result<Vec<IpAddr>> + Send + Sync>;

/// The artwork client's resolver: one lookup per connection, every answer
/// checked, and the checked addresses are the ones connected to.
pub(crate) struct PublicResolver {
    lookup: Lookup,
    allow: fn(IpAddr) -> bool,
}

impl PublicResolver {
    pub(crate) fn system() -> Self {
        Self::new(Arc::new(system_lookup), public_ip)
    }

    pub(crate) fn new(lookup: Lookup, allow: fn(IpAddr) -> bool) -> Self {
        Self { lookup, allow }
    }
}

fn system_lookup(host: &str) -> io::Result<Vec<IpAddr>> {
    Ok((host, 0)
        .to_socket_addrs()?
        .map(|address| address.ip())
        .collect())
}

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        let lookup = Arc::clone(&self.lookup);
        let allow = self.allow;
        let (sender, receiver) = futures::channel::oneshot::channel();
        // getaddrinfo blocks, so it runs off the async workers, as reqwest's
        // own resolver does. The request's timeout still bounds the wait.
        let spawned = std::thread::Builder::new()
            .name("matinee-artwork-dns".into())
            .spawn(move || {
                let _ = sender.send(lookup(&host));
            });
        Box::pin(async move {
            spawned?;
            let addresses = receiver.await??;
            if addresses.is_empty() {
                return Err(io::Error::new(io::ErrorKind::NotFound, "no address").into());
            }
            // One forbidden answer refuses the name: a host that is partly
            // private is not a public cover host.
            if !addresses.iter().all(|address| allow(*address)) {
                return Err(Box::new(ForbiddenDestination) as Box<dyn Error + Send + Sync>);
            }
            let addresses: Addrs = Box::new(
                addresses
                    .into_iter()
                    .map(|address| SocketAddr::new(address, 0)),
            );
            Ok(addresses)
        })
    }
}

/// Whether `error` or anything it wraps is a [`ForbiddenDestination`].
pub(crate) fn is_forbidden(error: &(dyn Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if error.is::<ForbiddenDestination>() {
            return true;
        }
        current = error.source();
    }
    false
}
