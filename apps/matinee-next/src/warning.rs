//! Login's plain-HTTP notice.
//!
//! This is separate from address validation. An incomplete address produces
//! no warning; submit reports that error instead. Loopback HTTP is silent.
//! Any other HTTP server is allowed and gets an informational warning.

use matinee_jellyfin::normalize_server_url;

pub const HTTP_WARNING: &str =
    "This server uses unencrypted HTTP. Prefer HTTPS when connecting beyond this computer.";

pub fn insecure_http_warning(server: &str) -> Option<&'static str> {
    let normalized = normalize_server_url(server).ok()?;
    let url = url::Url::parse(&normalized).ok()?;
    if url.scheme() != "http" {
        return None;
    }
    if is_loopback(&url) {
        None
    } else {
        Some(HTTP_WARNING)
    }
}

fn is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address == std::net::Ipv4Addr::LOCALHOST,
        Some(url::Host::Ipv6(address)) => address == std::net::Ipv6Addr::LOCALHOST,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_and_https_are_quiet() {
        assert!(insecure_http_warning("http://localhost:8096").is_none());
        assert!(insecure_http_warning("http://127.0.0.1:8096").is_none());
        assert!(insecure_http_warning("http://[::1]:8096").is_none());
        assert!(insecure_http_warning("https://example.com").is_none());
        assert!(insecure_http_warning("HTTP://LOCALHOST:8096").is_none());
    }

    #[test]
    fn lan_and_local_names_warn() {
        assert_eq!(
            insecure_http_warning("http://192.168.1.20:8096"),
            Some(HTTP_WARNING)
        );
        assert_eq!(
            insecure_http_warning("http://jellyfin.local:8096"),
            Some(HTTP_WARNING)
        );
    }

    #[test]
    fn an_incomplete_address_does_not_warn() {
        assert!(insecure_http_warning("").is_none());
        assert!(insecure_http_warning("   ").is_none());
        assert!(insecure_http_warning("not a url").is_none());
        assert!(insecure_http_warning("http://").is_none());
    }
}
