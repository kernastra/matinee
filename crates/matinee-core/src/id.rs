//! Identifiers that keep a user id from being passed where an item id belongs.
//!
//! The inner value may be a server id. The newtype is the only ceremony.

use std::fmt;

/// An identifier failed validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdError {
    kind: &'static str,
}

impl IdError {
    fn new(kind: &'static str) -> Self {
        Self { kind }
    }
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}", self.kind)
    }
}

impl std::error::Error for IdError {}

macro_rules! id_newtype {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// Accept a non-empty id without path or control characters.
            pub fn parse(value: impl AsRef<str>) -> Result<Self, IdError> {
                parse_id(value.as_ref(), $label).map(Self)
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

id_newtype!(ItemId, "item id");
id_newtype!(UserId, "user id");
id_newtype!(LibraryId, "library id");
id_newtype!(MediaSourceId, "media source id");
id_newtype!(PlaySessionId, "play session id");

fn parse_id(value: &str, kind: &'static str) -> Result<String, IdError> {
    if value.is_empty()
        || value.len() > 200
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '/' | '\\' | '?' | '#' | '%' | '"' | '<' | '>')
        })
    {
        return Err(IdError::new(kind));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_server_guid() {
        let id = ItemId::parse("a1b2c3d4e5f64789a0b1c2d3e4f50617").unwrap();
        assert_eq!(id.as_str(), "a1b2c3d4e5f64789a0b1c2d3e4f50617");
    }

    #[test]
    fn accepts_a_hyphenated_play_session() {
        let id = PlaySessionId::parse("4f3c2b10-1111-4222-8333-abcdef012345").unwrap();
        assert!(id.as_str().contains('-'));
    }

    #[test]
    fn rejects_empty_whitespace_and_path_characters() {
        assert!(ItemId::parse("").is_err());
        assert!(ItemId::parse("   ").is_err());
        assert!(ItemId::parse("movie/1").is_err());
        assert!(ItemId::parse("movie?x=1").is_err());
        assert!(ItemId::parse("a\nb").is_err());
        assert!(UserId::parse("user 1").is_err());
    }
}
