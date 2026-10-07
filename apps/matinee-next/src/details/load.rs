//! Details requests on the service runtime.
//!
//! Each function answers one [`Request`] with a [`Response`]. Server errors
//! become typed failures here; no Jellyfin text reaches the screen.
//!
//! An authorization failure on any request, primary or secondary, is not a
//! content failure: the answer is [`SessionEnded`] and the screen reports it
//! to the shell instead of showing "unavailable" in a section.

use matinee_jellyfin::{JellyfinClient, JellyfinError, Transport};

use super::model::{DetailsFailure, RELATED_LIMIT, Request, Response};
use crate::session::{SessionEnded, session_ended};

/// Answer `request`. The ticket stays with the caller.
pub(crate) async fn run<T: Transport>(
    client: &JellyfinClient<T>,
    request: &Request,
) -> Result<Response, SessionEnded> {
    Ok(match request {
        Request::Item { item, .. } => {
            let item = checked(client.item_details(item).await)?;
            Response::Item(item.map_err(|error| DetailsFailure::from_error(&error)))
        }
        Request::Related { item, .. } => {
            // Similar titles ask for one extra so the title itself can be left out.
            let (similar, collections) = futures::join!(
                client.similar_items(item, RELATED_LIMIT as u32 + 1),
                client.item_collections(item),
            );
            Response::Related {
                similar: checked(similar)?.ok(),
                collections: checked(collections)?.ok(),
            }
        }
        Request::Outline { series, .. } => {
            let (seasons, next_up) = futures::join!(
                client.series_seasons(series),
                client.next_up_episode(series),
            );
            Response::Outline {
                seasons: checked(seasons)?.ok(),
                // No next up is not a failure: the series may be finished.
                next_up: checked(next_up)?.ok().flatten(),
            }
        }
        Request::Episodes { series, season, .. } => {
            Response::Episodes(checked(client.season_episodes(series, season).await)?.ok())
        }
    })
}

/// Pass the answer through unless it says the session has ended.
fn checked<T>(result: Result<T, JellyfinError>) -> Result<Result<T, JellyfinError>, SessionEnded> {
    match result {
        Err(error) if session_ended(&error) => Err(SessionEnded),
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use matinee_core::ItemId;

    use super::*;
    use crate::runtime::ServiceRuntime;
    use crate::test_support::{Reply, client, fake_jellyfin};

    fn answer(replies: Vec<Reply>, request: Request) -> Result<Response, SessionEnded> {
        let (address, _seen) = fake_jellyfin(replies);
        let client = client(&address);
        let runtime = ServiceRuntime::new().unwrap();
        let (_task, rx) = runtime.spawn(async move { run(client.as_ref(), &request).await });
        rx.blocking_recv().unwrap()
    }

    fn related() -> Request {
        let (mut model, requests) =
            super::super::model::DetailsModel::open(ItemId::parse("movie-1").unwrap());
        let movie = matinee_core::MediaItem {
            identity: matinee_core::ItemIdentity {
                id: ItemId::parse("movie-1").unwrap(),
                name: "Movie".into(),
            },
            kind: matinee_core::ItemKind::Movie,
            metadata: Default::default(),
            artwork: Default::default(),
            user: Default::default(),
            hierarchy: Default::default(),
            media: Default::default(),
            people: Vec::new(),
            chapters: Vec::new(),
        };
        model
            .apply(requests[0].ticket(), Response::Item(Ok(movie)))
            .remove(0)
    }

    const ITEMS: &[u8] = br#"{"Items":[]}"#;

    #[test]
    fn an_unauthorized_title_ends_the_session_not_the_screen() {
        let item = Request::Item {
            item: ItemId::parse("movie-1").unwrap(),
            ticket: related().ticket(),
        };
        assert_eq!(
            answer(vec![Reply::Status(401, Vec::new())], item.clone()),
            Err(SessionEnded)
        );
        assert_eq!(
            answer(vec![Reply::Status(403, Vec::new())], item.clone()),
            Err(SessionEnded)
        );
        // Not found is still the title's own failure.
        assert_eq!(
            answer(vec![Reply::Status(404, Vec::new())], item),
            Ok(Response::Item(Err(DetailsFailure::NotFound)))
        );
    }

    #[test]
    fn a_secondary_401_ends_the_session_but_other_failures_stay_local() {
        // Similar answers 401 while collections succeed: not "unavailable".
        let answered = answer(
            vec![
                Reply::Status(401, Vec::new()),
                Reply::Status(200, ITEMS.to_vec()),
            ],
            related(),
        );
        assert_eq!(answered, Err(SessionEnded));
        // A server error on one secondary request is that section's failure.
        let answered = answer(
            vec![
                Reply::Status(500, Vec::new()),
                Reply::Status(200, ITEMS.to_vec()),
            ],
            related(),
        )
        .unwrap();
        let Response::Related {
            similar,
            collections,
        } = answered
        else {
            panic!("related");
        };
        assert!(
            similar.is_none() ^ collections.is_none(),
            "one failed, one answered"
        );
    }
}
