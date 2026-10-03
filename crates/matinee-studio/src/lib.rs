//! Poster Studio services: provider credentials, Codex and fal generation,
//! movie manifests, and media-folder artwork.
//!
//! This crate does not know Tauri, GPUI, or Atelier. The application supplies
//! [`StudioPaths`] (home, app data, and an optional Pictures directory) and
//! polls [`ReqwestStudio`] and [`TokioProcess`] on a Tokio runtime this crate
//! does not create.
//!
//! # Prompt boundary
//!
//! Creative direction stays in `src/lib/posterPrompts.ts`. [`GenerationRequest`]
//! is the validated brief the service executes. This crate does not compose
//! the poster prompt.
//!
//! # Credentials
//!
//! fal and Higgsfield keys are raw strings in the shipping namespace
//! `dev.sean.matinee.image-generation`. Codex does not use that vault. A saved
//! Codex executable path lives in app data (`codex-cli-path`) and is
//! revalidated on every run.
//!
//! # Generation
//!
//! One Codex job is active per [`Studio`] value. Dropping the generation
//! future kills the child (`kill_on_drop`) and a reap thread waits for it.
//! The 12-minute timeout is the other
//! cancellation path. There is no separate cancel method. Higgsfield can be
//! discovered and can store a key; generation returns the shipping
//! "not available yet" error.
//!
//! # Images
//!
//! Provider downloads check scheme, credentials, redirects, declared content
//! type, magic bytes, and a byte cap. Magic-byte checks are stricter than the
//! shipping app, which trusted `Content-Type`. fal image URLs may be followed
//! for at most three validated hops. Reference downloads do not follow
//! redirects and must share the Jellyfin server's origin.

#[cfg(test)]
mod behavior;
mod codex;
mod error;
mod http;
mod images;
mod manifest;
mod model;
mod paths;
mod process;
mod redact;
mod service;

pub use codex::{
    CODEX_LOG_RETENTION, cleanup_expired_codex_jobs, codex_arguments, executable_candidates,
    find_executable, friendly_codex_failure,
};
pub use error::StudioError;
pub use http::{ReqwestStudio, StudioHttp, StudioRequest, StudioResponse};
pub use images::validate_provider_image_url;
pub use manifest::{MovieManifest, load_movie_manifest, manifest_json};
pub use model::{
    ArtworkKind, CustomPoster, GeneratedImage, GenerationRequest, ImageProvider,
    LocalProviderStatus, MAX_IMAGE_BYTES, MAX_MANIFEST_BYTES, MAX_PROMPT_CHARACTERS,
    MAX_REFERENCE_BYTES, ProviderKeyStatus,
};
pub use paths::StudioPaths;
pub use process::{ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, TokioProcess};
pub use service::{Studio, safe_file_stem, valid_item_id};
