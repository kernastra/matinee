//! Jellyfin artwork for native screens.
//!
//! A screen asks for an [`ArtworkRequest`] (no token in the address).
//! [`ArtworkLoader::load`] answers from a small bounded cache or starts a
//! fetch on the [`ServiceRuntime`]: `JellyfinClient::fetch_artwork` sends the
//! session authorization header, and the bytes are decoded on a blocking
//! thread into an Atelier [`DecodedImage`]. The GPUI thread only awaits the
//! result. It never sees a URL with a token, an HTTP client, or a decoder.
//!
//! The screen owns the returned task. Dropping or aborting it cancels the
//! fetch, which is how a screen that closes or moves on stops loading art it
//! no longer shows. Which response is current is the screen's decision.
//!
//! The cache is shared by every screen on the GPUI thread. It is bounded by
//! decoded bytes; an evicted image is released from the window atlases.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use atelier_ui::prelude::*;
use matinee_jellyfin::{ArtworkRequest, JellyfinClient, JellyfinError, ReqwestTransport};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::runtime::ServiceRuntime;

/// Decoded artwork kept for reuse across screens. About a dozen backdrops.
pub(crate) const ARTWORK_CACHE_BYTES: usize = 96 * 1024 * 1024;

/// Requested widths. Jellyfin scales on the server, so decoded size follows.
pub(crate) const BACKDROP_WIDTH: u32 = 1920;
pub(crate) const POSTER_WIDTH: u32 = 480;
pub(crate) const THUMB_WIDTH: u32 = 480;
pub(crate) const PORTRAIT_WIDTH: u32 = 240;

/// Longest decoded side Matinee accepts for any artwork.
const MAX_SIDE: u32 = 2560;

/// What one artwork slot shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Artwork {
    /// Nothing requested yet, or the request is in flight.
    #[default]
    Loading,
    Ready(DecodedImage),
    /// Jellyfin has no such image. Not an error.
    Missing,
    /// The fetch or the decode failed. The screen shows its placeholder.
    Failed,
}

impl Artwork {
    pub(crate) fn image(&self) -> Option<&DecodedImage> {
        match self {
            Self::Ready(image) => Some(image),
            _ => None,
        }
    }
}

pub(crate) type Client = Arc<JellyfinClient<ReqwestTransport>>;

/// The outcome of a fetch, ready to apply on the GPUI thread.
pub(crate) type ArtworkResult = Artwork;

/// A cache hit, or a fetch whose task the caller owns.
pub(crate) enum ArtworkLoad {
    Cached(DecodedImage),
    Pending {
        task: JoinHandle<()>,
        result: oneshot::Receiver<ArtworkResult>,
    },
}

/// Starts artwork fetches and remembers finished ones.
#[derive(Clone)]
pub(crate) struct ArtworkLoader {
    runtime: Arc<ServiceRuntime>,
    cache: Rc<RefCell<ArtworkCache>>,
}

impl ArtworkLoader {
    pub(crate) fn new(runtime: Arc<ServiceRuntime>) -> Self {
        Self {
            runtime,
            cache: Rc::new(RefCell::new(ArtworkCache::new(ARTWORK_CACHE_BYTES))),
        }
    }

    pub(crate) fn load(&self, client: &Client, request: ArtworkRequest) -> ArtworkLoad {
        if let Some(image) = self.cache.borrow_mut().get(&request.url) {
            return ArtworkLoad::Cached(image);
        }
        let client = Arc::clone(client);
        let (task, result) = self
            .runtime
            .spawn(async move { fetch_and_decode(&client, &request).await });
        ArtworkLoad::Pending { task, result }
    }

    /// Whether the cache holds this exact image, so its owner must not release it.
    pub(crate) fn is_cached(&self, image: &DecodedImage) -> bool {
        self.cache.borrow().contains(image)
    }

    /// Keep a finished image for other screens. Evicted images leave the atlas.
    pub(crate) fn remember(&self, key: &str, image: &DecodedImage, cx: &mut App) {
        let evicted = self.cache.borrow_mut().insert(key, image.clone());
        for image in evicted {
            image.release(cx);
        }
    }
}

/// Fetch with the session header, then decode off the async workers.
pub(crate) async fn fetch_and_decode(client: &Client, request: &ArtworkRequest) -> ArtworkResult {
    let bytes = match client.fetch_artwork(request, None).await {
        Ok(bytes) => bytes,
        Err(JellyfinError::NotFound) => return Artwork::Missing,
        Err(_) => return Artwork::Failed,
    };
    match tokio::task::spawn_blocking(move || DecodedImage::decode(&bytes, MAX_SIDE)).await {
        Ok(Ok(image)) => Artwork::Ready(image),
        _ => Artwork::Failed,
    }
}

