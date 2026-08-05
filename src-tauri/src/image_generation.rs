use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use keyring::{Entry, Error as KeyringError};
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    fs::File,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};
use tokio::{process::Command, time::timeout};

const KEYRING_SERVICE: &str = "dev.sean.matinee.image-generation";
const FAL_MODEL: &str = "fal-ai/flux/dev";
const FAL_REFERENCE_MODEL: &str = "fal-ai/flux-2/edit";
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_REFERENCE_BYTES: usize = 12 * 1024 * 1024;
const MAX_MOVIE_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

fn asset_spec(asset_type: &str) -> Result<(&'static str, u32, u32), String> {
    match asset_type {
        "Poster" => Ok(("vertical 2:3 poster", 1024, 1536)),
        "Backdrop" => Ok(("wide 16:9 backdrop", 1536, 864)),
        "Banner" => Ok(("ultra-wide 12:5 banner", 1536, 640)),
        "Thumbnail" => Ok(("landscape 16:9 thumbnail", 1280, 720)),
        _ => Err("Choose a supported Matinee artwork type.".into()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalProviderStatus {
    provider: String,
    found: bool,
    authenticated: bool,
    path: Option<String>,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKeyStatus {
    provider: String,
    configured: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedImage {
    provider: String,
    local_path: String,
    data_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPoster {
    item_id: String,
    local_path: String,
    data_url: String,
}

struct ReferenceAsset {
    bytes: Vec<u8>,
    content_type: String,
    extension: &'static str,
}

#[derive(Deserialize)]
struct FalImage {
    url: String,
    #[serde(default)]
    content_type: Option<String>,
}

#[derive(Deserialize)]
struct FalResponse {
    images: Vec<FalImage>,
}

fn supported_key_provider(provider: &str) -> Result<&str, String> {
    match provider {
        "fal" | "higgsfield" => Ok(provider),
        _ => Err("This provider does not use a Matinee-managed API key.".into()),
    }
}

fn mapped_media_candidates(media_path: &Path, home: &Path) -> Vec<PathBuf> {
    let local_roots = [home.join("media-data/media"), home.join("media")];
    let container_roots = [
        (Path::new("/media"), Path::new("")),
        (Path::new("/movies"), Path::new("movies")),
        (Path::new("/tv"), Path::new("tv")),
        (Path::new("/shows"), Path::new("shows")),
    ];

    container_roots
        .iter()
        .filter_map(|(container_root, local_subdirectory)| {
            media_path
                .strip_prefix(container_root)
                .ok()
                .map(|relative| (*local_subdirectory, relative))
        })
        .flat_map(|(local_subdirectory, relative)| {
            local_roots
                .iter()
                .map(move |root| root.join(local_subdirectory).join(relative))
        })
        .collect()
}

fn resolve_media_file(app: &AppHandle, media_path: &str) -> Result<PathBuf, String> {
    let reported_path = PathBuf::from(media_path);
    if reported_path.is_file() {
        return Ok(reported_path);
    }

    if let Ok(home) = app.path().home_dir() {
        if let Some(candidate) = mapped_media_candidates(&reported_path, &home)
            .into_iter()
            .find(|candidate| candidate.is_file())
        {
            return Ok(candidate);
        }
    }

    Err(format!(
        "Jellyfin reports this movie at {media_path}, but Matinee could not find the corresponding file on this computer. Check the Jellyfin container’s media path mapping."
    ))
}

#[tauri::command]
pub fn load_movie_manifest(
    app: AppHandle,
    media_path: String,
) -> Result<Option<serde_json::Value>, String> {
    let media_file = resolve_media_file(&app, &media_path)?;
    let movie_directory = media_file
        .parent()
        .ok_or_else(|| "Matinee could not determine this movie's folder.".to_string())?;
    let manifest_path = movie_directory.join("movie.mf.json");

    if !manifest_path.is_file() {
        return Ok(None);
    }

    let metadata = fs::metadata(&manifest_path)
        .map_err(|error| format!("Matinee could not inspect movie.mf.json: {error}"))?;
    if metadata.len() > MAX_MOVIE_MANIFEST_BYTES {
        return Err("movie.mf.json is larger than Matinee's 2 MB safety limit.".into());
    }

    let contents = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("Matinee could not read movie.mf.json: {error}"))?;
    let manifest: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|error| format!("movie.mf.json contains invalid JSON: {error}"))?;

    if manifest
        .get("manifestVersion")
        .and_then(|value| value.as_u64())
        != Some(1)
    {
        return Err("Matinee currently supports movie.mf.json manifestVersion 1.".into());
    }

    Ok(Some(manifest))
}

fn keyring_entry(provider: &str) -> Result<Entry, String> {
    let provider = supported_key_provider(provider)?;
    Entry::new(KEYRING_SERVICE, provider).map_err(|error| error.to_string())
}

fn get_provider_key(provider: &str) -> Result<String, String> {
    keyring_entry(provider)?
        .get_password()
        .map_err(|error| match error {
            KeyringError::NoEntry => format!("No {provider} API key is configured."),
            other => other.to_string(),
        })
}

fn executable_candidates(name: &str) -> Vec<PathBuf> {
    let executable = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let mut candidates = Vec::new();

    if let Some(path) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path).map(|directory| directory.join(&executable)));
    }

    if let Some(user_home) = env::var_os("HOME") {
        let user_home = PathBuf::from(user_home);
        candidates.extend([
            user_home.join(".local/bin").join(&executable),
            user_home.join(".cargo/bin").join(&executable),
            user_home.join(".bun/bin").join(&executable),
            user_home.join(".npm-global/bin").join(&executable),
        ]);
    }

    candidates.extend([
        PathBuf::from("/usr/local/bin").join(&executable),
        PathBuf::from("/usr/bin").join(&executable),
    ]);
    candidates
}

fn find_executable(name: &str) -> Option<PathBuf> {
    executable_candidates(name)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn generated_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("generated-posters");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

fn custom_posters_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("custom-posters");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

fn image_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

fn validated_poster_source(
    app: &AppHandle,
    local_path: &str,
    include_custom: bool,
) -> Result<PathBuf, String> {
    let source = PathBuf::from(local_path)
        .canonicalize()
        .map_err(|_| "The generated poster could not be found.".to_string())?;
    let generated_root = generated_directory(app)?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let in_generated = source.starts_with(generated_root);
    let in_custom = if include_custom {
        source.starts_with(
            custom_posters_directory(app)?
                .canonicalize()
                .map_err(|error| error.to_string())?,
        )
    } else {
        false
    };
    if (!in_generated && !in_custom) || !source.is_file() {
        return Err("Matinee can only use artwork it generated.".into());
    }
    Ok(source)
}

fn image_data_url(path: &Path, content_type: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("The generated image is larger than Matinee's 32 MB safety limit.".into());
    }
    Ok(format!(
        "data:{content_type};base64,{}",
        BASE64_STANDARD.encode(bytes)
    ))
}

fn image_file_in(directory: &Path) -> Result<PathBuf, String> {
    let mut images = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "webp"
                    )
                })
        })
        .collect::<Vec<_>>();
    images.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    images
        .pop()
        .ok_or_else(|| "Codex completed without saving a generated image.".into())
}

