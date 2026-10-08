//! Behavior tests against an in-memory transport. No sockets.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use matinee_core::{
    ImageRole, ImageTag, ItemHierarchy, ItemId, ItemIdentity, ItemKind, ItemMetadata, LibraryKind,
    LibraryPageRequest, LibrarySort, MediaItem, PlaySessionId, PlaybackMethod, PlaybackOptions,
    PlaybackPlan, PlaybackReport, ReportKind, SearchQuery, StreamAuthorization, TechnicalMedia,
    User, UserId, UserItemState,
};
use serde_json::Value;
use url::Url;

use crate::{
    CLIENT_VERSION, CancelFlag, DEVICE_ID, HttpRequest, HttpResponse, JellyfinClient,
    JellyfinError, Method, Password, Session, Transport, TransportError, authenticate,
};

fn wait<T>(future: impl Future<Output = T>) -> T {
    futures::executor::block_on(future)
}

fn user() -> User {
    User::new(UserId::parse("user-1").unwrap(), "Sean", None)
}

fn session() -> Session {
    Session::new("http://jellyfin.local:8096", "token with spaces", user()).unwrap()
}

fn bare_item(id: &str, name: &str, kind: ItemKind) -> MediaItem {
    MediaItem {
        identity: ItemIdentity {
            id: ItemId::parse(id).unwrap(),
            name: name.to_string(),
        },
        kind,
        metadata: ItemMetadata::default(),
        artwork: Default::default(),
        user: UserItemState::default(),
        hierarchy: ItemHierarchy::default(),
        media: TechnicalMedia::new(Vec::new(), Vec::new()),
        people: Vec::new(),
        chapters: Vec::new(),
    }
}

type Handler = dyn Fn(&HttpRequest) -> Result<HttpResponse, TransportError> + Send + Sync;

struct Mock {
    calls: Arc<Mutex<Vec<HttpRequest>>>,
    handler: Arc<Handler>,
}

impl Mock {
    fn new(
        handler: impl Fn(&HttpRequest) -> Result<HttpResponse, TransportError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            handler: Arc::new(handler),
        }
    }
}

impl Transport for Mock {
    fn send(
        &self,
        request: HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send {
        let result = (self.handler)(&request);
        self.calls.lock().expect("calls").push(request);
        async move { result }
    }
}

fn ok_json(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        body: body.as_bytes().to_vec(),
    }
}

fn items_body(entries: &[&str]) -> String {
    format!(r#"{{"Items":[{}]}}"#, entries.join(","))
}

fn item_json(id: &str, name: &str, kind: &str) -> String {
    format!(r#"{{"Id":"{id}","Name":"{name}","Type":"{kind}"}}"#)
}

fn query_value(request: &HttpRequest, key: &str) -> Option<String> {
    Url::parse(&request.url).ok().and_then(|url| {
        url.query_pairs()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.into_owned())
    })
}

fn path_of(request: &HttpRequest) -> String {
    Url::parse(&request.url).unwrap().path().to_string()
}

fn auth_header(request: &HttpRequest) -> String {
    request
        .headers
        .iter()
        .find(|(name, _)| name == "Authorization")
        .unwrap()
        .1
        .clone()
}

#[test]
fn authorization_header_uses_the_fixed_device_id() {
    let header = session().authorization_header().unwrap();
    assert!(header.starts_with("MediaBrowser "));
    assert!(header.contains("Client=\"Matinee\""));
    assert!(header.contains("Device=\"Desktop\""));
    assert!(header.contains(&format!("DeviceId=\"{DEVICE_ID}\"")));
    assert!(header.contains(&format!("Version=\"{CLIENT_VERSION}\"")));
    assert!(header.contains("Token=\"token with spaces\""));
    assert!(!format!("{:?}", session()).contains("token with spaces"));
    assert!(Session::new("http://jellyfin.local:8096", "bad\ntoken", user()).is_err());
    assert!(!format!("{:?}", Password::new("hunter2")).contains("hunter2"));
    let request = HttpRequest {
        method: Method::Post,
        url: "http://jellyfin.local:8096/Users/AuthenticateByName?api_key=super-secret".into(),
        headers: vec![(
            "Authorization".into(),
            "MediaBrowser Token=\"super-secret\"".into(),
        )],
        body: Some("{\"Pw\":\"hunter2\"}".into()),
        cancel: None,
    };
    let rendered = format!("{request:?}");
    assert!(rendered.contains("<omitted>"));
    assert!(rendered.contains("redacted"));
    assert!(!rendered.contains("super-secret"));
    assert!(!rendered.contains("hunter2"));
}

#[test]
fn image_urls_encode_the_token() {
    let current = session();
    let item = bare_item("movie-1", "Movie", ItemKind::Movie);
    assert!(!item.artwork.has_primary());
    assert_eq!(
        current.artwork().image(item.id(), ImageRole::Primary, 360),
        "http://jellyfin.local:8096/Items/movie-1/Images/Primary?maxWidth=360&quality=90&api_key=token%20with%20spaces"
    );
    let native = current
        .artwork()
        .image_request(item.id(), ImageRole::Primary, 360);
    assert_eq!(
        native.url,
        "http://jellyfin.local:8096/Items/movie-1/Images/Primary?maxWidth=360&quality=90"
    );
    assert!(!native.url.contains("api_key"));
    assert!(
        current
            .artwork()
            .backdrop(&item, 1280)
            .contains("/Images/Primary?")
    );
    let avatar = Session::new(
        "http://jellyfin.local:8096",
        "token with spaces",
        User::new(
            UserId::parse("user-1").unwrap(),
            "Sean",
            ImageTag::parse("avatar tag"),
        ),
    )
    .unwrap();
    let user_image = Url::parse(&avatar.artwork().user_image(96)).unwrap();
    assert_eq!(user_image.path(), "/Users/user-1/Images/Primary");
    assert_eq!(pair(&user_image, "tag").as_deref(), Some("avatar tag"));
    assert_eq!(
        pair(&user_image, "api_key").as_deref(),
        Some("token with spaces")
    );
    let chapter = Url::parse(&current.artwork().chapter_image(
        item.id(),
        3,
        480,
        ImageTag::parse("chapter-tag").as_ref(),
    ))
    .unwrap();
    assert_eq!(chapter.path(), "/Items/movie-1/Images/Chapter/3");
    assert_eq!(pair(&chapter, "maxWidth").as_deref(), Some("480"));
    assert_eq!(pair(&chapter, "quality").as_deref(), Some("88"));
    assert_eq!(pair(&chapter, "tag").as_deref(), Some("chapter-tag"));
    let indexed = Url::parse(&current.artwork().backdrop_image(
        item.id(),
        1,
        ImageTag::parse("back").as_ref(),
        800,
    ))
    .unwrap();
    assert_eq!(indexed.path(), "/Items/movie-1/Images/Backdrop/1");
    assert_eq!(pair(&indexed, "tag").as_deref(), Some("back"));
}

fn pair(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

#[test]
fn authentication_succeeds_and_maps_errors() {
    let transport = Mock::new(|request| {
        assert_eq!(request.method, Method::Post);
        assert!(path_of(request).ends_with("/Users/AuthenticateByName"));
        assert!(!auth_header(request).contains("Token="));
        assert!(auth_header(request).contains("DeviceId=\"matinee-desktop\""));
        let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["Username"], "Sean");
        assert_eq!(body["Pw"], "hunter2");
        assert!(!format!("{request:?}").contains("hunter2"));
        Ok(ok_json(
            r#"{"AccessToken":"issued-token","User":{"Id":"user-1","Name":"Sean"}}"#,
        ))
    });
    let signed_in = wait(authenticate(
        &transport,
        "jellyfin.local:8096",
        "Sean",
        Password::new("hunter2"),
    ))
    .unwrap();
    assert_eq!(signed_in.server_url(), "http://jellyfin.local:8096");
    assert_eq!(signed_in.user().name(), "Sean");
    assert_eq!(signed_in.access_token(), "issued-token");

    let rejected = Mock::new(|_| {
        Ok(HttpResponse {
            status: 401,
            body: Vec::new(),
        })
    });
    let error = wait(authenticate(
        &rejected,
        "http://jellyfin.local:8096",
        "Sean",
        Password::new("nope"),
    ))
    .unwrap_err();
    assert!(matches!(error, JellyfinError::AuthRejected));
    assert_eq!(
        error.to_string(),
        "That username or password was not accepted."
    );

    let missing = Mock::new(|_| {
        Ok(HttpResponse {
            status: 404,
            body: Vec::new(),
        })
    });
    assert!(matches!(
        wait(authenticate(
            &missing,
            "http://jellyfin.local:8096",
            "Sean",
            Password::new("nope")
        ))
        .unwrap_err(),
        JellyfinError::NotFound
    ));

    let down = Mock::new(|_| {
        Err(TransportError::Unreachable(
            "connect api_key=super-secret".into(),
        ))
    });
    let error = wait(authenticate(
        &down,
        "http://jellyfin.local:8096",
        "Sean",
        Password::new("nope"),
    ))
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Could not reach Jellyfin. Check the server address and try again."
    );
    assert!(!error.context().unwrap_or("").contains("super-secret"));

    let broken = Mock::new(|_| {
        Ok(HttpResponse {
            status: 500,
            body: b"Pw=hunter2".to_vec(),
        })
    });
    let error = wait(authenticate(
        &broken,
        "http://jellyfin.local:8096",
        "Sean",
        Password::new("hunter2"),
    ))
    .unwrap_err();
    match error {
        JellyfinError::Server { status: 500, .. } => {}
        other => panic!("unexpected {other}"),
    }
    assert!(!error.to_string().contains("hunter2"));

    let garbled = Mock::new(|_| Ok(ok_json("[]")));
    assert!(matches!(
        wait(authenticate(
            &garbled,
            "http://jellyfin.local:8096",
            "Sean",
            Password::new("x")
        ))
        .unwrap_err(),
        JellyfinError::Malformed { .. }
    ));
}

