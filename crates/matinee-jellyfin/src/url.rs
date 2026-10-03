//! Server address normalization.
//!
//! Plain HTTP is allowed. Only `http` and `https` are accepted. A copied web
//! client path (`/web/index.html#!/home.html`) is removed and a reverse-proxy
//! base path in front of `/web` is kept.

use url::Url;

use crate::error::JellyfinError;

const INVALID: &str = "Enter a valid Jellyfin address, such as jellyfin.local:8096.";

pub fn normalize_server_url(value: &str) -> Result<String, JellyfinError> {
    // Reject control characters before trimming so a trailing newline or tab
    // cannot be silently dropped and later smuggled into a header or log.
    if value.chars().any(char::is_control) || value.len() > 2048 {
        return Err(JellyfinError::invalid_url(INVALID));
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(JellyfinError::invalid_url(
            "Enter your Jellyfin server address.",
        ));
    }
    // `//host` is scheme-relative. Prepending `http://` would hide that and
    // accept an address the caller did not mean as an absolute URL.
    if trimmed.starts_with("//") {
        return Err(JellyfinError::invalid_url(INVALID));
    }

    let with_scheme = if has_scheme(trimmed) {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| JellyfinError::invalid_url(INVALID))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(JellyfinError::invalid_url(
            "Jellyfin addresses must begin with http:// or https://.",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(JellyfinError::invalid_url(
            "Jellyfin addresses cannot include a username or password.",
        ));
    }
    if url.host().is_none() {
        return Err(JellyfinError::invalid_url(INVALID));
    }

    let path = strip_web_suffix(url.path());
    let path = path.trim_end_matches('/');
    let mut server = String::new();
    server.push_str(url.scheme());
    server.push_str("://");
    server.push_str(&host_for_url(&url)?);
    if let Some(port) = url.port() {
        server.push(':');
        server.push_str(&port.to_string());
    }
    if !path.is_empty() {
        if path.starts_with('/') {
            server.push_str(path);
        } else {
            server.push('/');
            server.push_str(path);
        }
    }
    Ok(server)
}

pub(crate) fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host() == right.host()
        && left.port_or_known_default() == right.port_or_known_default()
}

pub(crate) fn join_server(server: &str, path: &str) -> Result<String, JellyfinError> {
    if !path.starts_with('/') || path.starts_with("//") || path.chars().any(char::is_control) {
        return Err(JellyfinError::invalid_url(
            "The Jellyfin request path is not valid.",
        ));
    }
    Ok(format!("{server}{path}"))
}

fn has_scheme(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once("://") else {
        return false;
    };
    let mut chars = scheme.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'))
}

fn host_for_url(url: &Url) -> Result<String, JellyfinError> {
    match url.host() {
        Some(url::Host::Ipv6(address)) => Ok(format!("[{address}]")),
        Some(_) => url
            .host_str()
            .map(ToString::to_string)
            .ok_or_else(|| JellyfinError::invalid_url(INVALID)),
        None => Err(JellyfinError::invalid_url(INVALID)),
    }
}

fn strip_web_suffix(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if let Some(index) = lower.rfind("/web") {
        let rest = &lower[index + 4..];
        if rest.is_empty() || rest.starts_with('/') {
            return path[..index].to_string();
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_bare_lan_address() {
        assert_eq!(
            normalize_server_url("jellyfin.local:8096").unwrap(),
            "http://jellyfin.local:8096"
        );
        assert_eq!(
            normalize_server_url("  localhost:8096  ").unwrap(),
            "http://localhost:8096"
        );
        assert_eq!(
            normalize_server_url("192.168.1.20:8096").unwrap(),
            "http://192.168.1.20:8096"
        );
    }

    #[test]
    fn removes_a_copied_web_client_path() {
        assert_eq!(
            normalize_server_url("http://jellyfin.local:8096/web/index.html#!/home.html").unwrap(),
            "http://jellyfin.local:8096"
        );
        assert_eq!(
            normalize_server_url("HTTP://jellyfin.local:8096/Web/index.html?api_key=secret#/home")
                .unwrap(),
            "http://jellyfin.local:8096"
        );
    }

    #[test]
    fn preserves_a_base_path_before_the_web_client() {
        assert_eq!(
            normalize_server_url("https://media.example.com/jellyfin/web/").unwrap(),
            "https://media.example.com/jellyfin"
        );
        assert_eq!(
            normalize_server_url("https://media.example.com/jellyfin/web/index.html").unwrap(),
            "https://media.example.com/jellyfin"
        );
    }

    #[test]
    fn strips_trailing_slashes_and_leaves_unrelated_paths() {
        assert_eq!(
            normalize_server_url("http://jellyfin.local:8096/").unwrap(),
            "http://jellyfin.local:8096"
        );
        assert_eq!(
            normalize_server_url("http://jellyfin.local:8096/jellyfin/").unwrap(),
            "http://jellyfin.local:8096/jellyfin"
        );
        assert_eq!(
            normalize_server_url("http://jellyfin.local:8096/webhook").unwrap(),
            "http://jellyfin.local:8096/webhook"
        );
    }

    #[test]
    fn accepts_ipv6_localhost() {
        assert_eq!(
            normalize_server_url("[::1]:8096").unwrap(),
            "http://[::1]:8096"
        );
    }

    #[test]
    fn rejects_empty_unsupported_and_credentialed_urls() {
        assert!(normalize_server_url("  ").is_err());
        assert_eq!(
            normalize_server_url("").unwrap_err().to_string(),
            "Enter your Jellyfin server address."
        );
        assert_eq!(
            normalize_server_url("ftp://jellyfin.local")
                .unwrap_err()
                .to_string(),
            "Jellyfin addresses must begin with http:// or https://."
        );
        assert_eq!(
            normalize_server_url("http://sean:secret@jellyfin.local:8096")
                .unwrap_err()
                .to_string(),
            "Jellyfin addresses cannot include a username or password."
        );
        assert!(normalize_server_url("http://sean@jellyfin.local").is_err());
        assert!(normalize_server_url("http://jellyfin.local:8096\n").is_err());
        assert!(normalize_server_url("not a host").is_err());
    }

    #[test]
    fn same_origin_compares_scheme_host_and_port() {
        let base = Url::parse("http://jellyfin.local:8096/jellyfin").unwrap();
        let sibling = Url::parse("http://jellyfin.local:8096/Videos/1").unwrap();
        let other_port = Url::parse("http://jellyfin.local:8097/Videos/1").unwrap();
        let other_scheme = Url::parse("https://jellyfin.local:8096/Videos/1").unwrap();
        assert!(same_origin(&base, &sibling));
        assert!(!same_origin(&base, &other_port));
        assert!(!same_origin(&base, &other_scheme));
    }
}
