//! Artwork URLs.
//!
//! Availability lives on the domain item. These builders only turn an id,
//! a role, and a width into an address. The presentation layer does not
//! construct them.
//!
//! The shipping app puts `api_key=<token>` on image URLs because its image
//! loader cannot set the authorization header. [`ArtworkUrls`] keeps that
//! shape so a URL-only loader still works. The token can then show up in a
//! log, a proxy, or an image cache.
//!
//! [`ArtworkRequest`] is the native shape: the address has no token. A Phase 3
//! loader sends [`Session::authorization_header`](crate::Session::authorization_header)
//! with it. This crate does not change how images are drawn.

use matinee_core::{ImageRole, ImageTag, ItemId, MediaItem};

use crate::query::{Query, encode_component};
use crate::session::Session;

/// An image address with no access token.
///
/// The loader that fetches it must send the session authorization header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtworkRequest {
    pub url: String,
}

pub struct ArtworkUrls<'a> {
    session: &'a Session,
}

impl<'a> ArtworkUrls<'a> {
    pub(crate) fn new(session: &'a Session) -> Self {
        Self { session }
    }

    pub fn image(&self, item_id: &ItemId, role: ImageRole, width: u32) -> String {
        self.item_image(item_id, role.as_str(), None, width, 90, true)
    }

    /// Native image address. No `api_key`. Phase 3 attaches the session header.
    pub fn image_request(&self, item_id: &ItemId, role: ImageRole, width: u32) -> ArtworkRequest {
        ArtworkRequest {
            url: self.item_image(item_id, role.as_str(), None, width, 90, false),
        }
    }

    /// Backdrop when the item has one, otherwise the primary image.
    pub fn backdrop(&self, item: &MediaItem, width: u32) -> String {
        let role = if item.artwork.has_backdrop() {
            ImageRole::Backdrop
        } else {
            ImageRole::Primary
        };
        self.image(item.id(), role, width)
    }

    pub fn backdrop_image(
        &self,
        item_id: &ItemId,
        index: u32,
        tag: Option<&ImageTag>,
        width: u32,
    ) -> String {
        let path = format!(
            "/Items/{}/Images/Backdrop/{index}",
            encode_component(item_id.as_str())
        );
        self.url(&path, width, 90, tag, true)
    }

    pub fn chapter_image(
        &self,
        item_id: &ItemId,
        chapter_index: u32,
        width: u32,
        tag: Option<&ImageTag>,
    ) -> String {
        let path = format!(
            "/Items/{}/Images/Chapter/{chapter_index}",
            encode_component(item_id.as_str())
        );
        self.url(&path, width, 88, tag, true)
    }

    pub fn user_image(&self, width: u32) -> String {
        let path = format!(
            "/Users/{}/Images/Primary",
            encode_component(self.session.user().id().as_str())
        );
        self.url(&path, width, 90, self.session.user().avatar.as_ref(), true)
    }

    /// Cast portrait. The shipping details view builds this beside the client.
    pub fn person_image(&self, person_id: &ItemId, width: u32) -> String {
        self.item_image(person_id, "Primary", None, width, 88, true)
    }

    fn item_image(
        &self,
        item_id: &ItemId,
        role: &str,
        tag: Option<&ImageTag>,
        width: u32,
        quality: u32,
        api_key: bool,
    ) -> String {
        let path = format!(
            "/Items/{}/Images/{role}",
            encode_component(item_id.as_str())
        );
        self.url(&path, width, quality, tag, api_key)
    }

    fn url(
        &self,
        path: &str,
        width: u32,
        quality: u32,
        tag: Option<&ImageTag>,
        api_key: bool,
    ) -> String {
        let mut query = Query::new()
            .pair("maxWidth", width.to_string())
            .pair("quality", quality.to_string());
        if api_key {
            query = query.pair("api_key", self.session.access_token());
        }
        if let Some(tag) = tag {
            query = query.pair("tag", tag.as_str());
        }
        format!("{}{path}?{}", self.session.server_url(), query.encode())
    }
}
