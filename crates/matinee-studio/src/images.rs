//! Image bytes, content types, and untrusted provider URLs.
//!
//! Generated-image downloads are public-network only. fal does not publish one
//! stable file host: `fal.run` returns a URL, often on `*.fal.media`, and that
//! URL may redirect to a signed object-storage address whose query is a
//! credential. An allowlist of today's CDN names would break the next host.
//! Jellyfin reference downloads do not use this policy. They must stay on the
//! configured server, including localhost, `.local`, and LAN addresses.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use url::Url;

use crate::error::StudioError;

pub(crate) fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

pub(crate) fn content_type_matches(header: &str, sniffed: &str) -> bool {
    let header = header
        .split(';')
        .next()
        .unwrap_or(header)
        .trim()
        .to_ascii_lowercase();
    if header.is_empty() || header == "application/octet-stream" {
        return true;
    }
    let header = if header == "image/jpg" {
        "image/jpeg"
    } else {
        header.as_str()
    };
    header == sniffed
}

/// A provider-supplied image URL.
///
/// HTTP and HTTPS only. The host must exist, userinfo is rejected, and an IP
/// literal or a name that is obviously local is rejected. A public hostname
/// is accepted here; [`crate::http::ReqwestStudio`] resolves it and refuses
/// the download when any address is not public, then connects to those
/// addresses so a later lookup cannot swap in a private one.
pub fn validate_provider_image_url(value: &str) -> Result<Url, StudioError> {
    let url = Url::parse(value).map_err(|_| {
        StudioError::provider("The image provider returned an unusable image address.")
    })?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(refused_address());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(StudioError::provider(
            "The image provider returned an image address that contains credentials.",
        ));
    }
    match url.host() {
        Some(url::Host::Ipv4(address)) if !is_public_ip(IpAddr::V4(address)) => {
            Err(refused_address())
        }
        Some(url::Host::Ipv6(address)) if !is_public_ip(IpAddr::V6(address)) => {
            Err(refused_address())
        }
        Some(url::Host::Domain(name)) if blocked_provider_name(name) => Err(refused_address()),
        Some(_) => Ok(url),
        None => Err(refused_address()),
    }
}

/// The next hop of a generated-image download.
///
/// Relative locations stay on the current origin. An HTTPS response may not
/// redirect to HTTP. Every hop is checked with [`validate_provider_image_url`].
pub(crate) fn follow_provider_redirect(current: &Url, location: &str) -> Result<Url, StudioError> {
    if location.trim().is_empty() {
        return Err(StudioError::provider(
            "The image provider redirected without a destination.",
        ));
    }
    let next = current
        .join(location)
        .map_err(|_| StudioError::provider("The image provider returned an unusable redirect."))?;
    let next = validate_provider_image_url(next.as_str())?;
    if current.scheme() == "https" && next.scheme() != "https" {
        return Err(StudioError::provider(
            "The image provider redirected an HTTPS download to an insecure address.",
        ));
    }
    Ok(next)
}

pub(crate) fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_v4(address),
        IpAddr::V6(address) => match address.to_ipv4_mapped() {
            Some(mapped) => is_public_v4(mapped),
            None => is_public_v6(address),
        },
    }
}

/// `IpAddr::is_global` is still unstable on Rust 1.90, so the non-public
/// ranges are listed here. This is the generated-image policy only.
fn is_public_v4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    let shared = a == 100 && (b & 0b1100_0000) == 64;
    let protocol = a == 192 && b == 0 && c == 0;
    let benchmarking = a == 198 && (b & 0b1111_1110) == 18;
    let reserved = a >= 240;
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_multicast()
        || address.is_broadcast()
        || address.is_documentation()
        || shared
        || protocol
        || benchmarking
        || reserved)
}

fn is_public_v6(address: Ipv6Addr) -> bool {
    let segments = address.segments();
    let unique_local = (segments[0] & 0xfe00) == 0xfc00;
    let link_local = (segments[0] & 0xffc0) == 0xfe80;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_multicast()
        || unique_local
        || link_local
        || documentation)
}

fn refused_address() -> StudioError {
    StudioError::provider("The image provider returned an image address Matinee will not fetch.")
}

fn blocked_provider_name(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    name == "localhost"
        || name.ends_with(".localhost")
        || name.ends_with(".local")
        || name == "metadata.google.internal"
        || name == "metadata.google"
}

