//! Keep API keys and authorization material out of errors and logs.

use url::Url;

/// Scheme, host, port, and path. Query and fragment stay out of logs because
/// fal's signed download URLs carry the credential there.
pub(crate) fn log_target(value: &str) -> String {
    let Ok(url) = Url::parse(value) else {
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
    let path = url.path();
    if !path.is_empty() {
        out.push_str(path);
    }
    out
}

pub(crate) fn redact(value: &str) -> String {
    let mut text = strip_url_queries(value);
    for key in [
        "authorization",
        "api_key",
        "apikey",
        "x-api-key",
        "password",
        "token",
        "key",
    ] {
        text = redact_assignment(&text, key);
    }
    let mut out = String::new();
    for character in text.chars().take(280) {
        if character.is_control() {
            out.push(' ');
        } else {
            out.push(character);
        }
    }
    out.trim().to_string()
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
    fn logs_omit_signed_query_strings() {
        let logged =
            log_target("https://user:secret@cdn.example:443/files/a.png?token=signed-secret#frag");
        assert_eq!(logged, "https://cdn.example/files/a.png");
        assert_eq!(log_target("not a url"), "[unparsed-url]");
        let redacted = redact(
            "authorization=Bearer bearer-secret password=hunter2 https://cdn.example/a?token=query-secret",
        );
        assert!(!redacted.contains("bearer-secret"));
        assert!(!redacted.contains("hunter2"));
        assert!(!redacted.contains("query-secret"));
    }
}
