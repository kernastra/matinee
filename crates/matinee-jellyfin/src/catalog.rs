//! Home, library, details, search, and user-state writes.

use matinee_core::{
    CollectionContext, HomeFeed, HomeShelf, ItemId, LibraryKind, LibrarySort, MediaItem,
};
use serde_json::json;

use crate::client::{Endpoint, JellyfinClient};
use crate::convert::{item_from_dto, items_from_dtos};
use crate::dto::{ItemDto, ItemsDto};
use crate::error::JellyfinError;
use crate::query::encode_component;
use crate::query::{
    collection_items_path, collections_path, favorites_path, item_path, latest_path, library_path,
    movies_path, next_up_feed_path, resume_path, search_path, series_path, similar_path,
    top_rated_path,
};
use crate::session::ServerInfo;
use crate::transport::{CancelFlag, Method, Transport};

impl<T: Transport> JellyfinClient<T> {
    pub async fn server_info(&self) -> Result<ServerInfo, JellyfinError> {
        let dto: crate::dto::ServerInfoDto = self
            .get_json(Endpoint::System, "/System/Info/Public", "server info")
            .await?;
        Ok(map_server_info(dto))
    }

    pub async fn home_feed(&self) -> Result<HomeFeed, JellyfinError> {
        let user_id = self.session().user().id().as_str();
        let resume_path = resume_path(user_id);
        let latest_path = latest_path(user_id);
        let movies_path = movies_path(user_id);
        let series_path = series_path(user_id);
        let top_rated_path = top_rated_path(user_id);
        let favorites_path = favorites_path(user_id);
        let (resume, latest, movies, series, top_rated, favorites) = futures::join!(
            self.items(Endpoint::Home, &resume_path),
            self.items(Endpoint::Home, &latest_path),
            self.items(Endpoint::Home, &movies_path),
            self.items(Endpoint::Home, &series_path),
            self.items(Endpoint::Home, &top_rated_path),
            self.items(Endpoint::Home, &favorites_path),
        );
        Ok(HomeFeed {
            resume: resume?,
            latest: latest?,
            movies: movies?,
            series: series?,
            top_rated: empty_shelf("top rated", top_rated),
            favorites: empty_shelf("favorites", favorites),
        })
    }

    /// One native Home shelf. Each shelf is a separate request so a failure
    /// stays with that shelf. Paths match the shipping Home queries; Next Up
    /// is native (shipping Home has no such row).
    pub async fn home_shelf(&self, shelf: HomeShelf) -> Result<Vec<MediaItem>, JellyfinError> {
        let user_id = self.session().user().id().as_str();
        let path = match shelf {
            HomeShelf::ContinueWatching => resume_path(user_id),
            HomeShelf::NextUp => next_up_feed_path(user_id),
            HomeShelf::RecentMovies => movies_path(user_id),
            HomeShelf::RecentSeries => series_path(user_id),
            HomeShelf::Favorites => favorites_path(user_id),
        };
        self.items(Endpoint::Home, &path).await
    }

    pub async fn library_items(
        &self,
        kind: LibraryKind,
        sort: LibrarySort,
    ) -> Result<Vec<MediaItem>, JellyfinError> {
        let user_id = self.session().user().id().as_str();
        self.items(Endpoint::Library, &library_path(user_id, kind, sort))
            .await
    }

    pub async fn item_details(&self, item_id: &ItemId) -> Result<MediaItem, JellyfinError> {
        let user_id = self.session().user().id().as_str();
        let dto: ItemDto = self
            .get_json(Endpoint::Item, &item_path(user_id, item_id), "item")
            .await?;
        item_from_dto(dto)
    }

    pub async fn similar_items(
        &self,
        item_id: &ItemId,
        limit: u32,
    ) -> Result<Vec<MediaItem>, JellyfinError> {
        let user_id = self.session().user().id().as_str();
        self.items(Endpoint::Item, &similar_path(user_id, item_id, limit))
            .await
    }