#[test]
fn server_info_uses_the_public_endpoint() {
    let transport = Mock::new(|request| {
        assert_eq!(path_of(request), "/System/Info/Public");
        assert!(auth_header(request).contains("Token=\"token with spaces\""));
        Ok(ok_json(
            r#"{"ServerName":"Andromeda","Version":"10.10.7","OperatingSystem":"Linux","ProductName":"Jellyfin Server","Id":"server-1"}"#,
        ))
    });
    let client = JellyfinClient::new(session(), transport);
    let info = wait(client.server_info()).unwrap();
    assert_eq!(info.name.as_deref(), Some("Andromeda"));
    assert_eq!(info.version.as_deref(), Some("10.10.7"));
    assert_eq!(info.operating_system.as_deref(), Some("Linux"));
    assert_eq!(info.product.as_deref(), Some("Jellyfin Server"));
    assert_eq!(info.id.as_deref(), Some("server-1"));
}

#[test]
fn home_feed_keeps_going_when_top_rated_or_favorites_fail() {
    let transport = Mock::new(|request| {
        assert_eq!(request.method, Method::Get);
        if path_of(request).ends_with("/Resume") {
            return Ok(ok_json(&items_body(&[&item_json(
                "resume", "Resume", "Movie",
            )])));
        }
        if query_value(request, "SortBy").as_deref() == Some("CommunityRating") {
            return Ok(HttpResponse {
                status: 500,
                body: Vec::new(),
            });
        }
        if query_value(request, "Filters").as_deref() == Some("IsFavorite") {
            return Ok(HttpResponse {
                status: 401,
                body: Vec::new(),
            });
        }
        Ok(ok_json(&items_body(&[&item_json("row", "Row", "Series")])))
    });
    let client = JellyfinClient::new(session(), transport);
    let feed = wait(client.home_feed()).unwrap();
    assert_eq!(feed.resume[0].name(), "Resume");
    assert_eq!(feed.latest.len(), 1);
    assert_eq!(feed.movies.len(), 1);
    assert_eq!(feed.series.len(), 1);
    assert!(feed.top_rated.is_empty());
    assert!(feed.favorites.is_empty());
    assert_eq!(client.session().server_url(), "http://jellyfin.local:8096");
}

#[test]
fn home_shelves_are_separate_requests_with_typed_failures() {
    use matinee_core::HomeShelf;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    let transport = Mock::new(move |request| {
        record.lock().expect("seen").push(request.url.clone());
        if path_of(request) == "/Shows/NextUp" {
            return Ok(HttpResponse {
                status: 500,
                body: b"boom".to_vec(),
            });
        }
        if path_of(request).ends_with("/Items/Resume") {
            return Ok(HttpResponse {
                status: 401,
                body: Vec::new(),
            });
        }
        Ok(ok_json(&items_body(&[&item_json("m1", "One", "Movie")])))
    });
    let client = JellyfinClient::new(session(), transport);
    assert!(matches!(
        wait(client.home_shelf(HomeShelf::ContinueWatching)).unwrap_err(),
        JellyfinError::Unauthorized
    ));
    assert!(matches!(
        wait(client.home_shelf(HomeShelf::NextUp)).unwrap_err(),
        JellyfinError::Server { status: 500, .. }
    ));
    for shelf in [
        HomeShelf::RecentMovies,
        HomeShelf::RecentSeries,
        HomeShelf::Favorites,
    ] {
        assert_eq!(wait(client.home_shelf(shelf)).unwrap().len(), 1);
    }
    let urls: Vec<Url> = seen
        .lock()
        .expect("seen")
        .iter()
        .map(|value| Url::parse(value).unwrap())
        .collect();
    assert_eq!(urls.len(), 5, "one request per shelf");
    let next_up = urls
        .iter()
        .find(|url| url.path() == "/Shows/NextUp")
        .unwrap();
    assert_eq!(pair(next_up, "seriesId"), None, "every series, not one");
    assert_eq!(pair(next_up, "enableResumable").as_deref(), Some("false"));
    assert_eq!(pair(next_up, "limit").as_deref(), Some("12"));
    // Jellyfin's `TvShowsController.GetNextUp` accepts `enableResumable`
    // (default true). False drops a series whose next episode has a saved
    // position; that episode is exactly what `/Items/Resume` lists for
    // Continue Watching, so nothing is lost between the two rows.
    assert_eq!(pair(next_up, "userId").as_deref(), Some("user-1"));
    assert_eq!(pair(next_up, "enableUserData").as_deref(), Some("true"));
    assert_eq!(pair(next_up, "enableImages").as_deref(), Some("true"));
    assert!(pair(next_up, "fields").is_some_and(|fields| fields.contains("Overview")));
    assert_eq!(pair(next_up, "enableRewatching"), None, "never rewatches");
    assert_eq!(pair(next_up, "disableFirstEpisode"), None, "server default");
    // The per-series Next Up used by Details keeps resumable episodes.
    let series = ItemId::parse("series-1").unwrap();
    let path = crate::query::next_up_path("user-1", &series);
    let url = Url::parse(&format!("http://h{path}")).unwrap();
    assert_eq!(pair(&url, "seriesId").as_deref(), Some("series-1"));
    assert_eq!(pair(&url, "enableResumable").as_deref(), Some("true"));
    assert!(urls.iter().any(|url| {
        pair(url, "IncludeItemTypes").as_deref() == Some("Movie")
            && pair(url, "SortBy").as_deref() == Some("DateCreated")
    }));
    assert!(urls.iter().any(|url| {
        pair(url, "IncludeItemTypes").as_deref() == Some("Series")
            && pair(url, "SortBy").as_deref() == Some("DateCreated")
    }));
    assert!(
        urls.iter()
            .any(|url| pair(url, "Filters").as_deref() == Some("IsFavorite"))
    );
    for url in &urls {
        assert!(!url.as_str().contains("api_key"), "{url}");
        assert!(!url.as_str().contains("token"), "{url}");
    }
}

