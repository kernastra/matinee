use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use tauri::{AppHandle, Manager};

const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
const MAX_PATH_CHARACTERS: usize = 4096;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkStorageSettings {
    library_root: String,
    export_root: String,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("artwork-storage.json"))
}

fn default_settings(app: &AppHandle) -> Result<ArtworkStorageSettings, String> {
    let library_root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("custom-posters");
    let export_root = app
        .path()
        .picture_dir()
        .map_err(|_| "Your Pictures folder could not be located.".to_string())?
        .join("Matinee");
    fs::create_dir_all(&library_root).map_err(|error| error.to_string())?;
    fs::create_dir_all(&export_root).map_err(|error| error.to_string())?;
    Ok(ArtworkStorageSettings {
        library_root: library_root
            .canonicalize()
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .into_owned(),
        export_root: export_root
            .canonicalize()
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .into_owned(),
    })
}

fn canonical_storage_root(value: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    if value.chars().count() > MAX_PATH_CHARACTERS {
        return Err("An artwork storage path is too long.".into());
    }
    let path = PathBuf::from(value.trim());
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err("Artwork storage folders must use absolute paths without . or ...".into());
    }
    if !path.exists() {
        let parent = path
            .parent()
            .ok_or_else(|| "Choose a folder below the filesystem root.".to_string())?;
        let canonical_parent = parent.canonicalize().map_err(|_| {
            "The parent of that artwork folder does not exist or cannot be accessed.".to_string()
        })?;
        if canonical_parent.parent().is_none() {
            return Err("Create or choose a dedicated artwork folder instead of writing directly in the filesystem or home root.".into());
        }
        fs::create_dir(&path)
            .map_err(|error| format!("Matinee could not create that artwork folder: {error}"))?;
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| "That artwork folder does not exist or cannot be accessed.".to_string())?;
    if !canonical.is_dir()
        || canonical.parent().is_none()
        || home.is_some_and(|home| canonical == home)
    {
        return Err(
            "Choose a dedicated artwork folder instead of the filesystem or home root.".into(),
        );
    }
    Ok(canonical)
}

fn normalize_settings(
    app: &AppHandle,
    settings: ArtworkStorageSettings,
) -> Result<ArtworkStorageSettings, String> {
    let home = app.path().home_dir().ok();
    let library_root = canonical_storage_root(&settings.library_root, home.as_deref())?;
    let export_root = canonical_storage_root(&settings.export_root, home.as_deref())?;
    let generated_root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("generated-posters");
    if library_root == export_root || library_root == generated_root {
        return Err("Use separate folders for the artwork library, exports, and Matinee's temporary generation workspace.".into());
    }
    Ok(ArtworkStorageSettings {
        library_root: library_root.to_string_lossy().into_owned(),
        export_root: export_root.to_string_lossy().into_owned(),
    })
}

fn load_settings(app: &AppHandle) -> Result<ArtworkStorageSettings, String> {
    let path = settings_path(app)?;
    if !path.is_file() {
        return default_settings(app);
    }
    if fs::metadata(&path)
        .map_err(|error| error.to_string())?
        .len()
        > MAX_SETTINGS_BYTES
    {
        return Err("Matinee's artwork-storage settings file is unexpectedly large.".into());
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Matinee could not read artwork-storage settings: {error}"))?;
    let settings = serde_json::from_str(&contents)
        .map_err(|error| format!("Matinee's artwork-storage settings are malformed: {error}"))?;
    normalize_settings(app, settings)
}

pub fn artwork_library_directory(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(PathBuf::from(load_settings(app)?.library_root))
}

pub fn artwork_export_directory(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(PathBuf::from(load_settings(app)?.export_root))
}

#[tauri::command]
pub fn get_artwork_storage_settings(app: AppHandle) -> Result<ArtworkStorageSettings, String> {
    load_settings(&app)
}

#[tauri::command]
pub fn save_artwork_storage_settings(
    app: AppHandle,
    settings: ArtworkStorageSettings,
) -> Result<ArtworkStorageSettings, String> {
    let settings = normalize_settings(&app, settings)?;
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let contents = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(path, contents)
        .map_err(|error| format!("Matinee could not save artwork-storage settings: {error}"))?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::canonical_storage_root;
    use std::path::Path;

    #[test]
    fn rejects_relative_and_broad_storage_roots() {
        assert!(canonical_storage_root("artwork", None).is_err());
        assert!(canonical_storage_root("/", None).is_err());
        assert!(canonical_storage_root("/tmp/../private", None).is_err());
        assert!(canonical_storage_root("/tmp", Some(Path::new("/tmp"))).is_err());
    }
}