fn log_excerpt(path: &Path, limit: usize) -> String {
    let Ok(contents) = fs::read_to_string(path) else {
        return String::new();
    };
    let characters = contents.trim().chars().collect::<Vec<_>>();
    characters[characters.len().saturating_sub(limit)..]
        .iter()
        .collect()
}

fn friendly_codex_failure(message: &str) -> String {
    if message.contains("moderation_blocked") || message.contains("rejected by the safety system") {
        return "The image provider blocked this result during safety review. Character-focused franchise artwork is more likely to trigger this. Try Auto, Signature Element, Scene, or Environment focus, or choose fal.ai in Settings.".into();
    }
    if message.contains("rate_limit") || message.contains("Too Many Requests") {
        return "Codex is temporarily rate-limited. Wait a moment and try this generation again."
            .into();
    }
    if message.is_empty() {
        "Codex stopped without returning an image or diagnostic message.".into()
    } else {
        format!("Codex image generation failed: {message}")
    }
}

fn safe_file_stem(value: &str) -> String {
    let stem = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else if character.is_whitespace() {
                '-'
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches(['-', '_'])
        .to_lowercase();
    if stem.is_empty() {
        "matinee-poster".into()
    } else {
        stem.chars().take(72).collect()
    }
}

fn valid_item_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

#[tauri::command]
pub fn list_custom_posters(app: AppHandle) -> Result<Vec<CustomPoster>, String> {
    let directory = custom_posters_directory(&app)?;
    let mut posters = Vec::new();
    for path in fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
    {
        let Some(item_id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if !valid_item_id(item_id) {
            continue;
        }
        let content_type = image_content_type(&path);
        let Ok(data_url) = image_data_url(&path, content_type) else {
            continue;
        };
        posters.push(CustomPoster {
            item_id: item_id.into(),
            local_path: path.to_string_lossy().into_owned(),
            data_url,
        });
    }
    Ok(posters)
}

#[tauri::command]
pub fn assign_generated_poster(
    app: AppHandle,
    item_id: String,
    local_path: String,
) -> Result<CustomPoster, String> {
    if !valid_item_id(&item_id) {
        return Err("The Jellyfin item identifier is not valid.".into());
    }
    let source = validated_poster_source(&app, &local_path, false)?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|value| matches!(value.as_str(), "png" | "jpg" | "jpeg" | "webp"))
        .unwrap_or_else(|| "png".into());
    let directory = custom_posters_directory(&app)?;
    let destination = directory.join(format!("{item_id}.{extension}"));
    fs::copy(&source, &destination).map_err(|error| error.to_string())?;
    for old_extension in ["png", "jpg", "jpeg", "webp"] {
        let old_path = directory.join(format!("{item_id}.{old_extension}"));
        if old_path != destination && old_path.exists() {
            let _ = fs::remove_file(old_path);
        }
    }
    let content_type = image_content_type(&destination);
    Ok(CustomPoster {
        item_id,
        local_path: destination.to_string_lossy().into_owned(),
        data_url: image_data_url(&destination, content_type)?,
    })
}

#[tauri::command]
pub fn export_generated_image(
    app: AppHandle,
    local_path: String,
    title: String,
) -> Result<String, String> {
    let source = validated_poster_source(&app, &local_path, true)?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| matches!(*value, "png" | "jpg" | "jpeg" | "webp"))
        .unwrap_or("png");
    let pictures = app
        .path()
        .picture_dir()
        .map_err(|_| "Your Pictures folder could not be located.".to_string())?
        .join("Matinee");
    fs::create_dir_all(&pictures).map_err(|error| error.to_string())?;
    let destination = pictures.join(format!(
        "{}-{}.{}",
        safe_file_stem(&title),
        timestamp_millis(),
        extension
    ));
    fs::copy(source, &destination).map_err(|error| error.to_string())?;
    Ok(destination.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn export_poster_to_media_folder(
    app: AppHandle,
    local_path: String,
    media_path: String,
    overwrite: bool,
) -> Result<String, String> {
    let source = validated_poster_source(&app, &local_path, true)?;
    let media_file = resolve_media_file(&app, &media_path)?;
    let video_extension = media_file
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        video_extension.as_str(),
        "mkv" | "mp4" | "m4v" | "avi" | "mov" | "webm" | "ts" | "m2ts" | "iso"
    ) {
        return Err("Jellyfin did not provide a recognized movie-file path.".into());
    }
    let directory = media_file
        .parent()
        .ok_or_else(|| "The movie folder could not be resolved.".to_string())?;
    let destination = directory.join("poster.jpg");
    if destination.exists() && !overwrite {
        return Err("POSTER_EXISTS".into());
    }

    let decoded = image::open(&source)
        .map_err(|error| format!("The generated poster could not be decoded: {error}"))?;
    let temporary = directory.join(format!(".matinee-poster-{}.jpg", timestamp_millis()));
    let file = File::create(&temporary).map_err(|error| error.to_string())?;
    if let Err(error) =
        image::codecs::jpeg::JpegEncoder::new_with_quality(file, 92).encode_image(&decoded)
    {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    if destination.exists() {
        fs::remove_file(&destination).map_err(|error| error.to_string())?;
    }
    fs::rename(&temporary, &destination).map_err(|error| error.to_string())?;
    Ok(destination.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::{asset_spec, friendly_codex_failure, mapped_media_candidates};
    use std::path::{Path, PathBuf};

    #[test]
    fn maps_jellyfin_media_mount_to_host_media_root() {
        let candidates = mapped_media_candidates(
            Path::new("/media/movies/Toy Story (1995)/Toy Story.mkv"),
            Path::new("/home/sean"),
        );

        assert_eq!(
            candidates.first(),
            Some(&PathBuf::from(
                "/home/sean/media-data/media/movies/Toy Story (1995)/Toy Story.mkv"
            ))
        );
    }

    #[test]
    fn maps_separate_movies_mount_to_movies_subdirectory() {
        let candidates = mapped_media_candidates(
            Path::new("/movies/Arrival (2016)/Arrival.mkv"),
            Path::new("/home/sean"),
        );

        assert_eq!(
            candidates.first(),
            Some(&PathBuf::from(
                "/home/sean/media-data/media/movies/Arrival (2016)/Arrival.mkv"
            ))
        );
    }

    #[test]
    fn maps_artwork_types_to_provider_dimensions() {
        assert_eq!(
            asset_spec("Poster"),
            Ok(("vertical 2:3 poster", 1024, 1536))
        );
        assert_eq!(
            asset_spec("Backdrop"),
            Ok(("wide 16:9 backdrop", 1536, 864))
        );
        assert_eq!(
            asset_spec("Banner"),
            Ok(("ultra-wide 12:5 banner", 1536, 640))
        );
        assert!(asset_spec("Unsupported").is_err());
    }

    #[test]
    fn turns_provider_moderation_dumps_into_actionable_errors() {
        let message =
            friendly_codex_failure(r#"image generation failed: { "code": "moderation_blocked" }"#);

        assert!(message.contains("blocked this result during safety review"));
        assert!(message.contains("Signature Element"));
        assert!(!message.contains("moderation_blocked"));
    }
}

#[tauri::command]
pub async fn scan_local_image_provider(provider: String) -> Result<LocalProviderStatus, String> {
    let executable_name = match provider.as_str() {
        "codex" => "codex",
        "higgsfield" => "higgsfield",
        _ => return Err("Only Codex and Higgsfield have local CLI providers.".into()),
    };
    let Some(path) = find_executable(executable_name) else {
        return Ok(LocalProviderStatus {
            provider,
            found: false,
            authenticated: false,
            path: None,
            detail: format!("{executable_name} CLI was not found on this computer."),
        });
    };

    let (authenticated, detail) = if executable_name == "codex" {
        let output = timeout(
            Duration::from_secs(12),
            Command::new(&path).args(["login", "status"]).output(),
        )
        .await
        .map_err(|_| "Codex authentication check timed out.".to_string())?
        .map_err(|error| error.to_string())?;
        let message = String::from_utf8_lossy(if output.status.success() {
            &output.stdout
        } else {
            &output.stderr
        })
        .trim()
        .to_string();
        (
            output.status.success(),
            if message.is_empty() {
                "Codex CLI found.".into()
            } else {
                message
            },
        )
    } else {
        (
            false,
            "Higgsfield CLI found. Account authentication is managed by `higgsfield auth login`."
                .into(),
        )
    };

    Ok(LocalProviderStatus {
        provider,
        found: true,
        authenticated,
        path: Some(path.to_string_lossy().into_owned()),
        detail,
    })
}

#[tauri::command]
pub fn provider_key_status(provider: String) -> Result<ProviderKeyStatus, String> {
    let configured = match keyring_entry(&provider)?.get_password() {
        Ok(value) => !value.trim().is_empty(),
        Err(KeyringError::NoEntry) => false,
        Err(error) => return Err(error.to_string()),
    };
    Ok(ProviderKeyStatus {
        provider,
        configured,
    })
}

#[tauri::command]
pub fn save_provider_key(provider: String, key: String) -> Result<ProviderKeyStatus, String> {
    let key = key.trim();
    if key.len() < 12 {
        return Err("Enter a complete API key before saving.".into());
    }
    keyring_entry(&provider)?
        .set_password(key)
        .map_err(|error| error.to_string())?;
    Ok(ProviderKeyStatus {
        provider,
        configured: true,
    })
}

#[tauri::command]
pub fn remove_provider_key(provider: String) -> Result<ProviderKeyStatus, String> {
    match keyring_entry(&provider)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(ProviderKeyStatus {
            provider,
            configured: false,
        }),
        Err(error) => Err(error.to_string()),
    }
}

async fn download_reference_assets(
    reference_urls: &[String],
) -> Result<Vec<ReferenceAsset>, String> {
    if reference_urls.is_empty() {
        return Ok(Vec::new());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| error.to_string())?;
    let mut assets = Vec::new();
    for url in reference_urls.iter().take(4) {
        let Ok(response) = client.get(url).send().await else {
            continue;
        };
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length as usize > MAX_REFERENCE_BYTES)
        {
            continue;
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("image/jpeg")
            .split(';')
            .next()
            .unwrap_or("image/jpeg")
            .to_ascii_lowercase();
        let (content_type, extension) = match content_type.as_str() {
            "image/png" => ("image/png", "png"),
            "image/webp" => ("image/webp", "webp"),
            "image/jpeg" | "image/jpg" => ("image/jpeg", "jpg"),
            _ => continue,
        };
        let Ok(bytes) = response.bytes().await else {
            continue;
        };
        if bytes.is_empty() || bytes.len() > MAX_REFERENCE_BYTES {
            continue;
        }
        assets.push(ReferenceAsset {
            bytes: bytes.to_vec(),
            content_type: content_type.into(),
            extension,
        });
    }
    if assets.is_empty() {
        return Err("Matinee could not load the selected title’s Jellyfin backdrop images. Check that the artwork is available from this computer.".into());
    }
    Ok(assets)
}

async fn generate_with_fal(
    app: &AppHandle,
    prompt: &str,
    references: &[ReferenceAsset],
    asset_type: &str,
) -> Result<GeneratedImage, String> {
    let (_, width, height) = asset_spec(asset_type)?;
    let key = get_provider_key("fal")?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| error.to_string())?;
    let (model, input) = if references.is_empty() {
        (
            FAL_MODEL,
            serde_json::json!({
                "prompt": prompt,
                "image_size": { "width": width, "height": height },
                "num_images": 1,
                "enable_safety_checker": true,
                "output_format": "png"
            }),
        )
    } else {
        let image_urls = references
            .iter()
            .map(|reference| {
                format!(
                    "data:{};base64,{}",
                    reference.content_type,
                    BASE64_STANDARD.encode(&reference.bytes)
                )
            })
            .collect::<Vec<_>>();
        (
            FAL_REFERENCE_MODEL,
            serde_json::json!({
                "prompt": prompt,
                "image_urls": image_urls,
                "image_size": { "width": width, "height": height },
                "guidance_scale": 3.5,
                "num_inference_steps": 32,
                "num_images": 1,
                "enable_prompt_expansion": true,
                "enable_safety_checker": true,
                "output_format": "png"
            }),
        )
    };
    let response = client
        .post(format!("https://fal.run/{model}"))
        .header("Authorization", format!("Key {key}"))
        .json(&input)
        .send()
        .await
        .map_err(|error| format!("fal.ai request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("fal.ai response could not be read: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "fal.ai returned {status}: {}",
            body.chars().take(280).collect::<String>()
        ));
    }
    let result: FalResponse = serde_json::from_str(&body)
        .map_err(|error| format!("fal.ai returned an unexpected response: {error}"))?;
    let image = result
        .images
        .first()
        .ok_or_else(|| "fal.ai completed without returning an image.".to_string())?;
    let image_response = reqwest::get(&image.url)
        .await
        .map_err(|error| format!("The fal.ai image could not be downloaded: {error}"))?;
    if !image_response.status().is_success() {
        return Err(format!(
            "The fal.ai image download returned {}.",
            image_response.status()
        ));
    }
    let bytes = image_response
        .bytes()
        .await
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("The generated image is larger than Matinee's 32 MB safety limit.".into());
    }
    let path = generated_directory(app)?.join(format!("fal-{}.png", timestamp_millis()));
    fs::write(&path, &bytes).map_err(|error| error.to_string())?;
    let content_type = image.content_type.as_deref().unwrap_or("image/png");
    Ok(GeneratedImage {
        provider: "fal".into(),
        local_path: path.to_string_lossy().into_owned(),
        data_url: format!(
            "data:{content_type};base64,{}",
            BASE64_STANDARD.encode(bytes)
        ),
    })
}

