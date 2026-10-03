//! Vault errors. They name the failure, not the secret.

use std::fmt;

/// A credential operation failed, or the name was not acceptable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialError {
    /// The namespace or account key was empty or contained unsafe characters.
    InvalidName,
    /// The OS vault or the injected backend rejected the operation.
    ///
    /// `detail` is the backend's message after redaction. It does not include
    /// the secret value.
    Backend { detail: String },
}

impl CredentialError {
    pub(crate) fn backend(detail: impl Into<String>) -> Self {
        Self::Backend {
            detail: redact_backend(&detail.into()),
        }
    }
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName => f.write_str("The credential name is not valid."),
            Self::Backend { .. } => {
                f.write_str("The credential vault could not complete the request.")
            }
        }
    }
}

impl std::error::Error for CredentialError {}

fn redact_backend(detail: &str) -> String {
    let mut text = detail.to_string();
    for key in [
        "password",
        "api_key",
        "apikey",
        "token",
        "authorization",
        "secret",
    ] {
        text = redact_assignment(&text, key);
    }
    let mut out = String::with_capacity(text.len().min(180));
    for character in text.chars().take(180) {
        if character.is_control() {
            out.push(' ');
        } else {
            out.push(character);
        }
    }
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
    fn display_debug_and_source_omit_credential_assignments() {
        let planted = "password=super-secret token=token-secret api_key=key-secret authorization=Bearer bearer-secret <b>password=html-secret</b>\u{0001} tokenizer=keep-me";
        let error = CredentialError::backend(planted);
        let display = error.to_string();
        let debug = format!("{error:?}");
        assert!(display.contains("could not complete"));
        assert!(!display.contains('='));
        for secret in [
            "super-secret",
            "token-secret",
            "key-secret",
            "bearer-secret",
            "html-secret",
        ] {
            assert!(!display.contains(secret), "{secret} in display");
            assert!(!debug.contains(secret), "{secret} in debug");
        }
        assert!(!debug.contains('\u{0001}'));
        assert!(debug.contains("keep-me"));
        assert!(std::error::Error::source(&error).is_none());
    }
}