    pub async fn item_collections(
        &self,
        item_id: &ItemId,
    ) -> Result<Vec<CollectionContext>, JellyfinError> {
        let user_id = self.session().user().id().as_str();
        let collections = self
            .items(Endpoint::Item, &collections_path(user_id, item_id))
            .await?;
        let mut contexts = Vec::with_capacity(collections.len());
        for collection in collections {
            let items = self
                .items(
                    Endpoint::Item,
                    &collection_items_path(user_id, collection.id()),
                )
                .await?;
            contexts.push(CollectionContext { collection, items });
        }
        Ok(contexts)
    }

    pub async fn search_library(
        &self,
        term: &str,
        cancel: Option<&CancelFlag>,
    ) -> Result<Vec<MediaItem>, JellyfinError> {
        let user_id = self.session().user().id().as_str();
        let path = search_path(user_id, term);
        let response = self
            .request(Endpoint::Search, Method::Get, &path, None, cancel)
            .await?;
        let dto: ItemsDto = serde_json::from_slice(&response.body)
            .map_err(|_| JellyfinError::malformed("items"))?;
        let items = dto.items.ok_or_else(|| JellyfinError::malformed("items"))?;
        items_from_dtos(items)
    }

    pub async fn set_item_favorite(
        &self,
        item_id: &ItemId,
        favorite: bool,
    ) -> Result<(), JellyfinError> {
        self.user_flag("FavoriteItems", item_id, favorite).await
    }

    pub async fn set_item_played(
        &self,
        item_id: &ItemId,
        played: bool,
    ) -> Result<(), JellyfinError> {
        self.user_flag("PlayedItems", item_id, played).await
    }

    /// Read user data, zero the resume position and played percentage, and
    /// write the rest of the object back.
    pub async fn clear_item_progress(&self, item_id: &ItemId) -> Result<(), JellyfinError> {
        let path = format!("/UserItems/{}/UserData", encode_component(item_id.as_str()));
        let response = self
            .request(Endpoint::UserData, Method::Get, &path, None, None)
            .await?;
        let mut value: serde_json::Value = serde_json::from_slice(&response.body)
            .map_err(|_| JellyfinError::malformed("user data"))?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| JellyfinError::malformed("user data"))?;
        object.insert("PlaybackPositionTicks".to_string(), json!(0));
        object.insert("PlayedPercentage".to_string(), json!(0));
        let body =
            serde_json::to_string(&value).map_err(|_| JellyfinError::malformed("user data"))?;
        self.request(Endpoint::UserData, Method::Post, &path, Some(body), None)
            .await?;
        Ok(())
    }

    async fn user_flag(
        &self,
        collection: &str,
        item_id: &ItemId,
        enabled: bool,
    ) -> Result<(), JellyfinError> {
        let path = format!(
            "/Users/{}/{}/{}",
            encode_component(self.session().user().id().as_str()),
            collection,
            encode_component(item_id.as_str())
        );
        let method = if enabled {
            Method::Post
        } else {
            Method::Delete
        };
        self.request(Endpoint::UserData, method, &path, None, None)
            .await?;
        Ok(())
    }

    pub(crate) async fn items(
        &self,
        endpoint: Endpoint,
        path: &str,
    ) -> Result<Vec<MediaItem>, JellyfinError> {
        let dto: ItemsDto = self.get_json(endpoint, path, "items").await?;
        let items = dto.items.ok_or_else(|| JellyfinError::malformed("items"))?;
        items_from_dtos(items)
    }
}

fn empty_shelf(shelf: &str, result: Result<Vec<MediaItem>, JellyfinError>) -> Vec<MediaItem> {
    match result {
        Ok(items) => items,
        Err(error) => {
            log::warn!(target: "matinee_jellyfin", "home shelf {shelf} failed: {error}");
            Vec::new()
        }
    }
}

fn map_server_info(dto: crate::dto::ServerInfoDto) -> ServerInfo {
    ServerInfo {
        name: dto.server_name,
        version: dto.version,
        operating_system: dto.operating_system,
        product: dto.product_name,
        id: dto.id,
    }
}
