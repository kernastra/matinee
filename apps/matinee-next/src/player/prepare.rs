//! Ask Jellyfin for one item and a playback plan.
//!
//! Source selection stays in `matinee-jellyfin`. This function does not invent
//! a stream URL.

use std::time::Duration;

use matinee_core::{ItemId, MediaItem, PlaybackOptions};
use matinee_jellyfin::{JellyfinClient, JellyfinError, ReqwestTransport};

use super::model::{PlanFailure, PreparedPlayback};

pub(crate) async fn prepare_playback(
    client: &JellyfinClient<ReqwestTransport>,
    item_id: ItemId,
) -> Result<PreparedPlayback, PlanFailure> {
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
    fn player_messages_use_the_jellyfin_redaction() {
        let message = matinee_jellyfin::redact_freeform(
            "failed Token=\"SECRET\" api_key=SECRET https://host/v?token=SECRET",
        );
        assert!(!message.contains("SECRET"));
        assert!(message.contains("redacted"));
    }
}
