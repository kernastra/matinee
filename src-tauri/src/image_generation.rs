//! Tauri adapter for Poster Studio.
//!
//! Path roots come from the Tauri app handle. Credentials, Codex, fal, and
//! media-folder writes live in `matinee-studio`.

use matinee_secrets::Secret;
use matinee_studio::{
    manifest_json, ArtworkKind, CustomPoster, GeneratedImage, GenerationRequest, ImageProvider,
    LocalProviderStatus, ProviderKeyStatus,
};
use tauri::{AppHandle, State};

use crate::services::{studio_paths, PosterService};

#[tauri::command]
pub fn load_movie_manifest(
    app: AppHandle,
    studio: State<'_, PosterService>,
    media_path: String,
) -> Result<Option<serde_json::Value>, String> {
    let paths = studio_paths(&app)?;
    match studio
        .load_manifest(&paths, &media_path)
        .map_err(|error| error.to_string())?
    {
        None => Ok(None),
        Some(manifest) => manifest_json(&manifest)
            .map(Some)
            .map_err(|error| error.to_string()),
    }
}

#[tauri::command]
pub fn list_custom_posters(
    app: AppHandle,
    studio: State<'_, PosterService>,
) -> Result<Vec<CustomPoster>, String> {
    let paths = studio_paths(&app)?;
    studio
        .list_custom_posters(&paths)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn assign_generated_poster(
    app: AppHandle,
    studio: State<'_, PosterService>,
    item_id: String,
    local_path: String,
) -> Result<CustomPoster, String> {
    let paths = studio_paths(&app)?;
    studio
        .assign_generated_poster(&paths, &item_id, &local_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn export_generated_image(
    app: AppHandle,
    studio: State<'_, PosterService>,
    local_path: String,
    title: String,
) -> Result<String, String> {
    let paths = studio_paths(&app)?;
    studio
        .export_generated_image(&paths, &local_path, &title)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn export_poster_to_media_folder(
    app: AppHandle,
    studio: State<'_, PosterService>,
    local_path: String,
    media_path: String,
    overwrite: bool,
) -> Result<String, String> {
    let paths = studio_paths(&app)?;
    studio
        .export_poster_to_media_folder(&paths, &local_path, &media_path, overwrite)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn scan_local_image_provider(
    app: AppHandle,
    studio: State<'_, PosterService>,
    provider: String,
) -> Result<LocalProviderStatus, String> {
    let paths = studio_paths(&app)?;
    let provider = match provider.as_str() {
        "codex" => ImageProvider::Codex,
        "higgsfield" => ImageProvider::Higgsfield,
        _ => return Err("Only Codex and Higgsfield have local CLI providers.".into()),
    };
    studio
        .scan_local(&paths, provider)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn provider_key_status(
    studio: State<'_, PosterService>,
    provider: String,
) -> Result<ProviderKeyStatus, String> {
    let provider = ImageProvider::parse(&provider).map_err(|error| error.to_string())?;
    studio
        .provider_key_status(provider)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_provider_key(
    studio: State<'_, PosterService>,
    provider: String,
    key: String,
) -> Result<ProviderKeyStatus, String> {
    let provider = ImageProvider::parse(&provider).map_err(|error| error.to_string())?;
    studio
        .save_provider_key(provider, Secret::new(key))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remove_provider_key(
    studio: State<'_, PosterService>,
    provider: String,
) -> Result<ProviderKeyStatus, String> {
    let provider = ImageProvider::parse(&provider).map_err(|error| error.to_string())?;
    studio
        .remove_provider_key(provider)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn generate_poster_image(
    app: AppHandle,
    studio: State<'_, PosterService>,
    provider: String,
    prompt: String,
    reference_urls: Vec<String>,
    jellyfin_server_url: String,
    asset_type: String,
) -> Result<GeneratedImage, String> {
    let paths = studio_paths(&app)?;
    let request = GenerationRequest {
        provider: ImageProvider::parse(&provider).map_err(|error| error.to_string())?,
        prompt,
        reference_urls,
        jellyfin_server_url,
        asset: ArtworkKind::parse(&asset_type).map_err(|error| error.to_string())?,
    };
    studio
        .generate(&paths, request)
        .await
        .map_err(|error| error.to_string())
}