#[test]
fn home_feed_issues_six_shipping_queries() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    let transport = Mock::new(move |request| {
        record.lock().expect("seen").push(request.url.clone());
        Ok(ok_json(&items_body(&[])))
    });
    let client = JellyfinClient::new(session(), transport);
    let feed = wait(client.home_feed()).unwrap();
    assert!(feed.is_empty());
    let urls: Vec<Url> = seen
        .lock()
        .expect("seen")
        .iter()
        .map(|value| Url::parse(value).unwrap())
        .collect();
    assert_eq!(urls.len(), 6);
    assert!(urls.iter().any(|url| url.path().ends_with("/Items/Resume")));
    assert!(
        urls.iter()
            .any(|url| pair(url, "SortBy").as_deref() == Some("CommunityRating"))
    );
    assert!(
        urls.iter()
            .any(|url| pair(url, "Filters").as_deref() == Some("IsFavorite"))
    );
    assert!(urls.iter().any(|url| {
        pair(url, "IncludeItemTypes").as_deref() == Some("Movie,Series")
            && pair(url, "SortBy").as_deref() == Some("DateCreated")
            && pair(url, "SortOrder").as_deref() == Some("Descending")
            && pair(url, "Limit").as_deref() == Some("6")
    }));
}

#[test]
fn a_required_home_shelf_failure_is_not_an_empty_feed() {
    let transport = Mock::new(|request| {
        if path_of(request).ends_with("/Resume") {
            Ok(HttpResponse {
                status: 500,
                body: Vec::new(),
            })
        } else {
            Ok(ok_json(&items_body(&[])))
        }
    });
    let client = JellyfinClient::new(session(), transport);
    assert!(matches!(
        wait(client.home_feed()).unwrap_err(),
        JellyfinError::Server { status: 500, .. }
    ));
}

#[test]
fn latest_movies_and_series_failures_fail_the_feed() {
    for kind in ["latest", "movies", "series"] {
        let kind = kind.to_string();
        let label = kind.clone();
        let transport = Mock::new(move |request| {
            let url = Url::parse(&request.url).unwrap();
            let types = pair(&url, "IncludeItemTypes");
            let limit = pair(&url, "Limit");
            let failed = match label.as_str() {
                "latest" => {
                    types.as_deref() == Some("Movie,Series") && limit.as_deref() == Some("6")
                }
                "movies" => types.as_deref() == Some("Movie") && limit.as_deref() == Some("12"),
                "series" => types.as_deref() == Some("Series"),
                _ => false,
            };
            if failed {
                Ok(HttpResponse {
                    status: 500,
                    body: Vec::new(),
                })
            } else {
                Ok(ok_json(&items_body(&[])))
            }
        });
        let client = JellyfinClient::new(session(), transport);
        assert!(
            matches!(
                wait(client.home_feed()).unwrap_err(),
                JellyfinError::Server { status: 500, .. }
            ),
            "{kind} must fail the feed"
        );
    }
}

#[test]
fn library_sorts_are_typed() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    let transport = Mock::new(move |request| {
        record.lock().expect("seen").push(request.url.clone());
        Ok(ok_json(&items_body(&[])))
    });
    let client = JellyfinClient::new(session(), transport);
    for sort in [
        LibrarySort::Name,
        LibrarySort::DateCreated,
        LibrarySort::ProductionYear,
        LibrarySort::CommunityRating,
    ] {
        wait(client.library_items(LibraryKind::Movies, sort)).unwrap();
    }
    let urls: Vec<Url> = seen
        .lock()
        .expect("seen")
        .iter()
        .map(|value| Url::parse(value).unwrap())
        .collect();
    assert!(urls.iter().all(|url| url.path() == "/Users/user-1/Items"));
    assert!(
        urls.iter()
            .all(|url| pair(url, "IncludeItemTypes").as_deref() == Some("Movie"))
    );
    assert_eq!(pair(&urls[0], "SortBy").as_deref(), Some("SortName"));
    assert_eq!(pair(&urls[0], "SortOrder").as_deref(), Some("Ascending"));
    assert_eq!(pair(&urls[1], "SortBy").as_deref(), Some("DateCreated"));
    assert_eq!(pair(&urls[1], "SortOrder").as_deref(), Some("Descending"));
    assert_eq!(pair(&urls[2], "SortBy").as_deref(), Some("ProductionYear"));
    assert_eq!(pair(&urls[3], "SortBy").as_deref(), Some("CommunityRating"));
    assert_eq!(pair(&urls[3], "Limit").as_deref(), Some("240"));
}

#[test]
fn details_similar_and_collections_preserve_order() {
    let transport = Mock::new(|request| {
        let path = path_of(request);
        if path == "/Users/user-1/Items/movie-1" {
            return Ok(ok_json(&item_json("movie-1", "Movie", "Movie")));
        }
        if path == "/Items/movie-1/Similar" {
            assert_eq!(query_value(request, "userId").as_deref(), Some("user-1"));
            assert_eq!(query_value(request, "limit").as_deref(), Some("8"));
            return Ok(ok_json(&items_body(&[
                &item_json("movie-2", "Sequel", "Movie"),
                &item_json("movie-3", "Prequel", "Movie"),
            ])));
        }
        if path == "/Items/movie-1/Collections" {
            return Ok(ok_json(&items_body(&[&item_json(
                "box-1",
                "The Trilogy",
                "BoxSet",
            )])));
        }
        assert_eq!(query_value(request, "ParentId").as_deref(), Some("box-1"));
        Ok(ok_json(&items_body(&[
            &item_json("movie-1", "Movie", "Movie"),
            &item_json("movie-2", "Sequel", "Movie"),
        ])))
    });
    let client = JellyfinClient::new(session(), transport);
    let id = ItemId::parse("movie-1").unwrap();
    assert_eq!(wait(client.item_details(&id)).unwrap().name(), "Movie");
    let similar = wait(client.similar_items(&id, 8)).unwrap();
    assert_eq!(similar[0].name(), "Sequel");
    assert_eq!(similar[1].name(), "Prequel");
    let collections = wait(client.item_collections(&id)).unwrap();
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].collection.name(), "The Trilogy");
    assert_eq!(collections[0].collection.kind, ItemKind::Collection);
    assert_eq!(collections[0].items[1].name(), "Sequel");
}

#[test]
fn clear_progress_writes_zeroes_and_keeps_the_rest() {
    let transport = Mock::new(|request| {
        if request.method == Method::Get {
            assert_eq!(path_of(request), "/UserItems/movie-1/UserData");
            return Ok(ok_json(
                r#"{"IsFavorite":true,"PlaybackPositionTicks":42,"PlayedPercentage":12,"LastPlayedDate":"2024-01-01T00:00:00Z","Played":true,"PlayCount":7,"CustomPluginField":{"Nested":"keep","Flag":null},"Rating":null}"#,
            ));
        }
        assert_eq!(request.method, Method::Post);
        let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["IsFavorite"], true);
        assert_eq!(body["PlaybackPositionTicks"], 0);
        assert_eq!(body["PlayedPercentage"], 0);
        assert_eq!(body["LastPlayedDate"], "2024-01-01T00:00:00Z");
        assert_eq!(body["Played"], true);
        assert_eq!(body["PlayCount"], 7);
        assert_eq!(body["CustomPluginField"]["Nested"], "keep");
        assert!(body["CustomPluginField"]["Flag"].is_null());
        assert!(body["Rating"].is_null());
        Ok(ok_json(""))
    });
    let client = JellyfinClient::new(session(), transport);
    wait(client.clear_item_progress(&ItemId::parse("movie-1").unwrap())).unwrap();
}

