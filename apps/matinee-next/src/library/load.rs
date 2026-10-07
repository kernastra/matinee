//! Library requests on the service runtime.
//!
//! Each [`Request`] is one HTTP call. Server errors become typed failures;
//! no Jellyfin text reaches the screen.

use matinee_jellyfin::{JellyfinClient, Transport};

use super::model::{LibraryFailure, Request, Response};

pub(crate) async fn run<T: Transport>(client: &JellyfinClient<T>, request: Request) -> Response {
    let fail = |error: matinee_jellyfin::JellyfinError| LibraryFailure::from_error(&error);
    match request {
        Request::Page { query, page, .. } => {
            Response::Page(client.library_page(&query, page).await.map_err(fail))
        }
        Request::Views { .. } => Response::Views(client.library_views().await.map_err(fail)),
        Request::Genres { kind, view, .. } => Response::Genres(
            client
                .library_genres(kind, view.as_ref())
                .await
                .map_err(fail),
        ),
        Request::Item { id, .. } => {
            Response::Item(client.item_details(&id).await.map(Box::new).map_err(fail))
        }
    }
}