pub(crate) fn same_http_origin(candidate: &Url, expected: &Url) -> bool {
    matches!(candidate.scheme(), "http" | "https")
        && candidate.scheme() == expected.scheme()
        && candidate.host_str() == expected.host_str()
        && candidate.port_or_known_default() == expected.port_or_known_default()
        && candidate.username().is_empty()
        && candidate.password().is_none()
}

pub(crate) fn extension_content_type(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_supported_images_and_rejects_other_bytes() {
        assert_eq!(
            sniff_image(b"\x89PNG\r\n\x1a\nrest").unwrap().0,
            "image/png"
        );
        assert_eq!(
            sniff_image(&[0xFF, 0xD8, 0xFF, 0x00]).unwrap().0,
            "image/jpeg"
        );
        let mut webp = b"RIFF____WEBP".to_vec();
        webp[4..8].copy_from_slice(b"size");
        assert_eq!(sniff_image(&webp).unwrap().0, "image/webp");
        assert!(sniff_image(b"<html>").is_none());
        assert!(!content_type_matches("image/png", "image/jpeg"));
        assert!(content_type_matches(
            "image/jpeg; charset=binary",
            "image/jpeg"
        ));
    }

    #[test]
    fn provider_urls_reject_credentials_and_non_http_schemes() {
        assert!(validate_provider_image_url("https://cdn.example/poster.png").is_ok());
        assert!(validate_provider_image_url("http://cdn.example/poster.png").is_ok());
        assert!(validate_provider_image_url("https://user:secret@cdn.example/poster.png").is_err());
        assert!(validate_provider_image_url("file:///etc/passwd").is_err());
        assert!(validate_provider_image_url("ftp://cdn.example/poster.png").is_err());
        assert!(validate_provider_image_url("javascript:alert(1)").is_err());
        let server = Url::parse("https://jellyfin.example:8920/base").unwrap();
        let valid = Url::parse("https://jellyfin.example:8920/Items/1/Images/Backdrop/0").unwrap();
        let wrong_port =
            Url::parse("https://jellyfin.example:9443/Items/1/Images/Backdrop/0").unwrap();
        assert!(same_http_origin(&valid, &server));
        assert!(!same_http_origin(&wrong_port, &server));
    }

    #[test]
    fn provider_urls_reject_local_and_private_destinations() {
        for url in [
            "http://127.0.0.1/poster.png",
            "http://127.0.0.1:9/poster.png",
            "http://localhost/poster.png",
            "http://localhost./poster.png",
            "http://api.localhost/poster.png",
            "http://jellyfin.local/poster.png",
            "http://[::1]/poster.png",
            "http://169.254.169.254/latest/meta-data",
            "http://10.1.2.3/poster.png",
            "http://192.168.1.20/poster.png",
            "http://172.16.0.4/poster.png",
            "http://172.31.255.1/poster.png",
            "http://[::ffff:127.0.0.1]/poster.png",
            "http://[fc00::1]/poster.png",
            "http://[fe80::1]/poster.png",
            "http://0.0.0.0/poster.png",
            "http://metadata.google.internal/poster.png",
        ] {
            assert!(
                validate_provider_image_url(url).is_err(),
                "{url} should be refused"
            );
        }
        assert!(validate_provider_image_url("https://1.1.1.1/poster.png").is_ok());
        assert!(validate_provider_image_url("https://[2606:4700:4700::1111]/poster.png").is_ok());
    }

    #[test]
    fn redirects_keep_the_public_https_policy() {
        let current = Url::parse("https://v3.fal.media/files/poster").unwrap();
        let relative = follow_provider_redirect(&current, "/files/next.png").unwrap();
        assert_eq!(relative.as_str(), "https://v3.fal.media/files/next.png");
        assert!(follow_provider_redirect(&current, "http://cdn.example/poster.png").is_err());
        assert!(follow_provider_redirect(&current, "http://127.0.0.1/poster.png").is_err());
        assert!(follow_provider_redirect(&current, "file:///etc/passwd").is_err());
        assert!(follow_provider_redirect(&current, "https://user:secret@cdn.example/a").is_err());
        assert!(follow_provider_redirect(&current, "").is_err());
        assert!(follow_provider_redirect(&current, "http://[").is_err());
        let upgraded = follow_provider_redirect(
            &Url::parse("http://cdn.example/a").unwrap(),
            "https://cdn.example/b",
        )
        .unwrap();
        assert_eq!(upgraded.scheme(), "https");
    }
}