#[test]
fn search_pages_movies_series_and_episodes_on_the_server() {
    let transport = Mock::new(|request| {
        assert_eq!(query_value(request, "SearchTerm").as_deref(), Some("a&b=c"));
        assert_eq!(
            query_value(request, "IncludeItemTypes").as_deref(),
            Some("Movie,Series,Episode")
        );
        assert_eq!(query_value(request, "StartIndex").as_deref(), Some("40"));
        assert_eq!(query_value(request, "Limit").as_deref(), Some("20"));
        assert_eq!(
            query_value(request, "EnableTotalRecordCount").as_deref(),
            Some("true")
        );
        assert!(query_value(request, "b").is_none());
        Ok(ok_json(&format!(
            r#"{{"Items":[{}],"TotalRecordCount":57,"StartIndex":40}}"#,
            item_json("movie-1", "Movie", "Movie")
        )))
    });
    let client = JellyfinClient::new(session(), transport);
    let query = SearchQuery::parse(" a&b=c ").unwrap();
    let page = LibraryPageRequest {
        start: 40,
        limit: 20,
    };
    let found = wait(client.search_page(&query, page, None)).unwrap();
    assert_eq!(found.items.len(), 1);
    assert_eq!(found.start, 40);
    assert_eq!(found.total, Some(57), "the server's count, for paging");
    let flag = CancelFlag::new();
    flag.cancel();
    let idle = Mock::new(|_| panic!("cancelled search must not send"));
    let client = JellyfinClient::new(session(), idle);
    assert!(matches!(
        wait(client.search_page(&query, page, Some(&flag))).unwrap_err(),
        JellyfinError::Cancelled
    ));
}

#[test]
fn search_page_never_returns_more_than_asked() {
    let transport = Mock::new(|_| {
        Ok(ok_json(&format!(
            r#"{{"Items":[{},{}],"TotalRecordCount":2}}"#,
            item_json("movie-1", "Movie", "Movie"),
            item_json("movie-2", "Movie", "Movie")
        )))
    });
    let client = JellyfinClient::new(session(), transport);
    let query = SearchQuery::parse("mo").unwrap();
    let page = LibraryPageRequest { start: 0, limit: 1 };
    let found = wait(client.search_page(&query, page, None)).unwrap();
    assert_eq!(found.items.len(), 1, "the page size is the server's limit");
    assert_eq!(found.total, Some(2));
}

#[test]
fn search_page_is_user_scoped_lean_and_header_authenticated() {
    let transport = Mock::new(|request| {
        assert_eq!(path_of(request), "/Users/user-1/Items");
        assert_eq!(
            query_value(request, "SearchTerm").as_deref(),
            Some("Amélie #1 + 50% off"),
            "one encoded value, decoded exactly"
        );
        assert_eq!(query_value(request, "Recursive").as_deref(), Some("true"));
        assert_eq!(
            query_value(request, "Fields").as_deref(),
            Some(crate::query::LIBRARY_FIELDS),
            "the grid's fields, not Details'"
        );
        assert_eq!(query_value(request, "ImageTypeLimit").as_deref(), Some("1"));
        assert_eq!(
            query_value(request, "EnableUserData").as_deref(),
            Some("true")
        );
        assert!(query_value(request, "api_key").is_none());
        assert!(!request.url.contains("token"), "{}", request.url);
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name == "Authorization" && value.contains("token with spaces")),
            "the token travels in the header"
        );
        Ok(ok_json(
            r#"{"Items":[{"Id":"episode-1","Name":"Pilot","Type":"Episode","SeriesId":"series-1","SeriesName":"Signal","IndexNumber":1,"ParentIndexNumber":1,"ImageTags":{"Primary":"still"},"UserData":{"PlaybackPositionTicks":6000000000,"PlayedPercentage":20.0}}]}"#,
        ))
    });
    let client = JellyfinClient::new(session(), transport);
    let query = SearchQuery::parse("Amélie #1 + 50% off").unwrap();
    let found = wait(client.search_page(
        &query,
        LibraryPageRequest {
            start: 0,
            limit: 60,
        },
        None,
    ))
    .unwrap();
    assert_eq!(found.total, None, "no count from the server: unknown");
    let episode = &found.items[0];
    assert_eq!(episode.kind, ItemKind::Episode);
    assert_eq!(episode.hierarchy.series_name.as_deref(), Some("Signal"));
    assert!(episode.is_resumable(), "user data is mapped");
    assert!(episode.artwork.primary.is_some());
}

