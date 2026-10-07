//! Details requests on the service runtime.
//!
//! Each function answers one [`Request`] with a [`Response`]. Server errors
//! become typed failures here; no Jellyfin text reaches the screen.

use matinee_jellyfin::{JellyfinClient, Transport};

use super::model::{DetailsFailure, RELATED_LIMIT, Request, Response};

/// Answer `request`. The ticket stays with the caller.
pub(crate) async fn run<T: Transport>(client: &JellyfinClient<T>, request: &Request) -> Response {
    match request {
        Request::Item { item, .. } => Response::Item(
            client
                .item_details(item)
                .await
                .map_err(|error| DetailsFailure::from_error(&error)),
        ),
        Request::Related { item, .. } => {
            // Similar titles ask for one extra so the title itself can be left out.
            let (similar, collections) = futures::join!(
                client.similar_items(item, RELATED_LIMIT as u32 + 1),
                client.item_collections(item),
            );
            Response::Related {
                similar: similar.ok(),
                collections: collections.ok(),
            }
        }
        Request::Outline { series, .. } => {
            let (seasons, next_up) = futures::join!(
                client.series_seasons(series),
                client.next_up_episode(series),
            );
            Response::Outline {
                seasons: seasons.ok(),
                // No next up is not a failure: the series may be finished.
                next_up: next_up.ok().flatten(),
            }
        }
        Request::Episodes { series, season, .. } => {
            Response::Episodes(client.season_episodes(series, season).await.ok())
        }
    }
}
