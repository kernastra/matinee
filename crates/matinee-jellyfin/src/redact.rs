//! Redaction for logs and error context.
//!
//! Tokens, passwords, and authorization material are removed before a string
//! is stored on an error or written to a log line.

use url::Url;

const SENSITIVE_KEYS: &[&str] = &[
    "api_key",
    "apikey",
    "accesstoken",
    "token",
    "password",
    "pw",
];

pub(crate) fn redact_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return redact_freeform(value);
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            if is_sensitive(key.as_ref()) {
                (key.into_owned(), "redacted".to_string())
            } else {
                (key.into_owned(), value.into_owned())
            }
        })
        .collect();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in &pairs {
            serializer.append_pair(key, value);
        }
        let query = serializer.finish();
        url.set_query(Some(&query));
    }
    url.to_string()
}

pub(crate) fn redact_freeform(value: &str) -> String {
    let mut text = redact_token_header(value);
    for key in [
        "api_key",
        "ApiKey",
        "apiKey",
        "AccessToken",
        "Token",
        "password",
        "Password",
        "Pw",
    ] {
        text = redact_assignment(&text, key);
    }
    truncate(&text)
}

pub(crate) fn redact_header_value(name: &str, value: &str) -> String {
    if name.eq_ignore_ascii_case("authorization") || value.starts_with("MediaBrowser ") {
        "redacted".to_string()
    } else {
        redact_freeform(value)
    }
}

fn redact_token_header(value: &str) -> String {
    let mut rest = value;
    let mut out = String::new();
    while let Some(index) = rest.find("Token=\"") {
        out.push_str(&rest[..index]);
        out.push_str("Token=\"redacted\"");
        rest = &rest[index + "Token=\"".len()..];
        if let Some(end) = rest.find('"') {
            rest = &rest[end + 1..];
        } else {
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

fn redact_assignment(value: &str, key: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let needle = format!("{}=", key.to_ascii_lowercase());
    let mut out = String::new();
    let mut cursor = 0;
    while let Some(found) = lower[cursor..].find(&needle) {
        let start = cursor + found;
        out.push_str(&value[cursor..start]);
        out.push_str(&value[start..start + needle.len()]);
        out.push_str("redacted");
        let value_start = start + needle.len();
        let rest = &value[value_start..];
        let skip = rest
            .find(|character: char| character == '&' || character.is_whitespace())
            .unwrap_or(rest.len());
        cursor = value_start + skip;
    }
    out.push_str(&value[cursor..]);
    out
}

fn is_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEYS.contains(&lower.as_str())
}

fn truncate(value: &str) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(180).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

pub(crate) fn describe_exchange(
    category: &str,
    method: &str,
    url: &str,
    status: Option<u16>,
) -> String {
    let target = Url::parse(url)
        .map(|parsed| {
            let mut out = format!("{}://", parsed.scheme());
            match parsed.host_str() {
                Some(host) => out.push_str(host),
                None => out.push_str("[no-host]"),
            }
            if let Some(port) = parsed.port() {
                out.push(':');
                out.push_str(&port.to_string());
            }
            out.push_str(parsed.path());
            out
        })
        .unwrap_or_else(|_| "[unparsed-url]".to_string());
    match status {
        Some(status) => format!("{category} {method} {target} status={status}"),
        None => format!("{category} {method} {target}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_query_tokens_and_authorization() {
        let url = "http://jellyfin.local:8096/Videos/1/stream?Static=true&api_key=super-secret&MediaSourceId=source";
        let redacted = redact_url(url);
        assert!(!redacted.contains("super-secret"));
        assert!(redacted.contains("api_key=redacted"));
        assert!(redacted.contains("MediaSourceId=source"));
        assert_eq!(
            redact_header_value("Authorization", "MediaBrowser Token=\"super-secret\""),
            "redacted"
        );
        let line = describe_exchange("playback", "GET", url, Some(200));
        assert!(!line.contains("super-secret"));
        assert!(line.contains("status=200"));
        assert!(!line.contains("api_key"));
        assert!(!line.contains('?'));
        assert!(line.contains("http://jellyfin.local:8096/Videos/1/stream"));
    }

    #[test]
    fn redacts_token_header_and_password_assignments() {
        let text = "Token=\"super-secret\" Pw=hunter2 api_key=abc";
        let redacted = redact_freeform(text);
        assert!(!redacted.contains("super-secret"));
        assert!(!redacted.contains("hunter2"));
        assert!(!redacted.contains("api_key=abc"));
        assert!(redacted.contains("redacted"));
    }
}
