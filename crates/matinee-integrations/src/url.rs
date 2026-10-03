//! Server address rules shared with the shipping integration form.
//!
//! HTTP and HTTPS are both valid, including LAN addresses. Credentials, a
//! query, and a fragment are rejected. A trailing slash is removed. A reverse
//! proxy base path is kept.

use url::Url;

use crate::error::IntegrationError;

pub fn normalize_server_url(value: &str) -> Result<String, IntegrationError> {
    if value
        .chars()
        .any(|character| character.is_control() && !character.is_whitespace())
    {
        return Err(invalid("Enter a valid Radarr or Sonarr server address."));
    }
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() {
        return Err(invalid("Enter the server address first."));
    }
    if value.chars().any(|character| character.is_control()) {
        return Err(invalid("Enter a valid Radarr or Sonarr server address."));
    }
    let url = Url::parse(value)
        .map_err(|_| invalid("Enter a complete address beginning with http:// or https://."))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid(
            "Integration addresses must begin with http:// or https://.",
        ));
    }
    if url.host_str().is_none() {
        return Err(invalid("Enter a valid Radarr or Sonarr server address."));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "Integration addresses cannot contain credentials, a query, or a fragment.",
        ));
    }
    Ok(value.to_string())
}

fn invalid(message: &str) -> IntegrationError {
    IntegrationError::invalid_server(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lan_localhost_local_and_proxy_paths() {
        assert_eq!(
            normalize_server_url(" http://radarr.local:7878/ ").unwrap(),
            "http://radarr.local:7878"
        );
        assert_eq!(
            normalize_server_url("http://192.168.1.20:7878").unwrap(),
            "http://192.168.1.20:7878"
        );
        assert_eq!(
            normalize_server_url("http://[2001:db8::1]:8989/sonarr/").unwrap(),
            "http://[2001:db8::1]:8989/sonarr"
        );
        assert_eq!(
            normalize_server_url("http://localhost:7878").unwrap(),
            "http://localhost:7878"
        );
        assert_eq!(
            normalize_server_url("https://media.example.com/radarr").unwrap(),
            "https://media.example.com/radarr"
        );
        assert_eq!(
            normalize_server_url("https://sonarr.example").unwrap(),
            "https://sonarr.example"
        );
    }

    #[test]
    fn rejects_credentials_query_fragment_scheme_and_garbage() {
        assert!(normalize_server_url("").is_err());
        assert!(normalize_server_url("   ").is_err());
        assert!(normalize_server_url("radarr.local:7878").is_err());
        assert!(normalize_server_url("file:///tmp/radarr").is_err());
        assert!(normalize_server_url("ftp://radarr.local").is_err());
        assert!(normalize_server_url("http://user:secret@radarr.local:7878").is_err());
        assert!(normalize_server_url("http://radarr.local:7878?key=secret").is_err());
        assert!(normalize_server_url("http://radarr.local:7878#frag").is_err());
        assert!(normalize_server_url("http://radarr.local:99999").is_err());
        assert!(normalize_server_url("http://radarr.local:abc").is_err());
        assert!(normalize_server_url("http://radarr.local:7878/\u{0001}").is_err());
        assert!(normalize_server_url("http://\u{0007}radarr.local").is_err());
    }
}
