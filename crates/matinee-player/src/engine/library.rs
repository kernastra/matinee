//! Locate a libmpv shared library at runtime.

#![forbid(unsafe_code)]
//!
//! Development machines may use the system library. That build is often GPL
//! and must not be redistributed. Release artifacts are a separately pinned
//! LGPL libmpv and LGPL FFmpeg, described in `docs/architecture/playback.md`.
//! Nothing is downloaded here.

use std::path::PathBuf;

use crate::engine::ffi::{self, Api};
use crate::error::PlayerError;

pub struct LoadedLibrary {
    pub path: PathBuf,
    pub version: (u32, u32),
    pub api: Api,
    _library: libloading::Library,
}

pub fn load() -> Result<LoadedLibrary, PlayerError> {
    if let Some(path) = std::env::var_os("MATINEE_LIBMPV") {
        return load_path(PathBuf::from(path));
    }
    let mut searched = Vec::new();
    for name in default_names() {
        searched.push(name.to_string());
        match open_name(name) {
            Ok(loaded) => return Ok(loaded),
            Err(error @ PlayerError::LibraryIncompatible { .. }) => return Err(error),
            Err(_) => continue,
        }
    }
    Err(PlayerError::LibraryMissing { searched })
}

pub fn load_path(path: PathBuf) -> Result<LoadedLibrary, PlayerError> {
    if !path.is_file() {
        return Err(PlayerError::LibraryMissing {
            searched: vec![path.display().to_string()],
        });
    }
    let name = path.display().to_string();
    let library = ffi::open_library(&path).map_err(|error| PlayerError::LibraryIncompatible {
        path: name.clone(),
        detail: error.to_string(),
    })?;
    finish(library, path)
}

fn open_name(name: &str) -> Result<LoadedLibrary, PlayerError> {
    let library = ffi::open_library(name).map_err(|_| PlayerError::LibraryMissing {
        searched: vec![name.to_string()],
    })?;
    finish(library, PathBuf::from(name))
}

fn finish(library: libloading::Library, path: PathBuf) -> Result<LoadedLibrary, PlayerError> {
    let api = ffi::bind(&library).map_err(|error| match error {
        PlayerError::LibraryIncompatible { detail, .. } => PlayerError::LibraryIncompatible {
            path: path.display().to_string(),
            detail,
        },
        other => other,
    })?;
    let version = ffi::client_version(&api);
    if !accept_client_version(version.0) {
        return Err(PlayerError::LibraryIncompatible {
            path: path.display().to_string(),
            detail: format!("client API {}.{} is not 2.x", version.0, version.1),
        });
    }
    Ok(LoadedLibrary {
        path,
        version,
        api,
        _library: library,
    })
}

pub fn default_names() -> &'static [&'static str] {
    #[cfg(target_os = "linux")]
    {
        &["libmpv.so.2", "libmpv.so"]
    }
    #[cfg(target_os = "macos")]
    {
        &["libmpv.2.dylib", "libmpv.dylib"]
    }
    #[cfg(target_os = "windows")]
    {
        &["libmpv-2.dll", "mpv-2.dll", "libmpv.dll"]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        &["libmpv.so.2", "libmpv.so"]
    }
}

pub fn accept_client_version(major: u32) -> bool {
    major == 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_client_api_2_is_accepted() {
        assert!(accept_client_version(2));
        assert!(!accept_client_version(1));
        assert!(!accept_client_version(3));
    }

    #[test]
    fn missing_path_is_typed() {
        let missing = std::path::Path::new("/tmp/matinee-no-such-libmpv.so");
        assert!(matches!(
            load_path(missing.to_path_buf()),
            Err(PlayerError::LibraryMissing { .. })
        ));
    }
}
