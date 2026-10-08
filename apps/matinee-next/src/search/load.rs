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

#[cfg(test)]
mod tests {
    //! The HTTP boundary: the real client and transport on the service
    //! runtime, against a loopback stand-in for Jellyfin.

    use matinee_core::{ItemKind, LibraryPage};

    use super::*;
    use crate::runtime::ServiceRuntime;
    use crate::search::model::{PAGE_SIZE, SearchModel};
    use crate::test_support::{FIXTURE_TOKEN, Reply, client, fake_jellyfin};

    /// The first page the model sends for `text`.
    fn first_page(text: &str) -> Request {
        let mut model = SearchModel::new();
        let wait = model.type_text(text).expect("a query");
        model.debounced(wait).remove(0)
    }

    fn answer(replies: Vec<Reply>, request: Request) -> (Response, Vec<String>) {
        let (address, seen) = fake_jellyfin(replies);
        let client = client(&address);
        let runtime = ServiceRuntime::new().unwrap();
        let (_task, rx) = runtime.spawn(async move { run(client.as_ref(), request).await });
        let response = rx.blocking_recv().unwrap();
        (response, seen.try_iter().collect())
    }

    const PAGE: &str = r#"{"Items":[
        {"Id":"movie-1","Name":"Harbor","Type":"Movie","ProductionYear":1999,
         "ImageTags":{"Primary":"tag-1"},"UserData":{"Played":true}},
        {"Id":"episode-1","Name":"Pilot","Type":"Episode","SeriesId":"series-1",
         "SeriesName":"Harbor Lights","IndexNumber":1,"ParentIndexNumber":2}
    ],"TotalRecordCount":61,"StartIndex":0}"#;

    #[test]
    fn a_page_goes_over_http_with_the_header_and_comes_back_typed() {
        let (response, seen) = answer(
            vec![Reply::Status(200, PAGE.as_bytes().to_vec())],
            first_page("  Harbor & Co #2 é "),
        );
        let Response::Page(Ok(LibraryPage {
            items,
            start,
            total,
        })) = response
        else {
            panic!("a page: {response:?}");
        };
        assert_eq!((start, total), (0, Some(61)));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name(), "Harbor");
        assert!(items[0].user.is_played());
        assert_eq!(items[1].kind, ItemKind::Episode);
        assert_eq!(
            items[1].hierarchy.series_name.as_deref(),
            Some("Harbor Lights")
        );
        assert_eq!(items[1].hierarchy.episode_label().as_deref(), Some("S2 E1"));

        let head = seen[0].lines().next().unwrap().to_string();
        assert!(
            head.starts_with("GET /Users/user-1/Items?"),
            "scoped to the user: {head}"
        );
        // The term is trimmed and encoded as one value: `&` and `#` cannot
        // start another parameter or a fragment.
        assert!(
            head.contains("SearchTerm=Harbor%20%26%20Co%20%232%20%C3%A9&"),
            "{head}"
        );
        for pair in [
            "Recursive=true",
            "IncludeItemTypes=Movie%2CSeries%2CEpisode",
            "StartIndex=0",
            &format!("Limit={PAGE_SIZE}"),
            "EnableTotalRecordCount=true",
            "EnableUserData=true",
            "ImageTypeLimit=1",
        ] {
            assert!(head.contains(pair), "{pair} in {head}");
        }
        // The token travels in the header, never in the address.
        assert!(!head.contains(FIXTURE_TOKEN), "{head}");
        assert!(!head.to_ascii_lowercase().contains("api_key"), "{head}");
        assert!(
            seen[0].lines().any(
                |line| line.to_ascii_lowercase().starts_with("authorization:")
                    && line.contains(FIXTURE_TOKEN)
            ),
            "the session header"
        );
    }

    #[test]
    fn failures_are_typed_and_carry_no_server_text() {
        let page = || first_page("harbor");
        let (response, _) = answer(vec![Reply::Status(401, Vec::new())], page());
        assert_eq!(response, Response::Page(Err(SearchFailure::SignedOut)));
        let (response, _) = answer(vec![Reply::Status(403, Vec::new())], page());
        assert_eq!(response, Response::Page(Err(SearchFailure::SignedOut)));
        let (response, _) = answer(
            vec![Reply::Status(
                500,
                b"SearchTerm=harbor stack trace".to_vec(),
            )],
            page(),
        );
        assert_eq!(response, Response::Page(Err(SearchFailure::Unreadable)));
        let (response, _) = answer(vec![Reply::Status(200, b"{not json".to_vec())], page());
        assert_eq!(response, Response::Page(Err(SearchFailure::Unreadable)));
        let (response, _) = answer(vec![Reply::Status(200, b"{}".to_vec())], page());
        assert_eq!(
            response,
            Response::Page(Err(SearchFailure::Unreadable)),
            "no Items is malformed, not empty"
        );
        let (response, _) = answer(
            vec![Reply::Status(200, br#"{"Items":[]}"#.to_vec())],
            page(),
        );
        assert_eq!(
            response,
            Response::Page(Ok(LibraryPage {
                items: Vec::new(),
                start: 0,
                total: None,
            })),
            "an empty page is zero matches"
        );
    }

    #[test]
    fn a_server_that_is_not_there_is_unreachable() {
        // Bind and release a port, so nothing listens on it.
        let address = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", listener.local_addr().unwrap())
        };
        let client = client(&address);
        let runtime = ServiceRuntime::new().unwrap();
        let request = first_page("harbor");
        let (_task, rx) = runtime.spawn(async move { run(client.as_ref(), request).await });
        assert_eq!(
            rx.blocking_recv().unwrap(),
            Response::Page(Err(SearchFailure::Unreachable))
        );
    }

    #[test]
    fn reconcile_asks_for_the_one_title() {
        let mut model = SearchModel::new();
        let wait = model.type_text("harbor").unwrap();
        let page = model.debounced(wait).remove(0);
        let (response, _) = answer(
            vec![Reply::Status(200, PAGE.as_bytes().to_vec())],
            page.clone(),
        );
        model.apply(&page, response);
        model.note_opened(model.items()[0].id().clone());
        let item = model.reconcile().expect("one title");
        let (response, seen) = answer(
            vec![Reply::Status(
                200,
                br#"{"Id":"movie-1","Name":"Harbor","Type":"Movie","UserData":{"PlaybackPositionTicks":6000000000}}"#
                    .to_vec(),
            )],
            item,
        );
        let Response::Item(Ok(updated)) = response else {
            panic!("one title");
        };
        assert!(updated.is_resumable());
        assert!(
            seen[0].starts_with("GET /Users/user-1/Items/movie-1"),
            "{}",
            seen[0].lines().next().unwrap()
        );
    }

    #[test]
    fn an_aborted_page_stops_waiting_and_answers_nothing() {
        let (address, seen) = fake_jellyfin(vec![Reply::Hang]);
        let client = client(&address);
        let runtime = ServiceRuntime::new().unwrap();
        let request = first_page("harbor");
        let (task, rx) = runtime.spawn(async move { run(client.as_ref(), request).await });
        // The server has the request; it never answers.
        seen.recv_timeout(std::time::Duration::from_secs(5))
            .expect("the request reached the server");
        task.abort();
        assert!(
            rx.blocking_recv().is_err(),
            "an aborted request delivers nothing to apply"
        );
    }
}