#[test]
fn search_page_errors_are_classified_without_server_text() {
    let query = SearchQuery::parse("harbor").unwrap();
    let page = LibraryPageRequest {
        start: 0,
        limit: 60,
    };
    let answer = |status: u16, body: &'static str| {
        let transport = Mock::new(move |_| {
            Ok(HttpResponse {
                status,
                body: body.as_bytes().to_vec(),
            })
        });
        wait(JellyfinClient::new(session(), transport).search_page(&query, page, None))
    };
    assert!(matches!(
        answer(401, "").unwrap_err(),
        JellyfinError::Unauthorized
    ));
    assert!(matches!(
        answer(403, "").unwrap_err(),
        JellyfinError::Unauthorized
    ));
    let server = answer(500, "SearchTerm=harbor at line 3").unwrap_err();
    assert!(matches!(server, JellyfinError::Server { status: 500, .. }));
    assert!(
        !format!("{server} {server:?}").contains("line 3"),
        "no body"
    );
    assert!(matches!(
        answer(200, "{not json").unwrap_err(),
        JellyfinError::Malformed { .. }
    ));
    assert!(matches!(
        answer(200, r#"{"TotalRecordCount":3}"#).unwrap_err(),
        JellyfinError::Malformed { .. }
    ));
    let negative = answer(200, r#"{"Items":[],"TotalRecordCount":-1}"#).unwrap();
    assert_eq!(negative.total, None, "a nonsense count is not trusted");
    let unreachable = Mock::new(|_| Err(TransportError::Unreachable("timed out".into())));
    assert!(matches!(
        wait(JellyfinClient::new(session(), unreachable).search_page(&query, page, None))
            .unwrap_err(),
        JellyfinError::Unreachable { .. }
    ));
}

#[test]
fn series_navigation_preserves_order_and_absence() {
    let transport = Mock::new(|request| {
        let path = path_of(request);
        if path == "/Shows/series-1/Seasons" {
            return Ok(ok_json(&items_body(&[
                &item_json("season-2", "Season 2", "Season"),
                &item_json("season-1", "Season 1", "Season"),
            ])));
        }
        if path == "/Shows/series-1/Episodes"
            && query_value(request, "seasonId").as_deref() == Some("season-1")
        {
            assert_eq!(
                query_value(request, "sortBy").as_deref(),
                Some("IndexNumber")
            );
            return Ok(ok_json(&items_body(&[
                &item_json("episode-2", "Second", "Episode"),
                &item_json("episode-1", "Pilot", "Episode"),
            ])));
        }
        if path == "/Shows/NextUp" {
            assert_eq!(
                query_value(request, "seriesId").as_deref(),
                Some("series-1")
            );
            return Ok(ok_json(&items_body(&[])));
        }
        if query_value(request, "startItemId").as_deref() == Some("episode-1") {
            return Ok(ok_json(&items_body(&[
                &item_json("episode-9", "Later", "Episode"),
                &item_json("episode-1", "Pilot", "Episode"),
            ])));
        }
        panic!("unexpected {}", request.url);
    });
    let client = JellyfinClient::new(session(), transport);
    let series = ItemId::parse("series-1").unwrap();
    let seasons = wait(client.series_seasons(&series)).unwrap();
    assert_eq!(seasons[0].name(), "Season 2");
    assert_eq!(seasons[1].name(), "Season 1");
    let episodes =
        wait(client.season_episodes(&series, &ItemId::parse("season-1").unwrap())).unwrap();
    assert_eq!(episodes[0].name(), "Second");
    assert!(wait(client.next_up_episode(&series)).unwrap().is_none());
    let mut episode = bare_item("episode-1", "Pilot", ItemKind::Episode);
    episode.hierarchy.series_id = Some(series);
    let following = wait(client.following_episode(&episode)).unwrap().unwrap();
    assert_eq!(following.name(), "Later");
    let orphan = bare_item("episode-2", "Lost", ItemKind::Episode);
    let quiet = Mock::new(|_| panic!("missing series id must not send"));
    let client = JellyfinClient::new(session(), quiet);
    assert!(wait(client.following_episode(&orphan)).unwrap().is_none());
}

#[test]
fn playback_uses_the_native_profile_and_rejects_the_legacy_fallback() {
    let transport = Mock::new(|request| {
        assert_eq!(path_of(request), "/Items/movie-1/PlaybackInfo");
        assert_eq!(request.method, Method::Post);
        let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["IsPlayback"], true);
        assert_eq!(body["MaxStreamingBitrate"], 8_000_000);
        assert_eq!(body["AudioStreamIndex"], 2);
        assert_eq!(body["SubtitleStreamIndex"], -1);
        assert_eq!(body["StartTimeTicks"], 90_000_000);
        assert_eq!(body["DeviceProfile"]["Name"], "Matinee Native");
        assert_ne!(body["DeviceProfile"]["Name"], "Matinee WebKit");
        let video = body["DeviceProfile"]["DirectPlayProfiles"][0]["VideoCodec"]
            .as_str()
            .unwrap();
        assert!(video.contains("hevc") && video.contains("av1"));
        assert_eq!(
            body["DeviceProfile"]["TranscodingProfiles"][0]["Protocol"],
            "hls"
        );
        assert_eq!(
            body["DeviceProfile"]["TranscodingProfiles"][0]["Container"],
            "mp4"
        );
        assert_eq!(
            body["DeviceProfile"]["TranscodingProfiles"][0]["MaxAudioChannels"],
            "6"
        );
        Ok(ok_json(
            r#"{"PlaySessionId":"play-1","MediaSources":[{"Id":"source-1","SupportsDirectPlay":true,"SupportsDirectStream":true,"SupportsTranscoding":true,"MediaStreams":[{"Index":0,"Type":"Video","Codec":"hevc"}]}]}"#,
        ))
    });
    let client = JellyfinClient::new(session(), transport);
    let plan = wait(client.playback_plan(
        &bare_item("movie-1", "Movie", ItemKind::Movie),
        PlaybackOptions {
            max_bitrate: Some(8_000_000),
            audio_stream_index: Some(2),
            subtitle_stream_index: Some(-1),
            start_position: Some(Duration::from_secs(9)),
        },
        None,
    ))
    .unwrap();
    assert_eq!(plan.method, PlaybackMethod::DirectPlay);
    assert_eq!(plan.authorization, StreamAuthorization::Session);
    assert_eq!(
        plan.play_session_id.as_ref().map(PlaySessionId::as_str),
        Some("play-1")
    );
    assert_eq!(plan.selected_audio, Some(2));
    assert_eq!(plan.selected_subtitle, Some(-1));
    assert_eq!(plan.start_position, Duration::from_secs(9));
    let url = Url::parse(&plan.url).unwrap();
    assert_eq!(url.path(), "/Videos/movie-1/stream");
    assert_eq!(pair(&url, "Static").as_deref(), Some("true"));
    assert_eq!(pair(&url, "MediaSourceId").as_deref(), Some("source-1"));
    assert_eq!(pair(&url, "PlaySessionId").as_deref(), Some("play-1"));
    assert_eq!(pair(&url, "DeviceId").as_deref(), Some(DEVICE_ID));
    assert!(pair(&url, "api_key").is_none());
    assert_eq!(plan.streams.len(), 1);

    let legacy = Mock::new(|_| {
        Ok(ok_json(
            r#"{"ErrorCode":"NoCompatibleStream","MediaSources":[]}"#,
        ))
    });
    let client = JellyfinClient::new(session(), legacy);
    let error = wait(client.playback_plan(
        &bare_item("movie-1", "Movie", ItemKind::Movie),
        PlaybackOptions::default(),
        None,
    ))
    .unwrap_err();
    assert!(matches!(error, JellyfinError::NoCompatibleSource));
    assert!(!error.to_string().contains("Static"));
}

#[test]
fn transcode_urls_stay_on_the_server() {
    let transport = Mock::new(|_| {
        Ok(ok_json(
            r#"{"PlaySessionId":"play-2","MediaSources":[{"SupportsDirectPlay":false,"SupportsDirectStream":false,"SupportsTranscoding":true,"TranscodingUrl":"/Videos/movie-1/master.m3u8?MediaSourceId=source-1"}]}"#,
        ))
    });
    let client = JellyfinClient::new(session(), transport);
    let plan = wait(client.playback_plan(
        &bare_item("movie-1", "Movie", ItemKind::Movie),
        PlaybackOptions::default(),
        None,
    ))
    .unwrap();
    assert_eq!(plan.method, PlaybackMethod::Transcode);
    assert_eq!(plan.authorization, StreamAuthorization::Session);
    let url = Url::parse(&plan.url).unwrap();
    assert_eq!(url.path(), "/Videos/movie-1/master.m3u8");
    assert_eq!(pair(&url, "MediaSourceId").as_deref(), Some("source-1"));
    assert!(pair(&url, "api_key").is_none());

    let direct_stream = Mock::new(|_| {
        Ok(ok_json(
            r#"{"PlaySessionId":"play-3","MediaSources":[{"SupportsDirectPlay":false,"SupportsDirectStream":true,"TranscodingUrl":"/Videos/movie-1/master.m3u8?MediaSourceId=source-1&api_key=already"}]}"#,
        ))
    });
    let client = JellyfinClient::new(session(), direct_stream);
    let plan = wait(client.playback_plan(
        &bare_item("movie-1", "Movie", ItemKind::Movie),
        PlaybackOptions::default(),
        None,
    ))
    .unwrap();
    assert_eq!(plan.method, PlaybackMethod::DirectStream);
    assert_eq!(plan.authorization, StreamAuthorization::Session);
    let url = Url::parse(&plan.url).unwrap();
    assert_eq!(pair(&url, "MediaSourceId").as_deref(), Some("source-1"));
    assert!(pair(&url, "api_key").is_none());

    let evil = Mock::new(|_| {
        Ok(ok_json(
            r#"{"PlaySessionId":"play-4","MediaSources":[{"SupportsDirectPlay":false,"TranscodingUrl":"https://evil.example/steal"}]}"#,
        ))
    });
    let client = JellyfinClient::new(session(), evil);
    let error = wait(client.playback_plan(
        &bare_item("movie-1", "Movie", ItemKind::Movie),
        PlaybackOptions::default(),
        None,
    ))
    .unwrap_err();
    assert!(matches!(error, JellyfinError::PlaybackUnavailable { .. }));
    assert!(!error.to_string().contains("token with spaces"));
    assert!(!error.to_string().contains("evil.example"));
}

