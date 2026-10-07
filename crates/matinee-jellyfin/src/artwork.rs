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
//! [`ArtworkRequest`] is the native shape: the address has no token.
//! [`JellyfinClient::fetch_artwork`] sends it with
//! [`Session::authorization_header`](crate::Session::authorization_header).
//! This crate returns encoded bytes. It does not decode or draw them.

use matinee_core::{ImageRole, ImageTag, ItemId, MediaItem, Person};

use crate::client::{Endpoint, JellyfinClient};
use crate::error::JellyfinError;
use crate::query::{Query, encode_component};
use crate::session::Session;
use crate::transport::{CancelFlag, Method, Transport};

/// Largest artwork body Matinee accepts. The widths Matinee asks for are far smaller.
pub const MAX_ARTWORK_BYTES: usize = 16 * 1024 * 1024;

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

    /// Native request for an image the item has, with its tag so a cached copy
    /// changes when the artwork does. `None` when Jellyfin has no such image.
    pub fn item_request(
        &self,
        item: &MediaItem,
        role: ImageRole,
        width: u32,
    ) -> Option<ArtworkRequest> {
        let tag = match role {
            ImageRole::Primary => item.artwork.primary.as_ref(),
            ImageRole::Backdrop => item.artwork.backdrop(0),
            ImageRole::Logo => item.artwork.logo.as_ref(),
        }?;
        Some(ArtworkRequest {
            url: self.item_image(item.id(), role.as_str(), Some(tag), width, 90, false),
        })
    }

    /// Native cast portrait. `None` when the person has no id or no image.
    pub fn person_request(&self, person: &Person, width: u32) -> Option<ArtworkRequest> {
        let id = person.id.as_ref()?;
        let tag = person.image.as_ref()?;
        Some(ArtworkRequest {
            url: self.item_image(id, "Primary", Some(tag), width, 88, false),
        })
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

impl<T: Transport> JellyfinClient<T> {
    /// Encoded artwork bytes for a native request, sent with the session
    /// authorization header. The address must be on this session's server;
    /// a body over [`MAX_ARTWORK_BYTES`] is refused.
    pub async fn fetch_artwork(
        &self,
        request: &ArtworkRequest,
        cancel: Option<&CancelFlag>,
    ) -> Result<Vec<u8>, JellyfinError> {
        let path = request
            .url
            .strip_prefix(self.session().server_url())
            .filter(|path| path.starts_with('/'))
            .ok_or_else(|| {
                JellyfinError::invalid_url("The artwork address is not on this Jellyfin server.")
            })?;
        let response = self
            .request(Endpoint::Artwork, Method::Get, path, None, cancel)
            .await?;
        if response.body.len() > MAX_ARTWORK_BYTES {
            return Err(JellyfinError::malformed("artwork"));
        }
        Ok(response.body)
    }
}
