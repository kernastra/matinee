use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};
use tauri::{AppHandle, Manager};

const MAX_MEDIA_PATH_MAPPINGS: usize = 24;
const MAX_TRUSTED_MEDIA_ROOTS: usize = 24;
const MAX_MEDIA_PATH_SETTINGS_BYTES: u64 = 128 * 1024;
const MAX_PATH_CHARACTERS: usize = 4096;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPathMapping {
    jellyfin_prefix: String,
    local_root: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPathSettings {
    mappings: Vec<MediaPathMapping>,
    trusted_roots: Vec<String>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("media-paths.json"))
}

fn normalized_jellyfin_prefix(value: &str) -> Result<PathBuf, String> {
    if value.chars().count() > MAX_PATH_CHARACTERS {
        return Err("A Jellyfin path prefix is too long.".into());
    }
    let path = PathBuf::from(value.trim());
    if !path.is_absolute()
        || path.parent().is_none()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(
            "Each Jellyfin path prefix must be an absolute folder path such as /media or /movies."
                .into(),
        );
    }
    Ok(path)
}

fn canonical_local_root(value: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    if value.chars().count() > MAX_PATH_CHARACTERS {
        return Err("A local media root path is too long.".into());
    }
    let path = PathBuf::from(value.trim());
    if !path.is_absolute() {
        return Err("Each local media root must be an absolute folder path.".into());
    }
    let canonical = path.canonicalize().map_err(|_| {
        format!(
            "The local media folder {} does not exist or cannot be accessed.",
            path.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "The local media root {} is not a folder.",
            canonical.display()
        ));
    }
    if canonical.parent().is_none() || home.is_some_and(|home| canonical == home) {
        return Err(
            "Choose a media folder instead of the filesystem or home-directory root.".into(),
        );
    }
    Ok(canonical)
}

fn normalize_settings(
    settings: MediaPathSettings,
    home: Option<&Path>,
) -> Result<MediaPathSettings, String> {
    if settings.mappings.len() > MAX_MEDIA_PATH_MAPPINGS {
        return Err(format!(
            "Matinee supports up to {MAX_MEDIA_PATH_MAPPINGS} media path mappings."
        ));
    }
    if settings.trusted_roots.len() > MAX_TRUSTED_MEDIA_ROOTS {
        return Err(format!(
            "Matinee supports up to {MAX_TRUSTED_MEDIA_ROOTS} trusted media roots."
        ));
    }

    let mut mappings = Vec::new();
    let mut mapping_keys = HashSet::new();
    let mut trusted_roots = Vec::new();
    let mut trusted_keys = HashSet::new();

    for mapping in settings.mappings {
        let jellyfin_prefix = normalized_jellyfin_prefix(&mapping.jellyfin_prefix)?;
        let local_root = canonical_local_root(&mapping.local_root, home)?;
        let jellyfin_prefix = jellyfin_prefix.to_string_lossy().into_owned();
        let local_root = local_root.to_string_lossy().into_owned();
        if mapping_keys.insert((jellyfin_prefix.clone(), local_root.clone())) {
            mappings.push(MediaPathMapping {
                jellyfin_prefix,
                local_root: local_root.clone(),
            });
        }
    }

    for root in settings.trusted_roots {
        let root = canonical_local_root(&root, home)?
            .to_string_lossy()
            .into_owned();
        if trusted_keys.insert(root.clone()) {
            trusted_roots.push(root);
        }
    }

    Ok(MediaPathSettings {
        mappings,
        trusted_roots,
    })
}

fn default_settings(app: &AppHandle) -> MediaPathSettings {
    let Ok(home) = app.path().home_dir() else {
        return MediaPathSettings::default();
    };
    let mut settings = MediaPathSettings::default();

    for base in [home.join("media-data/media"), home.join("media")] {
        if !base.is_dir() {
            continue;
        }
        settings.mappings.push(MediaPathMapping {
            jellyfin_prefix: "/media".into(),
            local_root: base.to_string_lossy().into_owned(),
        });
        for (prefix, subdirectory) in [("/movies", "movies"), ("/tv", "tv"), ("/shows", "shows")] {
            let local_root = base.join(subdirectory);
            if local_root.is_dir() {
                settings.mappings.push(MediaPathMapping {
                    jellyfin_prefix: prefix.into(),
                    local_root: local_root.to_string_lossy().into_owned(),
                });
            }
        }
    }

    let videos = home.join("Videos");
    if videos.is_dir() {
        settings
            .trusted_roots
            .push(videos.to_string_lossy().into_owned());
    }

    normalize_settings(settings, Some(&home)).unwrap_or_default()
}