#[test]
fn playback_reports_include_position_streams_and_method() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    let transport = Mock::new(move |request| {
        record
            .lock()
            .expect("seen")
            .push((path_of(request), request.body.clone().unwrap_or_default()));
        Ok(ok_json(""))
    });
    let client = JellyfinClient::new(session(), transport);
    let report = PlaybackReport {
        item_id: ItemId::parse("movie-1").unwrap(),
        media_source_id: Some(matinee_core::MediaSourceId::parse("source-1").unwrap()),
        play_session_id: Some(PlaySessionId::parse("play-1").unwrap()),
        position: Duration::from_secs(3),
        paused: true,
        muted: true,
        volume: 0.5,
        audio_stream_index: Some(1),
        subtitle_stream_index: Some(-1),
        method: PlaybackMethod::DirectPlay,
    };
    wait(client.report_playback(ReportKind::Start, &report)).unwrap();
    wait(client.report_playback(ReportKind::Progress, &report)).unwrap();
    wait(client.report_playback(ReportKind::Stopped, &report)).unwrap();
    let calls = seen.lock().expect("seen").clone();
    assert_eq!(calls[0].0, "/Sessions/Playing");
    assert_eq!(calls[1].0, "/Sessions/Playing/Progress");
    assert_eq!(calls[2].0, "/Sessions/Playing/Stopped");
    let body: Value = serde_json::from_str(&calls[1].1).unwrap();
    assert_eq!(body["ItemId"], "movie-1");
    assert_eq!(body["MediaSourceId"], "source-1");
    assert_eq!(body["PlaySessionId"], "play-1");
    assert_eq!(body["PositionTicks"], 30_000_000);
    assert_eq!(body["IsPaused"], true);
    assert_eq!(body["IsMuted"], true);
    assert_eq!(body["VolumeLevel"], 50);
    assert_eq!(body["AudioStreamIndex"], 1);
    assert_eq!(body["SubtitleStreamIndex"], -1);
    assert_eq!(body["PlayMethod"], "DirectPlay");
    assert_eq!(body["CanSeek"], true);

    let quiet = PlaybackReport {
        play_session_id: None,
        ..report
    };
    wait(client.report_playback(ReportKind::Progress, &quiet)).unwrap();
    let calls = seen.lock().expect("seen").clone();
    let omitted: Value = serde_json::from_str(&calls[3].1).unwrap();
    assert!(omitted.get("PlaySessionId").is_none());
}

fn negotiated(body: &str) -> Result<PlaybackPlan, JellyfinError> {
    let body = body.to_string();
    let transport = Mock::new(move |_| Ok(ok_json(&body)));
    let client = JellyfinClient::new(session(), transport);
    wait(client.playback_plan(
        &bare_item("movie-1", "Movie", ItemKind::Movie),
        PlaybackOptions::default(),
        None,
    ))
}

fn negotiated_item(item_id: &str, body: &str) -> Result<PlaybackPlan, JellyfinError> {
    let body = body.to_string();
    let item_id = item_id.to_string();
    let transport = Mock::new(move |_| Ok(ok_json(&body)));
    let client = JellyfinClient::new(session(), transport);
    wait(client.playback_plan(
        &bare_item(&item_id, "Movie", ItemKind::Movie),
        PlaybackOptions::default(),
        None,
    ))
}

fn negotiated_on(server: &str, body: &str) -> Result<PlaybackPlan, JellyfinError> {
    let body = body.to_string();
    let transport = Mock::new(move |_| Ok(ok_json(&body)));
    let client = JellyfinClient::new(
        Session::new(server, "token with spaces", user()).unwrap(),
        transport,
    );
    wait(client.playback_plan(
        &bare_item("movie-1", "Movie", ItemKind::Movie),
        PlaybackOptions::default(),
        None,
    ))
}

#[test]
fn delivery_method_follows_what_the_address_does() {
    let remux = negotiated(
        r#"{"PlaySessionId":"play-5","MediaSources":[{"Id":"remux","SupportsDirectPlay":false,"SupportsDirectStream":true,"SupportsTranscoding":true,"TranscodingUrl":"/Videos/movie-1/master.m3u8?MediaSourceId=remux&TranscodeReasons=ContainerNotSupported","TranscodeReasons":"ContainerNotSupported"}]}"#,
    )
    .unwrap();
    assert_eq!(remux.method, PlaybackMethod::DirectStream);
    assert_eq!(remux.authorization, StreamAuthorization::Session);
    let url = Url::parse(&remux.url).unwrap();
    assert_eq!(pair(&url, "MediaSourceId").as_deref(), Some("remux"));
    assert!(pair(&url, "api_key").is_none());

    let both = negotiated(
        r#"{"PlaySessionId":"play-6","MediaSources":[{"Id":"both","SupportsDirectPlay":false,"SupportsDirectStream":true,"SupportsTranscoding":true,"TranscodingUrl":"/Videos/movie-1/master.m3u8?MediaSourceId=both&api_key=one&TranscodeReasons=VideoCodecNotSupported&api_key=two"}]}"#,
    )
    .unwrap();
    assert_eq!(both.method, PlaybackMethod::Transcode);
    let url = Url::parse(&both.url).unwrap();
    assert_eq!(pair(&url, "MediaSourceId").as_deref(), Some("both"));
    assert!(pair(&url, "api_key").is_none());
    assert_eq!(
        url.query_pairs()
            .filter(|(key, _)| key.eq_ignore_ascii_case("api_key"))
            .count(),
        0
    );

    let copied = negotiated(
        r#"{"MediaSources":[{"SupportsDirectPlay":false,"SupportsDirectStream":true,"SupportsTranscoding":true,"TranscodingUrl":"/Videos/movie-1/master.m3u8?MediaSourceId=copy&VideoCodec=copy&AudioCodec=copy"}]}"#,
    )
    .unwrap();
    assert_eq!(copied.method, PlaybackMethod::DirectStream);
    assert!(copied.play_session_id.is_none());
    assert!(pair(&Url::parse(&copied.url).unwrap(), "PlaySessionId").is_none());
}

#[test]
fn source_selection_prefers_direct_play_then_remux_then_transcode() {
    let direct = negotiated(
        r#"{"PlaySessionId":"play-7","MediaSources":[{"Id":"enc","SupportsDirectPlay":false,"SupportsTranscoding":true,"TranscodingUrl":"/Videos/movie-1/master.m3u8?MediaSourceId=enc&TranscodeReasons=VideoCodecNotSupported"},{"Id":"direct","SupportsDirectPlay":true}]}"#,
    )
    .unwrap();
    assert_eq!(direct.method, PlaybackMethod::DirectPlay);
    assert_eq!(
        direct.source_id.as_ref().map(|id| id.as_str()),
        Some("direct")
    );

    let omitted = negotiated(
        r#"{"PlaySessionId":"   ","MediaSources":[{"SupportsDirectPlay":true,"Id":"source-1"}]}"#,
    )
    .unwrap();
    assert!(omitted.play_session_id.is_none());
    assert!(pair(&Url::parse(&omitted.url).unwrap(), "PlaySessionId").is_none());
    assert_eq!(omitted.authorization, StreamAuthorization::Session);

    let remux = negotiated(
        r#"{"PlaySessionId":"play-8","MediaSources":[{"Id":"enc","SupportsDirectPlay":false,"SupportsDirectStream":true,"SupportsTranscoding":true,"TranscodingUrl":"/Videos/movie-1/master.m3u8?MediaSourceId=enc&TranscodeReasons=VideoCodecNotSupported"},{"Id":"remux","SupportsDirectPlay":false,"SupportsDirectStream":true,"SupportsTranscoding":true,"TranscodingUrl":"/Videos/movie-1/master.m3u8?MediaSourceId=remux&TranscodeReasons=ContainerBitrateExceedsLimit"}]}"#,
    )
    .unwrap();
    assert_eq!(remux.method, PlaybackMethod::DirectStream);
    assert_eq!(
        remux.source_id.as_ref().map(|id| id.as_str()),
        Some("remux")
    );

    let missing = negotiated(
        r#"{"PlaySessionId":"play-9","MediaSources":[{"Id":"dead","SupportsDirectPlay":false,"SupportsDirectStream":true,"SupportsTranscoding":true}]}"#,
    )
    .unwrap_err();
    assert!(matches!(missing, JellyfinError::PlaybackUnavailable { .. }));
    assert!(missing.to_string().contains("No playable media source"));
    assert!(!missing.to_string().contains("token"));
}

