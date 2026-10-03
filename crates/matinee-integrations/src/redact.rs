//! Strip credentials from errors and logs.

const SENSITIVE: &[&str] = &[
    "api_key",
    "apikey",
    "x-api-key",
    "authorization",
    "access_token",
    "accesstoken",
    "password",
    "token",
];

pub(crate) fn redact(value: &str) -> String {
    let mut text = strip_url_queries(value);
    for key in SENSITIVE {
        text = redact_assignment(&text, key);
    }
    let mut out = String::new();
    for character in text.chars().take(180) {
        if character.is_control() {
            out.push(' ');
        } else {
            out.push(character);
        }
    }
    out.trim().to_string()
}

pub(crate) fn log_target(value: &str) -> String {
    let Ok(url) = url::Url::parse(value) else {
        return "[unparsed-url]".to_string();
    };
    let mut out = format!("{}://", url.scheme());
    match url.host_str() {
        Some(host) => out.push_str(host),
        None => out.push_str("[no-host]"),
    }
    if let Some(port) = url.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    out.push_str(url.path());
    out
}

fn strip_url_queries(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(index) = rest.find('?') {
        out.push_str(&rest[..index]);
        rest = &rest[index + 1..];
        if let Some(end) = rest.find(|character: char| {
            character.is_whitespace() || matches!(character, ')' | ']' | '"' | '\'')
        }) {
            rest = &rest[end..];
        } else {
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

fn redact_assignment(value: &str, key: &str) -> String {
    let mut rest = value;
    let mut out = String::new();
    loop {
        let Some(index) = find_assignment_key(rest, key) else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..index]);
        out.push_str("[redacted]");
        rest = consume_assigned_value(&rest[index + key.len()..]);
    }
    out
}

fn find_assignment_key(haystack: &str, key: &str) -> Option<usize> {
    let lower = haystack.to_ascii_lowercase();
    let needle = key.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(index) = lower[offset..].find(&needle) {
        let absolute = offset + index;
        let before =
            absolute == 0 || !is_name_char(lower[..absolute].chars().next_back().unwrap_or(' '));
        let after_index = absolute + needle.len();
        let after = after_index >= lower.len()
            || !is_name_char(lower[after_index..].chars().next().unwrap_or(' '));
        if before && after {
            return Some(absolute);
        }
        offset = after_index;
        if offset >= lower.len() {
            break;
        }
    }
    None
}

fn is_name_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn consume_assigned_value(value: &str) -> &str {
    let value = value.trim_start_matches([' ', '\t', '=', ':', '"', '\'']);
    let (token, after) = split_token(value);
    if token.eq_ignore_ascii_case("bearer") {
        let after = after.trim_start_matches([' ', '\t', '=', ':', '"', '\'']);
        split_token(after).1
    } else {
        after
    }
}

fn split_token(value: &str) -> (&str, &str) {
    match value.find(|character: char| {
        character.is_whitespace()
            || matches!(
                character,
                ',' | '&' | '"' | '\'' | '<' | '>' | ';' | '(' | ')'
            )
    }) {
        Some(end) => (&value[..end], &value[end..]),
        None => (value, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_api_keys_headers_and_queries() {
        let text = redact(
            "X-Api-Key: super-secret-key failed token=abc123 authorization=Bearer bearer-secret password=hunter2 <b>api_key=html-secret</b> http://radarr.local/api?apikey=query-secret tokenizer=keep-me",
        );
        for secret in [
            "super-secret-key",
            "abc123",
            "bearer-secret",
            "hunter2",
            "html-secret",
            "query-secret",
        ] {
            assert!(!text.contains(secret), "{secret}");
        }
        assert!(text.contains("keep-me"));
        assert!(!text.contains('?'));
        let logged =
            log_target("https://radarr.local:7878/api/v3/calendar?apikey=query-secret#frag");
        assert_eq!(logged, "https://radarr.local:7878/api/v3/calendar");
    }
}
