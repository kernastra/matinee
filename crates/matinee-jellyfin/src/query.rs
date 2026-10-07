//! Query strings for the shipping Jellyfin calls.
//!
//! Keys match `src/lib/jellyfin.ts`. Values are encoded. Callers never pass
//! a raw Jellyfin query string.

use matinee_core::{
    ItemId, LibraryKind, LibraryPageRequest, LibraryQuery, LibrarySort, WatchFilter,
};

pub(crate) const ITEM_FIELDS: &str = "Overview,Genres,RunTimeTicks,ProductionYear,CommunityRating,CriticRating,OfficialRating,PrimaryImageAspectRatio,MediaStreams,MediaSources,Chapters,ParentId,DateCreated,PremiereDate,EndDate,Taglines,Studios,People,ProductionLocations,ProviderIds";

/// Fields for a library grid page: what a card and its ordering need, not
/// the people, streams, and chapters Details asks for. Image tags and user
/// data come back without being listed.
pub(crate) const LIBRARY_FIELDS: &str = "Genres,ProductionYear,CommunityRating,OfficialRating,PrimaryImageAspectRatio,DateCreated,PremiereDate";

pub(crate) struct Query {
    pairs: Vec<(&'static str, String)>,
}

impl Query {
    pub(crate) fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    pub(crate) fn pair(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.pairs.push((key, value.into()));
        self
    }

    pub(crate) fn encode(self) -> String {
        let mut out = String::new();
        for (index, (key, value)) in self.pairs.iter().enumerate() {
            if index > 0 {
                out.push('&');
            }
            out.push_str(key);
            out.push('=');
            out.push_str(&encode_component(value));
        }
        out
    }
}

pub(crate) fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                out.push('%');
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 0x0f)]));
            }
        }
    }
    out
}

fn user_items(user_id: &str, query: Query) -> String {
    format!(
        "/Users/{}/Items?{}",
        encode_component(user_id),
        query.encode()
    )
}

pub(crate) fn resume_path(user_id: &str) -> String {
    format!(
        "/Users/{}/Items/Resume?{}",
        encode_component(user_id),
        Query::new()
            .pair("Limit", "12")
            .pair("MediaTypes", "Video")
            .pair("Fields", ITEM_FIELDS)
            .encode()
    )
}

pub(crate) fn latest_path(user_id: &str) -> String {
    user_items(
        user_id,
        Query::new()
            .pair("Recursive", "true")
            .pair("IncludeItemTypes", "Movie,Series")
            .pair("SortBy", "DateCreated")
            .pair("SortOrder", "Descending")
            .pair("Limit", "6")
            .pair("Fields", ITEM_FIELDS)
            .pair("EnableUserData", "true"),
    )
}

pub(crate) fn movies_path(user_id: &str) -> String {
    shelf(user_id, "Movie", "12")
}

pub(crate) fn series_path(user_id: &str) -> String {
    shelf(user_id, "Series", "12")
}

fn shelf(user_id: &str, kinds: &str, limit: &str) -> String {
    user_items(
        user_id,
        Query::new()
            .pair("Recursive", "true")
            .pair("IncludeItemTypes", kinds)
            .pair("SortBy", "DateCreated")
            .pair("SortOrder", "Descending")
            .pair("Limit", limit)
            .pair("Fields", ITEM_FIELDS),
    )
}

pub(crate) fn top_rated_path(user_id: &str) -> String {
    user_items(
        user_id,
        Query::new()
            .pair("Recursive", "true")
            .pair("IncludeItemTypes", "Movie,Series")
            .pair("SortBy", "CommunityRating")
            .pair("SortOrder", "Descending")
            .pair("Limit", "12")
            .pair("Fields", ITEM_FIELDS)
            .pair("EnableUserData", "true"),
    )
}

pub(crate) fn favorites_path(user_id: &str) -> String {
    user_items(
        user_id,
        Query::new()
            .pair("Recursive", "true")
            .pair("IncludeItemTypes", "Movie,Series")
            .pair("Filters", "IsFavorite")
            .pair("SortBy", "DateCreated")
            .pair("SortOrder", "Descending")
            .pair("Limit", "12")
            .pair("Fields", ITEM_FIELDS)
            .pair("EnableUserData", "true"),
    )
}

