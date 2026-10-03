//! Filesystem roots the Tauri adapter fills in, and the native app will too.
//!
//! Media writes are limited to three home directories. Container paths reported
//! by Jellyfin are mapped onto those directories before the file is opened.
//! Canonicalization is required, so a symlink that leaves a root is rejected.

use std::path::{Path, PathBuf};

use crate::error::StudioError;

#[derive(Clone, Debug)]
pub struct StudioPaths {
    home: PathBuf,
    app_data: PathBuf,
    pictures: Option<PathBuf>,
}

impl StudioPaths {
    pub fn new(home: PathBuf, app_data: PathBuf, pictures: Option<PathBuf>) -> Self {
        Self {
            home,
            app_data,
            pictures,
        }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn app_data(&self) -> &Path {
        &self.app_data
    }

    pub fn pictures(&self) -> Option<&Path> {
        self.pictures.as_deref()
    }

    pub fn allowed_media_roots(&self) -> Vec<PathBuf> {
        [
            self.home.join("media-data/media"),
            self.home.join("media"),
            self.home.join("Videos"),
        ]
        .into_iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect()
    }

    pub fn app_file(&self, name: &str) -> PathBuf {
        self.app_data.join(name)
    }
}

pub fn mapped_media_candidates(media_path: &Path, home: &Path) -> Vec<PathBuf> {
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

pub fn resolve_media_file(paths: &StudioPaths, media_path: &str) -> Result<PathBuf, StudioError> {
    let reported = PathBuf::from(media_path);
    let roots = paths.allowed_media_roots();
    if let Some(media_file) = canonical_media_file(&reported, &roots) {
        return Ok(media_file);
    }
    if let Some(candidate) = mapped_media_candidates(&reported, paths.home())
        .into_iter()
        .find_map(|candidate| canonical_media_file(&candidate, &roots))
    {
        return Ok(candidate);
    }
    Err(StudioError::UnsafeMediaPath {
        message: format!(
            "Jellyfin reports this movie at {media_path}, but Matinee could not resolve it inside a trusted local media folder. Matinee supports ~/media-data/media, ~/media, and ~/Videos."
        ),
    })
}

fn canonical_media_file(candidate: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    let canonical = candidate.canonicalize().ok()?;
    (canonical.is_file() && roots.iter().any(|root| canonical.starts_with(root)))
        .then_some(canonical)
}

pub fn ensure_dir(path: &Path) -> Result<(), StudioError> {
    std::fs::create_dir_all(path).map_err(|error| StudioError::filesystem(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn maps_container_mounts_onto_the_home_media_root() {
        let home = Path::new("/home/sean");
        let movies = mapped_media_candidates(Path::new("/movies/Arrival (2016)/Arrival.mkv"), home);
        assert_eq!(
            movies.first(),
            Some(&home.join("media-data/media/movies/Arrival (2016)/Arrival.mkv"))
        );
        let media = mapped_media_candidates(
            Path::new("/media/movies/Toy Story (1995)/Toy Story.mkv"),
            home,
        );
        assert_eq!(
            media.first(),
            Some(&home.join("media-data/media/movies/Toy Story (1995)/Toy Story.mkv"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_traversal_symlink_escape_and_non_files() {
        let root = std::env::temp_dir().join(format!("matinee-media-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let media = home.join("media");
        let outside = root.join("outside");
        fs::create_dir_all(media.join("movies")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(media.join("movies/Film.mkv"), b"video").unwrap();
        fs::write(outside.join("secret.mkv"), b"nope").unwrap();
        std::os::unix::fs::symlink(outside.join("secret.mkv"), media.join("escape.mkv")).unwrap();
        fs::create_dir(media.join("not-a-file")).unwrap();

        let paths = StudioPaths::new(home.clone(), root.join("data"), None);
        let resolved =
            resolve_media_file(&paths, media.join("movies/Film.mkv").to_str().unwrap()).unwrap();
        assert!(resolved.is_file());
        assert!(
            resolve_media_file(
                &paths,
                media.join("../outside/secret.mkv").to_str().unwrap()
            )
            .is_err()
        );
        assert!(resolve_media_file(&paths, media.join("escape.mkv").to_str().unwrap()).is_err());
        assert!(resolve_media_file(&paths, media.join("not-a-file").to_str().unwrap()).is_err());
        let mapped = home.join("media-data/media/movies/Film.mkv");
        fs::create_dir_all(mapped.parent().unwrap()).unwrap();
        fs::write(&mapped, b"video").unwrap();
        let container = resolve_media_file(&paths, "/movies/Film.mkv").unwrap();
        assert_eq!(container, mapped.canonicalize().unwrap());
        let _ = fs::remove_dir_all(&root);
    }
}
