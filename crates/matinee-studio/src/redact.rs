//! Keep API keys and authorization material out of errors.

pub(crate) fn redact(value: &str) -> String {
    let mut text = value.to_string();
    for key in ["authorization", "api_key", "apiKey", "X-Api-Key", "Key "] {
        text = strip_assignment(&text, key);
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

fn strip_assignment(value: &str, key: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let needle = key.to_ascii_lowercase();
    let Some(index) = lower.find(&needle) else {
        return value.to_string();
    };
    let mut out = String::new();
    out.push_str(&value[..index]);
    out.push_str("[redacted]");
    let rest = &value[index + key.len()..];
    if let Some(end) = rest.find(|character: char| character.is_whitespace() || character == ',') {
        out.push_str(&rest[end..]);
    }
    out
}
