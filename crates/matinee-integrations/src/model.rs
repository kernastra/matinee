//! Calendar records the shipping UI already understands.
//!
//! `date` keeps the server's original string. Sorting and window checks use
//! [`crate::time::parse_instant`], not that string's lexical order.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::provider::IntegrationProvider;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReleaseKind {
    Theatrical,
    Digital,
    Physical,
    Episode,
}

impl ReleaseKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Theatrical => "theatrical",
            Self::Digital => "digital",
            Self::Physical => "physical",
            Self::Episode => "episode",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseMilestone {
    pub date: String,
    pub kind: ReleaseKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingRelease {
    pub id: String,
    pub source: IntegrationProvider,
    pub source_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<i64>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    /// Original server timestamp. Not a second clock.
    pub date: String,
    pub release_kind: ReleaseKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    pub genres: Vec<String>,
    pub monitored: bool,
    pub downloaded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestones: Option<Vec<ReleaseMilestone>>,
    /// Parsed instant used for ordering. Omitted from the invoke payload.
    #[serde(skip, default = "default_instant")]
    pub instant: DateTime<Utc>,
}

fn default_instant() -> DateTime<Utc> {
    DateTime::UNIX_EPOCH
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingResult {
    pub events: Vec<UpcomingRelease>,
    pub errors: BTreeMap<String, String>,
    pub home: Vec<UpcomingRelease>,
}

impl UpcomingResult {
    pub fn error_message(&self, provider: IntegrationProvider) -> Option<&str> {
        self.errors.get(provider.as_str()).map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationKeyStatus {
    pub provider: IntegrationProvider,
    pub configured: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationConnection {
    pub provider: IntegrationProvider,
    pub configured: bool,
    pub server_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// One upcoming request. Empty provider URLs mean that provider is off.
///
/// The URL is the address saved in settings. It selects the provider and
/// forms the cache key. The request itself uses the server URL stored with
/// the API key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpcomingQuery {
    pub radarr_url: String,
    pub sonarr_url: String,
    pub start: String,
    pub end: String,
    pub home_limit: usize,
}

impl UpcomingQuery {
    pub fn new(
        radarr_url: impl Into<String>,
        sonarr_url: impl Into<String>,
        start: impl Into<String>,
        end: impl Into<String>,
    ) -> Self {
        Self {
            radarr_url: radarr_url.into(),
            sonarr_url: sonarr_url.into(),
            start: start.into(),
            end: end.into(),
            home_limit: 12,
        }
    }

    pub fn with_home_limit(mut self, limit: usize) -> Self {
        self.home_limit = limit;
        self
    }

    pub(crate) fn cache_key(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}",
            self.radarr_url, self.sonarr_url, self.start, self.end
        )
    }
}
