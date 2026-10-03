//! Provider identity and the records Poster Studio already returns.

use serde::{Deserialize, Serialize};

use crate::error::StudioError;

/// Image providers Matinee knows about.
///
/// Codex and fal generate images. Higgsfield can be discovered and can store a
/// key, but generation is not implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageProvider {
    Codex,
    Fal,
    Higgsfield,
}

impl ImageProvider {
    pub fn parse(value: &str) -> Result<Self, StudioError> {
        match value {
            "codex" => Ok(Self::Codex),
            "fal" => Ok(Self::Fal),
            "higgsfield" => Ok(Self::Higgsfield),
            _ => Err(StudioError::UnknownProvider),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Fal => "fal",
            Self::Higgsfield => "higgsfield",
        }
    }

    pub fn generation_implemented(self) -> bool {
        matches!(self, Self::Codex | Self::Fal)
    }

    pub fn requires_api_key(self) -> bool {
        matches!(self, Self::Fal | Self::Higgsfield)
    }

    pub fn local_executable(self) -> Option<&'static str> {
        match self {
            Self::Codex => Some("codex"),
            Self::Higgsfield => Some("higgsfield"),
            Self::Fal => None,
        }
    }

    pub(crate) fn key_account(self) -> Result<&'static str, StudioError> {
        match self {
            Self::Fal => Ok("fal"),
            Self::Higgsfield => Ok("higgsfield"),
            Self::Codex => Err(StudioError::provider(
                "This provider does not use a Matinee-managed API key.",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtworkKind {
    Poster,
    Backdrop,
    Banner,
    Thumbnail,
}

impl ArtworkKind {
    pub fn parse(value: &str) -> Result<Self, StudioError> {
        match value {
            "Poster" => Ok(Self::Poster),
            "Backdrop" => Ok(Self::Backdrop),
            "Banner" => Ok(Self::Banner),
            "Thumbnail" => Ok(Self::Thumbnail),
            _ => Err(StudioError::UnsupportedArtwork),
        }
    }

    pub fn spec(self) -> (&'static str, u32, u32) {
        match self {
            Self::Poster => ("vertical 2:3 poster", 1024, 1536),
            Self::Backdrop => ("wide 16:9 backdrop", 1536, 864),
            Self::Banner => ("ultra-wide 12:5 banner", 1536, 640),
            Self::Thumbnail => ("landscape 16:9 thumbnail", 1280, 720),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalProviderStatus {
    pub provider: ImageProvider,
    pub found: bool,
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKeyStatus {
    pub provider: ImageProvider,
    pub configured: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedImage {
    pub provider: ImageProvider,
    pub local_path: String,
    pub data_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPoster {
    pub item_id: String,
    pub local_path: String,
    pub data_url: String,
}

#[derive(Clone, Debug)]
pub struct GenerationRequest {
    pub provider: ImageProvider,
    pub prompt: String,
    pub reference_urls: Vec<String>,
    pub jellyfin_server_url: String,
    pub asset: ArtworkKind,
}

pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_REFERENCE_BYTES: usize = 12 * 1024 * 1024;
pub const MAX_PROMPT_CHARACTERS: usize = 30_000;
pub const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

pub(crate) const CODEX_FILESYSTEM_PERMISSIONS: &str = "permissions.matinee-poster.filesystem={\":root\"=\"deny\",\":minimal\"=\"read\",\":workspace_roots\"={\".\"=\"write\"}}";
