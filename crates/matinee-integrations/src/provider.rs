//! Radarr and Sonarr. Unknown names fail at the adapter, before this enum exists.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::IntegrationError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntegrationProvider {
    Radarr,
    Sonarr,
}

impl IntegrationProvider {
    pub fn parse(value: &str) -> Result<Self, IntegrationError> {
        match value {
            "radarr" => Ok(Self::Radarr),
            "sonarr" => Ok(Self::Sonarr),
            _ => Err(IntegrationError::UnknownProvider),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Radarr => "radarr",
            Self::Sonarr => "sonarr",
        }
    }
}

impl fmt::Display for IntegrationProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_providers_only() {
        assert_eq!(
            IntegrationProvider::parse("radarr").unwrap(),
            IntegrationProvider::Radarr
        );
        assert_eq!(
            IntegrationProvider::parse("sonarr").unwrap(),
            IntegrationProvider::Sonarr
        );
        assert!(IntegrationProvider::parse("jellyseerr").is_err());
        assert!(IntegrationProvider::parse("Radarr").is_err());
    }
}