#[test]
fn stream_urls_drop_credentials_and_stay_on_the_server() {
    let encoded = negotiated_item(
        "movie+1",
        r#"{"MediaSources":[{"SupportsDirectPlay":true,"Id":"source&1"}]}"#,
    )
    .unwrap();
    assert!(encoded.url.contains("/Videos/movie%2B1/stream"));
    assert!(encoded.url.contains("MediaSourceId=source%261"));
    assert!(!encoded.url.contains("api_key"));

    let ampersand = negotiated_item(
        "movie&1",
        r#"{"MediaSources":[{"SupportsDirectPlay":true}]}"#,
    )
    .unwrap();
    assert!(ampersand.url.contains("/Videos/movie%261/stream"));

    let rooted = negotiated_on(
        "http://jellyfin.local:8096/jellyfin",
        r#"{"MediaSources":[{"SupportsDirectPlay":false,"SupportsTranscoding":true,"TranscodingUrl":"/Videos/movie-1/master.m3u8?MediaSourceId=source-1&AccessToken=secret&token=also"}]}"#,
    )
    .unwrap();
    let url = Url::parse(&rooted.url).unwrap();
    assert_eq!(url.path(), "/Videos/movie-1/master.m3u8");
    assert!(pair(&url, "api_key").is_none());
    assert!(pair(&url, "AccessToken").is_none());
    assert!(pair(&url, "token").is_none());
    assert!(!rooted.url.to_ascii_lowercase().contains("secret"));

    let relative = negotiated_on(
        "http://jellyfin.local:8096/jellyfin",
        r#"{"MediaSources":[{"SupportsDirectPlay":false,"SupportsTranscoding":true,"TranscodingUrl":"Videos/movie-1/master.m3u8?MediaSourceId=source-1"}]}"#,
    )
    .unwrap();
    assert_eq!(
        Url::parse(&relative.url).unwrap().path(),
        "/jellyfin/Videos/movie-1/master.m3u8"
    );

    for bad in [
        "//other-host/path",
        "http://use%72:secret@evil.example/steal",
        "/Videos/movie-1/master.m3u8?MediaSourceId=source\n1",
        "https://evil.example/steal",
    ] {
        let body = format!(
            r#"{{"MediaSources":[{{"SupportsDirectPlay":false,"TranscodingUrl":{}}}]}}"#,
            serde_json::to_string(bad).unwrap()
        );
        let error = negotiated(&body).unwrap_err();
        assert!(matches!(error, JellyfinError::PlaybackUnavailable { .. }));
        assert!(!error.to_string().contains("secret"));
        assert!(!error.to_string().contains("evil.example"));
        assert!(!error.to_string().contains("other-host"));
    }
}

#[test]
fn normalize_rejects_a_scheme_relative_address() {
    assert!(crate::normalize_server_url("//jellyfin.local").is_err());
}

#[test]
fn artwork_is_fetched_with_the_session_header_and_no_token_in_the_url() {
    let mock = Mock::new(|_| {
        Ok(HttpResponse {
            status: 200,
            body: vec![0x89, b'P', b'N', b'G'],
        })
    });
    let calls = Arc::clone(&mock.calls);
    let client = JellyfinClient::new(session(), mock);
    let mut item = bare_item("movie-1", "Movie", ItemKind::Movie);
    assert!(
        client
            .session()
            .artwork()
            .item_request(&item, ImageRole::Backdrop, 1600)
            .is_none(),
        "no backdrop tag means no request"
    );
    item.artwork.primary = ImageTag::parse("poster-tag");
    let request = client
        .session()
        .artwork()
        .item_request(&item, ImageRole::Primary, 480)
        .unwrap();
    assert!(!request.url.contains("api_key"));
    assert!(!request.url.contains("token"));
    assert!(request.url.contains("tag=poster-tag"));

    let bytes = wait(client.fetch_artwork(&request, None)).unwrap();
    assert_eq!(bytes, vec![0x89, b'P', b'N', b'G']);
    let sent = calls.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].url, request.url);
    let authorization = sent[0]
        .headers
        .iter()
        .find(|(name, _)| name == "Authorization")
        .map(|(_, value)| value.as_str())
        .unwrap();
    assert!(authorization.contains("Token=\"token with spaces\""));
    assert!(!format!("{:?}", sent[0]).contains("token with spaces"));
}

#[test]
fn artwork_off_the_server_or_too_large_is_refused() {
    let mock = Mock::new(|request| {
        Ok(HttpResponse {
            status: if request.url.contains("missing") {
                404
            } else {
                200
            },
            body: vec![0; crate::MAX_ARTWORK_BYTES + 1],
        })
    });
    let calls = Arc::clone(&mock.calls);
    let client = JellyfinClient::new(session(), mock);
    let foreign = crate::ArtworkRequest {
        url: "http://elsewhere.example/Items/movie-1/Images/Primary".into(),
    };
    assert!(matches!(
        wait(client.fetch_artwork(&foreign, None)),
        Err(JellyfinError::InvalidUrl { .. })
    ));
    assert!(calls.lock().unwrap().is_empty(), "nothing was sent");
    let large = client.session().artwork().image_request(
        &ItemId::parse("movie-1").unwrap(),
        ImageRole::Primary,
        480,
    );
    assert!(matches!(
        wait(client.fetch_artwork(&large, None)),
        Err(JellyfinError::Malformed { .. })
    ));
    let missing = client.session().artwork().image_request(
        &ItemId::parse("missing").unwrap(),
        ImageRole::Primary,
        480,
    );
    assert!(matches!(
        wait(client.fetch_artwork(&missing, None)),
        Err(JellyfinError::NotFound)
    ));
}

#[test]
fn cast_portraits_have_a_native_request() {
    use matinee_core::{Credit, Person};
    let current = session();
    let mut person = Person {
        id: ItemId::parse("person-1").ok(),
        name: "Ada".into(),
        role: Some("Captain".into()),
        credit: Credit::Actor,
        image: None,
    };
    assert!(current.artwork().person_request(&person, 240).is_none());
    person.image = ImageTag::parse("face");
    let request = current.artwork().person_request(&person, 240).unwrap();
    assert!(
        request
            .url
            .starts_with("http://jellyfin.local:8096/Items/person-1/Images/Primary?")
    );
    assert!(!request.url.contains("api_key"));
}

