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
    fn display_and_debug_omit_a_planted_secret() {
        let error = CredentialError::backend("password=super-secret-value\nline");
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains("super-secret-value"));
        assert!(!format!("{error:?}").contains('\n'));
    }
}
