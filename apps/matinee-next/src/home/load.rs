//! Home requests on the service runtime.
//!
//! Each [`Request`] is one shelf and one HTTP call. The screen starts them
//! all at once; nothing here waits for another shelf. Server errors become
//! typed failures; no Jellyfin text reaches the screen.

use matinee_jellyfin::{JellyfinClient, Transport};

use super::model::{HomeFailure, Request, Response};

pub(crate) async fn run<T: Transport>(client: &JellyfinClient<T>, request: Request) -> Response {
    client
        .home_shelf(request.shelf)
        .await
        .map_err(|error| HomeFailure::from_error(&error))
}