#[test]
fn library_pages_are_sorted_filtered_and_paged_on_the_server() {
    use matinee_core::{LibraryFilter, LibraryId, LibraryPageRequest, LibraryQuery, WatchFilter};
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    let transport = Mock::new(move |request| {
        record.lock().expect("seen").push(request.url.clone());
        Ok(ok_json(&format!(
            r#"{{"Items":[{},{}],"TotalRecordCount":1284,"StartIndex":100}}"#,
            item_json("m1", "One", "Movie"),
            item_json("m2", "Two", "Movie")
        )))
    });
    let client = JellyfinClient::new(session(), transport);
    let query = LibraryQuery {
        kind: LibraryKind::Movies,
        view: Some(LibraryId::parse("view-1").unwrap()),
        sort: LibrarySort::ProductionYear,
        filter: LibraryFilter {
            watch: WatchFilter::Unwatched,
            genre: Some(ItemId::parse("genre-9").unwrap()),
        },
    };
    let page = wait(client.library_page(
        &query,
        LibraryPageRequest {
            start: 100,
            limit: 100,
        },
    ))
    .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.start, 100);
    assert_eq!(page.total, Some(1284));
    assert!(page.has_more(100));

    let series = LibraryQuery::new(LibraryKind::Series);
    wait(client.library_page(
        &series,
        LibraryPageRequest {
            start: 0,
            limit: 100,
        },
    ))
    .unwrap();

    let urls: Vec<Url> = seen
        .lock()
        .expect("seen")
        .iter()
        .map(|value| Url::parse(value).unwrap())
        .collect();
    let first = &urls[0];
    assert_eq!(first.path(), "/Users/user-1/Items");
    assert_eq!(pair(first, "IncludeItemTypes").as_deref(), Some("Movie"));
    assert_eq!(pair(first, "ParentId").as_deref(), Some("view-1"));
    assert_eq!(
        pair(first, "SortBy").as_deref(),
        Some("ProductionYear,SortName")
    );
    assert_eq!(
        pair(first, "SortOrder").as_deref(),
        Some("Descending,Ascending")
    );
    assert_eq!(pair(first, "Filters").as_deref(), Some("IsUnplayed"));
    assert_eq!(pair(first, "GenreIds").as_deref(), Some("genre-9"));
    assert_eq!(pair(first, "StartIndex").as_deref(), Some("100"));
    assert_eq!(pair(first, "Limit").as_deref(), Some("100"));
    assert_eq!(
        pair(first, "EnableTotalRecordCount").as_deref(),
        Some("true")
    );
    assert_eq!(pair(first, "Recursive").as_deref(), Some("true"));
    let fields = pair(first, "Fields").unwrap();
    assert!(!fields.contains("People") && !fields.contains("MediaSources"));

    let second = &urls[1];
    assert_eq!(pair(second, "IncludeItemTypes").as_deref(), Some("Series"));
    assert_eq!(pair(second, "ParentId"), None, "every library, as shipping");
    assert_eq!(pair(second, "SortBy").as_deref(), Some("SortName"));
    assert_eq!(pair(second, "SortOrder").as_deref(), Some("Ascending"));
    assert_eq!(pair(second, "Filters"), None);
    assert_eq!(pair(second, "GenreIds"), None);
    for url in &urls {
        assert!(!url.as_str().contains("token"), "{url}");
        assert!(!url.as_str().contains("api_key"), "{url}");
    }
}

#[test]
fn library_watch_filters_map_to_server_filters() {
    use matinee_core::{LibraryPageRequest, LibraryQuery, WatchFilter};
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    let transport = Mock::new(move |request| {
        record.lock().expect("seen").push(request.url.clone());
        Ok(ok_json(&items_body(&[])))
    });
    let client = JellyfinClient::new(session(), transport);
    for watch in WatchFilter::ALL {
        let mut query = LibraryQuery::new(LibraryKind::Movies);
        query.filter.watch = watch;
        let page = wait(client.library_page(
            &query,
            LibraryPageRequest {
                start: 0,
                limit: 100,
            },
        ))
        .unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.total, None);
        assert!(
            !page.has_more(100),
            "an empty page without a total is the end"
        );
    }
    let filters: Vec<Option<String>> = seen
        .lock()
        .expect("seen")
        .iter()
        .map(|value| pair(&Url::parse(value).unwrap(), "Filters"))
        .collect();
    assert_eq!(
        filters,
        vec![
            None,
            Some("IsUnplayed".into()),
            Some("IsPlayed".into()),
            Some("IsFavorite".into())
        ]
    );
}

#[test]
fn library_requests_report_authorization_and_malformed_answers() {
    use matinee_core::{LibraryPageRequest, LibraryQuery};
    let transport = Mock::new(|request| {
        if path_of(request) == "/Users/user-1/Views" {
            return Ok(ok_json("{}"));
        }
        Ok(HttpResponse {
            status: 401,
            body: b"secret server text".to_vec(),
        })
    });
    let client = JellyfinClient::new(session(), transport);
    let query = LibraryQuery::new(LibraryKind::Movies);
    assert!(matches!(
        wait(client.library_page(
            &query,
            LibraryPageRequest {
                start: 0,
                limit: 100
            }
        ))
        .unwrap_err(),
        JellyfinError::Unauthorized
    ));
    assert!(matches!(
        wait(client.library_genres(LibraryKind::Movies, None)).unwrap_err(),
        JellyfinError::Unauthorized
    ));
    assert!(matches!(
        wait(client.library_views()).unwrap_err(),
        JellyfinError::Malformed { .. }
    ));
}

#[test]
fn library_views_keep_video_libraries_and_genres_keep_their_ids() {
    use matinee_core::{LibraryContent, LibraryId};
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    let transport = Mock::new(move |request| {
        record.lock().expect("seen").push(request.url.clone());
        if path_of(request) == "/Users/user-1/Views" {
            return Ok(ok_json(
                r#"{"Items":[
                    {"Id":"v-films","Name":"Films","CollectionType":"movies"},
                    {"Id":"v-tv","Name":"Television","CollectionType":"tvshows"},
                    {"Id":"v-music","Name":"Music","CollectionType":"music"},
                    {"Id":"v-mixed","Name":"Home Media"},
                    {"Id":"v-blank","Name":"  ","CollectionType":"movies"},
                    {"Id":"v-sets","Name":"Collections","CollectionType":"boxsets"}
                ]}"#,
            ));
        }
        Ok(ok_json(
            r#"{"Items":[{"Id":"g-1","Name":"Drama"},{"Id":"g-2","Name":"  "},{"Id":"g-3","Name":"Science Fiction"}]}"#,
        ))
    });
    let client = JellyfinClient::new(session(), transport);
    let views = wait(client.library_views()).unwrap();
    let summary: Vec<(&str, LibraryContent)> = views
        .iter()
        .map(|view| (view.name.as_str(), view.content))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("Films", LibraryContent::Movies),
            ("Television", LibraryContent::Series),
            ("Home Media", LibraryContent::Mixed),
        ]
    );
    let view = LibraryId::parse("v-films").unwrap();
    let genres = wait(client.library_genres(LibraryKind::Movies, Some(&view))).unwrap();
    let names: Vec<&str> = genres.iter().map(|genre| genre.name.as_str()).collect();
    assert_eq!(names, vec!["Drama", "Science Fiction"]);
    assert_eq!(genres[0].id.as_str(), "g-1");
    let urls: Vec<Url> = seen
        .lock()
        .expect("seen")
        .iter()
        .map(|value| Url::parse(value).unwrap())
        .collect();
    let genres_url = &urls[1];
    assert_eq!(genres_url.path(), "/Genres");
    assert_eq!(pair(genres_url, "parentId").as_deref(), Some("v-films"));
    assert_eq!(
        pair(genres_url, "includeItemTypes").as_deref(),
        Some("Movie")
    );
}
