//! Codex CLI discovery and the fixed `exec` invocation.
//!
//! The saved path is not a secret. It lives in app data. Every generation
//! canonicalizes it again and refuses a path that no longer matches the file
//! that was scanned.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::StudioError;
use crate::model::CODEX_FILESYSTEM_PERMISSIONS;
use crate::paths::{StudioPaths, ensure_dir};

pub const CODEX_LOG_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub fn executable_file_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub fn executable_candidates(
    name: &str,
    path_var: Option<&OsStr>,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    let executable = executable_file_name(name);
    let mut candidates = Vec::new();
    if let Some(path) = path_var {
        candidates.extend(std::env::split_paths(path).map(|directory| directory.join(&executable)));
    }
    if let Some(home) = home {
        candidates.extend([
            home.join(".local/bin").join(&executable),
            home.join(".cargo/bin").join(&executable),
            home.join(".bun/bin").join(&executable),
            home.join(".npm-global/bin").join(&executable),
        ]);
    }
    candidates.extend([
        PathBuf::from("/usr/local/bin").join(&executable),
        PathBuf::from("/usr/bin").join(&executable),
    ]);
    candidates
}

pub fn find_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH");
    let home = std::env::var_os("HOME").map(PathBuf::from);
    executable_candidates(name, path.as_deref(), home.as_deref())
        .into_iter()
        .find(|candidate| candidate.is_file())
}

pub fn codex_path_file(paths: &StudioPaths) -> PathBuf {
    paths.app_file("codex-cli-path")
}

pub fn save_codex_path(paths: &StudioPaths, path: &Path) -> Result<PathBuf, StudioError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| StudioError::ExecutableChanged {
            message: "The detected Codex executable could not be verified.".into(),
        })?;
    if !canonical.is_file() {
        return Err(StudioError::ExecutableChanged {
            message: "The detected Codex path is not a file.".into(),
        });
    }
    ensure_dir(paths.app_data())?;
    fs::write(
        codex_path_file(paths),
        canonical.to_string_lossy().as_bytes(),
    )
    .map_err(|error| StudioError::filesystem(error.to_string()))?;
    Ok(canonical)
}

pub fn saved_codex_path(paths: &StudioPaths) -> Result<PathBuf, StudioError> {
    let stored =
        fs::read_to_string(codex_path_file(paths)).map_err(|_| StudioError::ExecutableMissing {
            message: "Scan for the local Codex CLI in Settings before generating a poster.".into(),
        })?;
    let path = PathBuf::from(stored.trim());
    let canonical = path
        .canonicalize()
        .map_err(|_| StudioError::ExecutableMissing {
            message: "The saved Codex CLI path no longer exists. Scan again in Settings.".into(),
        })?;
    if !canonical.is_file() || canonical != path {
        return Err(StudioError::ExecutableChanged {
            message: "The saved Codex CLI path changed. Scan again in Settings before generating."
                .into(),
        });
    }
    Ok(canonical)
}

pub fn codex_arguments(job_directory: &Path, instruction: &str) -> Vec<String> {
    let args = vec![
        "exec".into(),
        "--ephemeral".into(),
        "--skip-git-repo-check".into(),
        "--ignore-user-config".into(),
        "--ignore-rules".into(),
        "--strict-config".into(),
        "--color".into(),
        "never".into(),
        "-c".into(),
        "model_reasoning_effort=\"low\"".into(),
        "-c".into(),
        "approval_policy=\"never\"".into(),
        "-c".into(),
        "web_search=\"disabled\"".into(),
        "-c".into(),
        "default_permissions=\"matinee-poster\"".into(),
        "-c".into(),
        "permissions.matinee-poster.description=\"Isolated Matinee poster generation\"".into(),
        "-c".into(),
        CODEX_FILESYSTEM_PERMISSIONS.into(),
        "-c".into(),
        "permissions.matinee-poster.network.enabled=false".into(),
        "-c".into(),
        "allow_login_shell=false".into(),
        "-c".into(),
        "shell_environment_policy.inherit=\"none\"".into(),
        "-C".into(),
        job_directory.to_string_lossy().into_owned(),
        instruction.to_string(),
    ];
    args
}

pub fn cleanup_expired_codex_jobs(directory: &Path, now: SystemTime, retention: Duration) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for path in entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("codex-"))
        })
    {
        let expired = fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= retention);
        if expired {
            let _ = fs::remove_dir_all(path);
        }
    }
}

pub fn friendly_codex_failure(message: &str) -> String {
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
        "Codex image generation failed. Matinee kept a local diagnostic for seven days; try again or choose fal.ai in Settings.".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_is_a_direct_exec_with_the_filesystem_policy() {
        let args = codex_arguments(Path::new("/tmp/job"), "draw");
        assert_eq!(args[0], "exec");
        assert!(args.iter().any(|arg| arg == CODEX_FILESYSTEM_PERMISSIONS));
        assert!(
            !args
                .iter()
                .any(|arg| matches!(arg.as_str(), "sh" | "cmd" | "powershell" | "-c" if false))
        );
        assert!(!CODEX_FILESYSTEM_PERMISSIONS.contains(":workspace_roots\"=\"write"));
        let message = friendly_codex_failure(r#"{"code":"moderation_blocked"}"#);
        assert!(message.contains("safety review"));
        assert!(!message.contains("moderation_blocked"));
    }

    #[test]
    fn stale_and_non_canonical_saved_paths_are_rejected() {
        let root = std::env::temp_dir().join(format!("matinee-codex-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        let paths = StudioPaths::new(home, data, None);
        let binary = root.join("codex-bin");
        fs::write(&binary, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&binary).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&binary, permissions).unwrap();
        }
        let saved = save_codex_path(&paths, &binary).unwrap();
        assert_eq!(saved, binary.canonicalize().unwrap());
        fs::remove_file(&binary).unwrap();
        assert!(saved_codex_path(&paths).is_err());
        fs::write(codex_path_file(&paths), b"../not-the-binary").unwrap();
        assert!(saved_codex_path(&paths).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn candidates_include_path_and_user_bin() {
        let home = Path::new("/home/sean");
        let candidates = executable_candidates("codex", Some(OsStr::new("/opt/bin")), Some(home));
        assert!(candidates.iter().any(|path| path == &PathBuf::from("/opt/bin/codex") || path.ends_with("codex.exe")));
        assert!(
            candidates
                .iter()
                .any(|path| path == &home.join(".local/bin/codex") || path.ends_with("codex.exe"))
        );
    }
}
