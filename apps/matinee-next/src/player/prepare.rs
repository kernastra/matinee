//! Ask Jellyfin for one item and a playback plan.
//!
//! Source selection stays in `matinee-jellyfin`. This function does not invent
//! a stream URL.

use std::time::Duration;

use matinee_core::{ItemId, MediaItem, PlaybackOptions};
use matinee_jellyfin::{JellyfinClient, JellyfinError, ReqwestTransport, Session};

use super::model::{PlanFailure, PreparedPlayback};

pub(crate) async fn prepare_playback(
    session: Session,
    item_id: ItemId,
) -> Result<PreparedPlayback, PlanFailure> {
    let transport = ReqwestTransport::new().map_err(|_| {
        PlanFailure::stream("Could not reach Jellyfin. Check the server address and try again.")
    })?;
    let client = JellyfinClient::new(session, transport);
    let item = client
        .item_details(&item_id)
        .await
        .map_err(failure_from_client)?;
    let runtime = item.metadata.runtime;
    let title = item.name().to_string();
    let context = episode_context(&item);
    let plan = client
        .playback_plan(&item, PlaybackOptions::default(), None)
        .await
        .map_err(failure_from_client)?;
    Ok(PreparedPlayback {
        title,
        context,
        runtime,
        plan,
    })
}

pub(crate) fn episode_context(item: &MediaItem) -> Option<String> {
    let series = item
        .hierarchy
        .series_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let episode = item.hierarchy.episode_label();
    match (series, episode) {
        (Some(series), Some(episode)) => Some(format!("{series} · {episode}")),
        (Some(series), None) => Some(series.to_string()),
        (None, Some(episode)) => Some(episode),
        (None, None) => None,
    }
}

fn failure_from_client(error: JellyfinError) -> PlanFailure {
    match &error {
        JellyfinError::NoCompatibleSource => PlanFailure::incompatible(error.to_string()),
        JellyfinError::Unreachable { .. } | JellyfinError::Cancelled => {
            PlanFailure::stream(error.to_string())
        }
        _ => PlanFailure::info(error.to_string()),
    }
}

/// Drop credential query pairs. The load uses the session header instead.
pub(crate) fn strip_credential_query(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return url.to_string();
    };
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(key, _)| !is_credential_key(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if pairs.is_empty() {
        parsed.set_query(None);
    } else {
        let mut encoded = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in &pairs {
            encoded.append_pair(key, value);
        }
        let query = encoded.finish();
        parsed.set_query(Some(&query));
    }
    parsed.to_string()
}

fn is_credential_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "api_key" | "apikey" | "accesstoken" | "token" | "password" | "pw"
    )
}

/// Where a resume position should start, following the shipping player.
///
/// A zero position is unwatched. A position inside the last 30 seconds of a
/// known runtime is treated as finished and starts at the beginning. Without
/// a runtime the resume is kept and corrected once the file duration arrives.
pub(crate) fn resume_start(resume: Duration, runtime: Option<Duration>) -> Option<Duration> {
    if resume.is_zero() {
        return None;
    }
    if let Some(runtime) = runtime.filter(|runtime| !runtime.is_zero())
        && resume + super::COMPLETED_TAIL >= runtime
    {
        return None;
    }
    Some(resume)
}

pub(crate) fn redact_message(value: &str) -> String {
    let mut text = redact_token_quotes(value);
    for key in [
        "api_key",
        "apiKey",
        "ApiKey",
        "AccessToken",
        "token",
        "Token",
    ] {
        text = redact_assignment(&text, key);
    }
    if text.len() > 280 {
        let mut end = 280;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

fn redact_token_quotes(value: &str) -> String {
    let mut rest = value;
    let mut out = String::new();
    let marker = "Token=\"";
    while let Some(index) = rest.find(marker) {
        out.push_str(&rest[..index]);
        out.push_str("Token=\"redacted\"");
        rest = &rest[index + marker.len()..];
        if let Some(end) = rest.find('"') {
            rest = &rest[end + 1..];
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
    let needle = format!("{key}=");
    while let Some(index) = rest.find(&needle) {
        out.push_str(&rest[..index]);
        out.push_str(&needle);
        out.push_str("redacted");
        rest = &rest[index + needle.len()..];
        let skip = rest
            .find(|character: char| {
                character == '&' || character == ' ' || character == '"' || character == ','
            })
            .unwrap_or(rest.len());
        rest = &rest[skip..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use matinee_core::{
        ItemHierarchy, ItemIdentity, ItemKind, ItemMetadata, MediaItem, TechnicalMedia,
        UserItemState,
    };

    fn item(series: Option<&str>, season: Option<u32>, episode: Option<u32>) -> MediaItem {
        MediaItem {
            identity: ItemIdentity {
                id: ItemId::parse("item-1").unwrap(),
                name: "Northwind".into(),
            },
            kind: ItemKind::Episode,
            metadata: ItemMetadata::default(),
            artwork: Default::default(),
            user: UserItemState::default(),
            hierarchy: ItemHierarchy {
                series_name: series.map(str::to_string),
                parent_index: season,
                index: episode,
                ..ItemHierarchy::default()
            },
            media: TechnicalMedia::default(),
            people: Vec::new(),
            chapters: Vec::new(),
        }
    }

    #[test]
    fn episode_context_joins_series_and_number() {
        assert_eq!(
            episode_context(&item(Some("Harbor Lights"), Some(2), Some(5))).as_deref(),
            Some("Harbor Lights · S2 E5")
        );
        assert_eq!(
            episode_context(&item(None, None, None)),
            None,
            "a movie has no episode line"
        );
    }

    #[test]
    fn resume_follows_the_shipping_tail() {
        assert_eq!(resume_start(Duration::ZERO, None), None);
        assert_eq!(
            resume_start(Duration::from_secs(90), Some(Duration::from_secs(3600))),
            Some(Duration::from_secs(90))
        );
        assert_eq!(
            resume_start(Duration::from_secs(100), None),
            Some(Duration::from_secs(100)),
            "unknown duration keeps the resume until the file reports one"
        );
        let runtime = Duration::from_secs(120);
        assert_eq!(
            resume_start(runtime - Duration::from_secs(20), Some(runtime)),
            None,
            "inside the last 30 seconds starts over"
        );
    }

    #[test]
    fn credential_queries_and_messages_are_stripped() {
        let url = strip_credential_query(
            "https://jellyfin.local/Videos/abc/stream?Static=true&api_key=SECRET&DeviceId=desk",
        );
        assert!(!url.contains("SECRET"));
        assert!(url.contains("Static=true"));
        assert!(url.contains("DeviceId=desk"));
        let message =
            redact_message("failed Token=\"SECRET\" api_key=SECRET https://host/v?token=SECRET");
        assert!(!message.contains("SECRET"));
        assert!(message.contains("redacted"));
    }
}
