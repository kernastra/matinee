//! Behavior tests against an in-memory transport. No sockets.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use matinee_core::{
    ImageRole, ImageTag, ItemHierarchy, ItemId, ItemIdentity, ItemKind, ItemMetadata, LibraryKind,
    LibrarySort, MediaItem, PlaybackMethod, PlaybackOptions, PlaybackReport, ReportKind,
    TechnicalMedia, User, UserId, UserItemState,
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
                r#"{"IsFavorite":true,"PlaybackPositionTicks":42,"PlayedPercentage":12,"LastPlayedDate":"2024-01-01T00:00:00Z"}"#,
            ));
        }
        assert_eq!(request.method, Method::Post);
        let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["IsFavorite"], true);
        assert_eq!(body["PlaybackPositionTicks"], 0);
        assert_eq!(body["PlayedPercentage"], 0);
        assert_eq!(body["LastPlayedDate"], "2024-01-01T00:00:00Z");
        Ok(ok_json(""))
    });
    let client = JellyfinClient::new(session(), transport);
    wait(client.clear_item_progress(&ItemId::parse("movie-1").unwrap())).unwrap();
}

#[test]
fn search_includes_episodes_and_can_be_cancelled() {
    let transport = Mock::new(|request| {
        assert_eq!(query_value(request, "SearchTerm").as_deref(), Some("a&b=c"));
        assert_eq!(
            query_value(request, "IncludeItemTypes").as_deref(),
            Some("Movie,Series,Episode")
        );
        assert!(query_value(request, "b").is_none());
        Ok(ok_json(&items_body(&[&item_json(
            "movie-1", "Movie", "Movie",
        )])))
    });
    let client = JellyfinClient::new(session(), transport);
    let found = wait(client.search_library("a&b=c", None)).unwrap();
    assert_eq!(found.len(), 1);
    let flag = CancelFlag::new();
    flag.cancel();
    let idle = Mock::new(|_| panic!("cancelled search must not send"));
    let client = JellyfinClient::new(session(), idle);
    assert!(matches!(
        wait(client.search_library("later", Some(&flag))).unwrap_err(),
        JellyfinError::Cancelled
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
    assert_eq!(plan.play_session_id.as_str(), "play-1");
    assert_eq!(plan.selected_audio, Some(2));
    assert_eq!(plan.selected_subtitle, Some(-1));
    assert_eq!(plan.start_position, Duration::from_secs(9));
    let url = Url::parse(&plan.url).unwrap();
    assert_eq!(url.path(), "/Videos/movie-1/stream");
    assert_eq!(pair(&url, "Static").as_deref(), Some("true"));
    assert_eq!(pair(&url, "MediaSourceId").as_deref(), Some("source-1"));
    assert_eq!(pair(&url, "PlaySessionId").as_deref(), Some("play-1"));
    assert_eq!(pair(&url, "api_key").as_deref(), Some("token with spaces"));
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
    let url = Url::parse(&plan.url).unwrap();
    assert_eq!(url.path(), "/Videos/movie-1/master.m3u8");
    assert_eq!(pair(&url, "MediaSourceId").as_deref(), Some("source-1"));
    assert_eq!(pair(&url, "api_key").as_deref(), Some("token with spaces"));

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
    let url = Url::parse(&plan.url).unwrap();
    assert_eq!(pair(&url, "api_key").as_deref(), Some("already"));

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
        play_session_id: matinee_core::PlaySessionId::parse("play-1").unwrap(),
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
}

#[test]
fn normalize_rejects_a_scheme_relative_address() {
    assert!(crate::normalize_server_url("//jellyfin.local").is_err());
}
