//! Artwork identity.
//!
//! A missing image is normal. These types say whether a picture exists.
//! They do not build a URL.

use std::fmt;

/// Opaque cache tag for one image. Empty tags are treated as missing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageTag(String);

impl ImageTag {
    pub fn parse(value: impl AsRef<str>) -> Option<Self> {
        let value = value.as_ref().trim();
        if value.is_empty()
            || value.len() > 512
            || value.chars().any(|character| character.is_control())
        {
            return None;
        }
        Some(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ImageTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which picture a URL builder should request.
///
/// Chapter images and avatars use their own paths, so they are not roles on
/// an item image URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageRole {
    Primary,
    Backdrop,
    Logo,
}

impl ImageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Backdrop => "Backdrop",
            Self::Logo => "Logo",
        }
    }
}

/// Artwork that may be absent. Absence is not an error.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemArtwork {
    pub primary: Option<ImageTag>,
    pub logo: Option<ImageTag>,
    pub backdrops: Vec<ImageTag>,
}

impl ItemArtwork {
    pub fn has_primary(&self) -> bool {
        self.primary.is_some()
    }

    pub fn has_logo(&self) -> bool {
        self.logo.is_some()
    }

    pub fn backdrop(&self, index: usize) -> Option<&ImageTag> {
        self.backdrops.get(index)
    }

    pub fn has_backdrop(&self) -> bool {
        !self.backdrops.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_artwork_is_empty_not_an_error() {
        let artwork = ItemArtwork::default();
        assert!(!artwork.has_primary());
        assert!(!artwork.has_logo());
        assert!(!artwork.has_backdrop());
        assert!(ImageTag::parse("  ").is_none());
        assert!(ImageTag::parse("tag").is_some());
    }
}
