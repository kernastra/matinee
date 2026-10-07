//! Search requests on the service runtime.
//!
//! Each [`Request`] is one HTTP call. Server errors become typed failures;
//! no Jellyfin text reaches the screen, and the query text is never logged.

use matinee_jellyfin::{JellyfinClient, Transport};

use super::model::{Request, Response, SearchFailure};

pub(crate) async fn run<T: Transport>(client: &JellyfinClient<T>, request: Request) -> Response {
    let fail = |error: matinee_jellyfin::JellyfinError| SearchFailure::from_error(&error);
    match request {
        Request::Page { query, page, .. } => {
            Response::Page(client.search_page(&query, page, None).await.map_err(fail))
        }
        Request::Item { id, .. } => {
            Response::Item(client.item_details(&id).await.map(Box::new).map_err(fail))
        }
    }
}