fn load_settings(app: &AppHandle) -> Result<MediaPathSettings, String> {
    let path = settings_path(app)?;
    if !path.is_file() {
        return Ok(default_settings(app));
    }
    if fs::metadata(&path)
        .map_err(|error| format!("Matinee could not inspect media-path settings: {error}"))?
        .len()
        > MAX_MEDIA_PATH_SETTINGS_BYTES
    {
        return Err("Matinee's media-path settings file is unexpectedly large.".into());
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Matinee could not read media-path settings: {error}"))?;
    let settings: MediaPathSettings = serde_json::from_str(&contents)
        .map_err(|error| format!("Matinee's media-path settings are malformed: {error}"))?;
    let home = app.path().home_dir().ok();
    normalize_settings(settings, home.as_deref())
}

fn mapped_media_candidates(media_path: &Path, mappings: &[MediaPathMapping]) -> Vec<PathBuf> {
    let mut mappings = mappings.iter().collect::<Vec<_>>();
    mappings.sort_by(|left, right| {
        Path::new(&right.jellyfin_prefix)
            .components()
            .count()
            .cmp(&Path::new(&left.jellyfin_prefix).components().count())
    });
    mappings
        .into_iter()
        .filter_map(|mapping| {
            media_path
                .strip_prefix(Path::new(&mapping.jellyfin_prefix))
                .ok()
                .map(|relative| Path::new(&mapping.local_root).join(relative))
        })
        .collect()
}

fn canonical_media_file(candidate: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    let canonical = candidate.canonicalize().ok()?;
    (canonical.is_file() && roots.iter().any(|root| canonical.starts_with(root)))
        .then_some(canonical)
}

pub fn resolve_media_file(app: &AppHandle, media_path: &str) -> Result<PathBuf, String> {
    let settings = load_settings(app)?;
    let roots = settings
        .mappings
        .iter()
        .map(|mapping| PathBuf::from(&mapping.local_root))
        .chain(settings.trusted_roots.iter().map(PathBuf::from))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return Err(
            "No trusted local media folders are configured. Add one in Settings → Media storage."
                .into(),
        );
    }

    let reported_path = PathBuf::from(media_path);
    if let Some(media_file) = canonical_media_file(&reported_path, &roots) {
        return Ok(media_file);
    }
    if let Some(candidate) = mapped_media_candidates(&reported_path, &settings.mappings)
        .into_iter()
        .find_map(|candidate| canonical_media_file(&candidate, &roots))
    {
        return Ok(candidate);
    }

    Err(format!(
        "Jellyfin reports this title at {media_path}, but no configured mapping resolves it inside a trusted local media folder. Check Settings → Media storage."
    ))
}

#[tauri::command]
pub fn get_media_path_settings(app: AppHandle) -> Result<MediaPathSettings, String> {
    load_settings(&app)
}

#[tauri::command]
pub fn save_media_path_settings(
    app: AppHandle,
    settings: MediaPathSettings,
) -> Result<MediaPathSettings, String> {
    let home = app.path().home_dir().ok();
    let settings = normalize_settings(settings, home.as_deref())?;
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let contents = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(path, contents)
        .map_err(|error| format!("Matinee could not save media-path settings: {error}"))?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::{mapped_media_candidates, normalized_jellyfin_prefix, MediaPathMapping};
    use std::path::{Path, PathBuf};

    #[test]
    fn maps_jellyfin_prefixes_to_configured_local_roots() {
        let mappings = vec![MediaPathMapping {
            jellyfin_prefix: "/media".into(),
            local_root: "/mnt/library".into(),
        }];
        assert_eq!(
            mapped_media_candidates(Path::new("/media/movies/Arrival/Arrival.mkv"), &mappings),
            vec![PathBuf::from("/mnt/library/movies/Arrival/Arrival.mkv")]
        );
    }

    #[test]
    fn prefers_the_most_specific_matching_prefix() {
        let mappings = vec![
            MediaPathMapping {
                jellyfin_prefix: "/media".into(),
                local_root: "/mnt/library".into(),
            },
            MediaPathMapping {
                jellyfin_prefix: "/media/movies".into(),
                local_root: "/mnt/films".into(),
            },
        ];
        assert_eq!(
            mapped_media_candidates(Path::new("/media/movies/Arrival/Arrival.mkv"), &mappings),
            vec![
                PathBuf::from("/mnt/films/Arrival/Arrival.mkv"),
                PathBuf::from("/mnt/library/movies/Arrival/Arrival.mkv"),
            ]
        );
    }

    #[test]
    fn rejects_relative_or_filesystem_root_prefixes() {
        assert!(normalized_jellyfin_prefix("media").is_err());
        assert!(normalized_jellyfin_prefix("/").is_err());
        assert!(normalized_jellyfin_prefix("/media/../private").is_err());
    }
}