/// Next up across every series in progress, for Home. Resumable episodes
/// are left to Continue Watching so the two rows do not repeat each other.
pub(crate) fn next_up_feed_path(user_id: &str) -> String {
    format!(
        "/Shows/NextUp?{}",
        Query::new()
            .pair("userId", user_id)
            .pair("limit", "12")
            .pair("fields", ITEM_FIELDS)
            .pair("enableImages", "true")
            .pair("enableUserData", "true")
            .pair("enableResumable", "false")
            .encode()
    )
}

pub(crate) fn library_path(user_id: &str, kind: LibraryKind, sort: LibrarySort) -> String {
    let include = match kind {
        LibraryKind::Movies => "Movie",
        LibraryKind::Series => "Series",
    };
    let sort_by = match sort {
        LibrarySort::Name => "SortName",
        LibrarySort::DateCreated => "DateCreated",
        LibrarySort::ProductionYear => "ProductionYear",
        LibrarySort::CommunityRating => "CommunityRating",
    };
    let order = if sort.descending() {
        "Descending"
    } else {
        "Ascending"
    };
    user_items(
        user_id,
        Query::new()
            .pair("Recursive", "true")
            .pair("IncludeItemTypes", include)
            .pair("SortBy", sort_by)
            .pair("SortOrder", order)
            .pair("Limit", "240")
            .pair("Fields", ITEM_FIELDS)
            .pair("EnableUserData", "true"),
    )
}

fn include_types(kind: LibraryKind) -> &'static str {
    match kind {
        LibraryKind::Movies => "Movie",
        LibraryKind::Series => "Series",
    }
}

fn sort_by(sort: LibrarySort) -> &'static str {
    match sort {
        LibrarySort::Name => "SortName",
        LibrarySort::DateCreated => "DateCreated",
        LibrarySort::ProductionYear => "ProductionYear",
        LibrarySort::CommunityRating => "CommunityRating",
    }
}

/// One page of a typed library query. Sorting, filtering, and paging all
/// happen on the server. Every sort but Title ends with the title, so
/// titles that tie (one release year, one rating) keep one order from page
/// to page.
pub(crate) fn library_page_path(
    user_id: &str,
    query: &LibraryQuery,
    page: LibraryPageRequest,
) -> String {
    let (sort_by, sort_order) = match query.sort {
        LibrarySort::Name => ("SortName".to_string(), "Ascending"),
        other => (
            format!("{},SortName", sort_by(other)),
            "Descending,Ascending",
        ),
    };
    let mut pairs = Query::new()
        .pair("Recursive", "true")
        .pair("IncludeItemTypes", include_types(query.kind));
    if let Some(view) = &query.view {
        pairs = pairs.pair("ParentId", view.as_str());
    }
    pairs = pairs.pair("SortBy", sort_by).pair("SortOrder", sort_order);
    let filters = match query.filter.watch {
        WatchFilter::All => None,
        WatchFilter::Unwatched => Some("IsUnplayed"),
        WatchFilter::Watched => Some("IsPlayed"),
        WatchFilter::Favorites => Some("IsFavorite"),
    };
    if let Some(filters) = filters {
        pairs = pairs.pair("Filters", filters);
    }
    if let Some(genre) = &query.filter.genre {
        pairs = pairs.pair("GenreIds", genre.as_str());
    }
    user_items(
        user_id,
        pairs
            .pair("StartIndex", page.start.to_string())
            .pair("Limit", page.limit.to_string())
            .pair("Fields", LIBRARY_FIELDS)
            .pair("ImageTypeLimit", "1")
            .pair("EnableUserData", "true")
            .pair("EnableTotalRecordCount", "true"),
    )
}

/// The person's libraries.
pub(crate) fn views_path(user_id: &str) -> String {
    format!("/Users/{}/Views", encode_component(user_id))
}

/// Genres present among titles of `kind`, optionally inside one library.
pub(crate) fn genres_path(user_id: &str, kind: LibraryKind, view: Option<&str>) -> String {
    let mut pairs = Query::new()
        .pair("userId", user_id)
        .pair("includeItemTypes", include_types(kind))
        .pair("recursive", "true")
        .pair("sortBy", "SortName")
        .pair("sortOrder", "Ascending")
        .pair("enableTotalRecordCount", "false");
    if let Some(view) = view {
        pairs = pairs.pair("parentId", view);
    }
    format!("/Genres?{}", pairs.encode())
}

