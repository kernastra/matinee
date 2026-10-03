//! An owned secret that does not print itself.

use std::fmt;

use zeroize::Zeroize;

/// A secret string.
///
/// `Debug` and `Display` render `[redacted]`. Drop zeroizes this owned copy.
/// See the crate docs for copies this type does not control.
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the secret for a vault write or a request header.
    ///
    /// Do not log the result.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn trim_chars(&self) -> usize {
        self.0.trim().chars().count()
    }

    pub fn is_blank(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Clone for Secret {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_are_redacted() {
        let secret = Secret::new("fal-key-should-not-appear");
        let rendered = format!("{secret} {secret:?}");
        assert!(!rendered.contains("fal-key"));
        assert!(rendered.contains("[redacted]"));
        assert!(!secret.is_blank());
    }
}