/// Least-recently-used decoded images, bounded by pixel bytes.
pub(crate) struct ArtworkCache {
    budget: usize,
    bytes: usize,
    entries: VecDeque<(String, DecodedImage)>,
}

impl ArtworkCache {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            budget,
            bytes: 0,
            entries: VecDeque::new(),
        }
    }

    pub(crate) fn get(&mut self, key: &str) -> Option<DecodedImage> {
        let index = self.entries.iter().position(|(entry, _)| entry == key)?;
        let entry = self.entries.remove(index)?;
        let image = entry.1.clone();
        self.entries.push_back(entry);
        Some(image)
    }

    /// Insert as most recent and return what had to leave. An image larger
    /// than the whole budget is not kept.
    pub(crate) fn insert(&mut self, key: &str, image: DecodedImage) -> Vec<DecodedImage> {
        let mut evicted = Vec::new();
        if let Some(index) = self.entries.iter().position(|(entry, _)| entry == key)
            && let Some((_, old)) = self.entries.remove(index)
        {
            self.bytes -= old.byte_size();
            if old != image {
                evicted.push(old);
            }
        }
        if image.byte_size() > self.budget {
            evicted.push(image);
            return evicted;
        }
        self.bytes += image.byte_size();
        self.entries.push_back((key.to_string(), image));
        while self.bytes > self.budget {
            let Some((_, oldest)) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= oldest.byte_size();
            evicted.push(oldest);
        }
        evicted
    }

    pub(crate) fn contains(&self, image: &DecodedImage) -> bool {
        self.entries.iter().any(|(_, entry)| entry == image)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×2 PNG.
    pub(crate) fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 120, 40, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    fn decoded() -> DecodedImage {
        DecodedImage::decode(&png(), 64).unwrap()
    }

    #[test]
    fn fetches_send_the_session_header_and_map_outcomes() {
        use crate::test_support::{FIXTURE_TOKEN, Reply, client, fake_jellyfin};
        use matinee_core::{ImageRole, ItemId};

        let (address, requests) = fake_jellyfin(vec![
            Reply::Status(200, png()),
            Reply::Status(404, Vec::new()),
            Reply::Status(200, b"<html>not art</html>".to_vec()),
        ]);
        let client = client(&address);
        let runtime = ServiceRuntime::new().unwrap();
        let request = |id: &str| {
            client.session().artwork().image_request(
                &ItemId::parse(id).unwrap(),
                ImageRole::Primary,
                POSTER_WIDTH,
            )
        };
        let mut outcomes = Vec::new();
        for id in ["poster", "missing", "garbage"] {
            let client = Arc::clone(&client);
            let request = request(id);
            let (_task, rx) =
                runtime.spawn(async move { fetch_and_decode(&client, &request).await });
            outcomes.push(rx.blocking_recv().unwrap());
        }
        assert!(matches!(outcomes[0], Artwork::Ready(ref image) if image.width() == 2));
        assert_eq!(outcomes[1], Artwork::Missing);
        assert_eq!(outcomes[2], Artwork::Failed);

        let first = requests.recv().unwrap();
        let line = first.lines().next().unwrap();
        assert!(
            line.starts_with("GET /Items/poster/Images/Primary?"),
            "{line}"
        );
        assert!(!line.contains("api_key"));
        assert!(!line.contains(FIXTURE_TOKEN));
        assert!(
            first.lines().any(
                |header| header.to_ascii_lowercase().starts_with("authorization:")
                    && header.contains(FIXTURE_TOKEN)
            ),
            "the token travels in the header"
        );
    }

    #[test]
    fn the_cache_evicts_the_least_recent_image_past_its_budget() {
        let one = decoded().byte_size();
        let mut cache = ArtworkCache::new(one * 2);
        let (a, b, c) = (decoded(), decoded(), decoded());
        assert!(cache.insert("a", a.clone()).is_empty());
        assert!(cache.insert("b", b.clone()).is_empty());
        // Reading `a` makes `b` the oldest.
        assert_eq!(cache.get("a"), Some(a.clone()));
        assert_eq!(cache.insert("c", c.clone()), vec![b]);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get("b"), None);
        assert_eq!(cache.get("c"), Some(c));
    }

    #[test]
    fn an_image_larger_than_the_budget_is_not_kept() {
        let mut cache = ArtworkCache::new(1);
        let image = decoded();
        assert_eq!(cache.insert("big", image.clone()), vec![image]);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn replacing_a_key_releases_the_old_image() {
        let mut cache = ArtworkCache::new(usize::MAX);
        let (old, new) = (decoded(), decoded());
        cache.insert("poster", old.clone());
        assert_eq!(cache.insert("poster", new.clone()), vec![old]);
        assert_eq!(cache.get("poster"), Some(new));
    }
}