async fn generate_with_codex(
    app: &AppHandle,
    prompt: &str,
    references: &[ReferenceAsset],
    asset_type: &str,
) -> Result<GeneratedImage, String> {
    let (asset_description, _, _) = asset_spec(asset_type)?;
    let codex = find_executable("codex")
        .ok_or_else(|| "Codex CLI was not found. Scan for it in Settings first.".to_string())?;
    let job_directory = generated_directory(app)?.join(format!("codex-{}", timestamp_millis()));
    fs::create_dir_all(&job_directory).map_err(|error| error.to_string())?;
    let mut reference_names = Vec::new();
    for (index, reference) in references.iter().enumerate() {
        let name = format!("movie-reference-{}.{}", index + 1, reference.extension);
        fs::write(job_directory.join(&name), &reference.bytes)
            .map_err(|error| error.to_string())?;
        reference_names.push(name);
    }
    let reference_instruction = if reference_names.is_empty() {
        String::new()
    } else {
        format!(
            "Use these local Jellyfin backdrop stills as visual source material: {}. Preserve their title-specific characters, wardrobe, props, environments, and production design, but transform them into an original Matinee poster composition. Do not merely filter, trace, or reproduce a source frame.\n\n",
            reference_names.join(", ")
        )
    };
    let instruction = format!(
        "Use $imagegen to create one original {asset_description} from the prompt below. Make exactly one built-in image-generation tool call. If that call fails or is rejected, stop and return the error without retrying, rewriting the prompt, or constructing a fallback image locally. {reference_instruction}Save the successful final image as a PNG inside the current working directory. Do not inspect unrelated project or personal-context files. Do not modify or overwrite the reference images and do not modify any other files.\n\n{prompt}"
    );
    let stdout_path = job_directory.join("codex-stdout.log");
    let stderr_path = job_directory.join("codex-stderr.log");
    let stdout = File::create(&stdout_path).map_err(|error| error.to_string())?;
    let stderr = File::create(&stderr_path).map_err(|error| error.to_string())?;
    eprintln!(
        "[poster-generation] starting Codex job at {} with {} reference image(s)",
        job_directory.display(),
        reference_names.len()
    );
    let mut child = Command::new(codex)
        .args([
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "--sandbox",
            "workspace-write",
            "--color",
            "never",
            "-c",
            "model_reasoning_effort=\"low\"",
            "-C",
        ])
        .arg(&job_directory)
        .arg(instruction)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| error.to_string())?;
    let status = match timeout(Duration::from_secs(720), child.wait()).await {
        Ok(result) => result.map_err(|error| error.to_string())?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let diagnostic = log_excerpt(&stderr_path, 600);
            eprintln!(
                "[poster-generation] Codex job timed out at {}",
                job_directory.display()
            );
            return Err(if diagnostic.is_empty() {
                "Codex image generation timed out after 12 minutes. The request was stopped cleanly; retry it or choose fal.ai in Settings.".into()
            } else {
                format!(
                    "Codex image generation timed out after 12 minutes. Last diagnostic: {diagnostic}"
                )
            });
        }
    };
    if !status.success() {
        let stderr = log_excerpt(&stderr_path, 600);
        let stdout = log_excerpt(&stdout_path, 600);
        let message = if stderr.is_empty() { stdout } else { stderr };
        return Err(if message.is_empty() {
            format!("Codex image generation failed: process exited with {status}")
        } else {
            friendly_codex_failure(&message)
        });
    }
    let path = image_file_in(&job_directory)?;
    eprintln!(
        "[poster-generation] Codex job completed with {}",
        path.display()
    );
    let content_type = match path.extension().and_then(|extension| extension.to_str()) {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    Ok(GeneratedImage {
        provider: "codex".into(),
        local_path: path.to_string_lossy().into_owned(),
        data_url: image_data_url(&path, content_type)?,
    })
}

#[tauri::command]
pub async fn generate_poster_image(
    app: AppHandle,
    provider: String,
    prompt: String,
    reference_urls: Vec<String>,
    asset_type: String,
) -> Result<GeneratedImage, String> {
    if prompt.trim().len() < 20 {
        return Err("The poster prompt needs a little more visual direction.".into());
    }
    if provider == "higgsfield" {
        return Err("Higgsfield does not currently publish an API-key image endpoint. Use its CLI/MCP account integration until developer endpoint documentation is available.".into());
    }
    let references = download_reference_assets(&reference_urls).await?;
    match provider.as_str() {
        "codex" => generate_with_codex(&app, prompt.trim(), &references, &asset_type).await,
        "fal" => generate_with_fal(&app, prompt.trim(), &references, &asset_type).await,
        _ => Err("Choose a supported image-generation provider.".into()),
    }
}