pub(crate) fn item_path(user_id: &str, item_id: &ItemId) -> String {
    format!(
        "/Users/{}/Items/{}?{}",
        encode_component(user_id),
        encode_component(item_id.as_str()),
        Query::new()
            .pair("userId", user_id)
            .pair("Fields", ITEM_FIELDS)
            .pair("EnableUserData", "true")
            .encode()
    )
}

pub(crate) fn similar_path(user_id: &str, item_id: &ItemId, limit: u32) -> String {
    format!(
        "/Items/{}/Similar?{}",
        encode_component(item_id.as_str()),
        Query::new()
            .pair("userId", user_id)
            .pair("limit", limit.to_string())
            .pair("fields", ITEM_FIELDS)
            .encode()
    )
}

pub(crate) fn collections_path(user_id: &str, item_id: &ItemId) -> String {
    format!(
        "/Items/{}/Collections?{}",
        encode_component(item_id.as_str()),
        Query::new()
            .pair("userId", user_id)
            .pair("limit", "2")
            .pair("fields", ITEM_FIELDS)
            .encode()
    )
}

pub(crate) fn collection_items_path(user_id: &str, collection_id: &ItemId) -> String {
    user_items(
        user_id,
        Query::new()
            .pair("ParentId", collection_id.as_str())
            .pair("Recursive", "true")
            .pair("IncludeItemTypes", "Movie,Series")
            .pair("SortBy", "ProductionYear,SortName")
            .pair("SortOrder", "Ascending")
            .pair("Fields", ITEM_FIELDS)
            .pair("EnableUserData", "true"),
    )
}

pub(crate) fn seasons_path(user_id: &str, series_id: &ItemId) -> String {
    format!(
        "/Shows/{}/Seasons?{}",
        encode_component(series_id.as_str()),
        Query::new()
            .pair("userId", user_id)
            .pair("fields", ITEM_FIELDS)
            .pair("isMissing", "false")
            .pair("enableImages", "true")
            .pair("enableUserData", "true")
            .encode()
    )
}

pub(crate) fn episodes_path(user_id: &str, series_id: &ItemId, season_id: &ItemId) -> String {
    format!(
        "/Shows/{}/Episodes?{}",
        encode_component(series_id.as_str()),
        Query::new()
            .pair("userId", user_id)
            .pair("seasonId", season_id.as_str())
            .pair("fields", ITEM_FIELDS)
            .pair("isMissing", "false")
            .pair("enableImages", "true")
            .pair("enableUserData", "true")
            .pair("sortBy", "IndexNumber")
            .encode()
    )
}

pub(crate) fn next_up_path(user_id: &str, series_id: &ItemId) -> String {
    format!(
        "/Shows/NextUp?{}",
        Query::new()
            .pair("userId", user_id)
            .pair("seriesId", series_id.as_str())
            .pair("limit", "1")
            .pair("fields", ITEM_FIELDS)
            .pair("enableImages", "true")
            .pair("enableUserData", "true")
            .pair("enableResumable", "true")
            .encode()
    )
}

pub(crate) fn following_path(user_id: &str, series_id: &ItemId, episode_id: &ItemId) -> String {
    format!(
        "/Shows/{}/Episodes?{}",
        encode_component(series_id.as_str()),
        Query::new()
            .pair("userId", user_id)
            .pair("startItemId", episode_id.as_str())
            .pair("limit", "2")
            .pair("fields", ITEM_FIELDS)
            .pair("isMissing", "false")
            .pair("enableImages", "true")
            .pair("enableUserData", "true")
            .pair("sortBy", "AiredEpisodeOrder")
            .encode()
    )
}

pub(crate) fn search_path(user_id: &str, term: &str) -> String {
    user_items(
        user_id,
        Query::new()
            .pair("Recursive", "true")
            .pair("SearchTerm", term)
            .pair("IncludeItemTypes", "Movie,Series,Episode")
            .pair("Limit", "40")
            .pair("Fields", ITEM_FIELDS)
            .pair("EnableUserData", "true"),
    )
}
