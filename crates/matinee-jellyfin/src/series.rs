//! Season, episode, next-up, and following-episode reads.
//!
//! Order is the server's order. A missing episode is [`None`], not an error.

use matinee_core::{ItemId, MediaItem};

use crate::client::{Endpoint, JellyfinClient};
use crate::error::JellyfinError;
use crate::query::{episodes_path, following_path, next_up_path, seasons_path};
use crate::transport::Transport;

impl<T: Transport> JellyfinClient<T> {
    pub async fn series_seasons(
        &self,
        series_id: &ItemId,
    ) -> Result<Vec<MediaItem>, JellyfinError> {
        let user_id = self.session().user().id().as_str();
        self.items(Endpoint::Series, &seasons_path(user_id, series_id))
            .await
    }

    pub async fn season_episodes(
        &self,
        series_id: &ItemId,
        season_id: &ItemId,
    ) -> Result<Vec<MediaItem>, JellyfinError> {
        let user_id = self.session().user().id().as_str();
        self.items(
            Endpoint::Series,
            &episodes_path(user_id, series_id, season_id),
        )
        .await
    }

    pub async fn next_up_episode(
        &self,
        series_id: &ItemId,
    ) -> Result<Option<MediaItem>, JellyfinError> {
        let user_id = self.session().user().id().as_str();
        let items = self
            .items(Endpoint::Series, &next_up_path(user_id, series_id))
            .await?;
        Ok(items.into_iter().next())
    }

    pub async fn following_episode(
        &self,
        episode: &MediaItem,
    ) -> Result<Option<MediaItem>, JellyfinError> {
        let Some(series_id) = episode.hierarchy.series_id.as_ref() else {
            return Ok(None);
        };
        let user_id = self.session().user().id().as_str();
        let items = self
            .items(
                Endpoint::Series,
                &following_path(user_id, series_id, episode.id()),
            )
            .await?;
        Ok(items.into_iter().find(|item| item.id() != episode.id()))
    }
}
