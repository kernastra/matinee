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
    let mut text = value.to_string();
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

fn redact_assignment(value: &str, key: &str) -> String {
    let mut rest = value;
    let mut out = String::new();
    let needle = key.to_ascii_lowercase();
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(index) = lower.find(&needle) else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..index]);
        out.push_str("[redacted]");
        rest = &rest[index + key.len()..];
        rest = rest.trim_start_matches([' ', '=', ':', '"', '\'']);
        if let Some(end) = rest.find(|character: char| {
            character.is_whitespace() || matches!(character, ',' | '&' | '"' | '\'')
        }) {
            rest = &rest[end..];
        } else {
            rest = "";
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_api_keys_and_headers() {
        let text = redact("X-Api-Key: super-secret-key failed token=abc123");
        assert!(!text.contains("super-secret-key"));
        assert!(!text.contains("abc123"));
    }
}
