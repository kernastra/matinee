use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

const KEYRING_SERVICE: &str = "dev.sean.matinee.jellyfin";
const MAX_PROFILES: usize = 12;
const MAX_PROFILES_FILE_BYTES: u64 = 128 * 1024;
const MAX_SERVER_URL_CHARACTERS: usize = 2048;
const MAX_TOKEN_CHARACTERS: usize = 16 * 1024;
const MAX_USER_FIELD_CHARACTERS: usize = 512;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JellyfinProfile {
    id: String,
    server_url: String,
    user_id: String,
    user_name: String,
    primary_image_tag: Option<String>,
    last_used_at: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RememberJellyfinProfile {
    server_url: String,
    access_token: String,
    user_id: String,
    user_name: String,
    primary_image_tag: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoredJellyfinSession {
    server_url: String,
    access_token: String,
    user: RestoredJellyfinUser,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct RestoredJellyfinUser {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_image_tag: Option<String>,
}

fn profiles_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("jellyfin-profiles.json"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn stable_profile_id(server_url: &str, user_id: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in server_url
        .as_bytes()
        .iter()
        .chain([0_u8].iter())
        .chain(user_id.as_bytes())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn keyring_entry(profile_id: &str) -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, &format!("profile-{profile_id}")).map_err(|error| error.to_string())
}

fn validate_profile_id(profile_id: &str) -> Result<(), String> {
    if profile_id.len() != 16 || !profile_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("That remembered Jellyfin profile identifier is invalid.".into());
    }
    Ok(())
}

fn validated_text(value: String, label: &str, maximum: usize) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() || value.chars().count() > maximum {
        return Err(format!("The Jellyfin {label} is missing or too long."));
    }
    Ok(value)
}

fn validate_profile(profile: RememberJellyfinProfile) -> Result<RememberJellyfinProfile, String> {
    let server_url = validated_text(
        profile.server_url,
        "server address",
        MAX_SERVER_URL_CHARACTERS,
    )?;
    if !(server_url.starts_with("http://") || server_url.starts_with("https://")) {
        return Err("The Jellyfin server address must use HTTP or HTTPS.".into());
    }
    let access_token = validated_text(profile.access_token, "access token", MAX_TOKEN_CHARACTERS)?;
    let user_id = validated_text(profile.user_id, "user ID", MAX_USER_FIELD_CHARACTERS)?;
    let user_name = validated_text(profile.user_name, "account name", MAX_USER_FIELD_CHARACTERS)?;
    let primary_image_tag = profile
        .primary_image_tag
        .map(|value| validated_text(value, "avatar tag", MAX_USER_FIELD_CHARACTERS))
        .transpose()?;
    Ok(RememberJellyfinProfile {
        server_url,
        access_token,
        user_id,
        user_name,
        primary_image_tag,
    })
}

fn load_profiles(app: &AppHandle) -> Result<Vec<JellyfinProfile>, String> {
    let path = profiles_path(app)?;
    if !path.is_file() {
        return Ok(Vec::new());
    }
    if fs::metadata(&path)
        .map_err(|error| format!("Matinee could not inspect remembered profiles: {error}"))?
        .len()
        > MAX_PROFILES_FILE_BYTES
    {
        return Err("Matinee's remembered-profile file is unexpectedly large.".into());
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Matinee could not read remembered profiles: {error}"))?;
    let mut profiles: Vec<JellyfinProfile> = serde_json::from_str(&contents)
        .map_err(|error| format!("Matinee's remembered profiles are malformed: {error}"))?;
    if profiles.len() > MAX_PROFILES {
        return Err("Matinee's remembered-profile limit was exceeded.".into());
    }
    for profile in &profiles {
        validate_profile_id(&profile.id)?;
        validated_text(
            profile.server_url.clone(),
            "server address",
            MAX_SERVER_URL_CHARACTERS,
        )?;
        validated_text(
            profile.user_id.clone(),
            "user ID",
            MAX_USER_FIELD_CHARACTERS,
        )?;
        validated_text(
            profile.user_name.clone(),
            "account name",
            MAX_USER_FIELD_CHARACTERS,
        )?;
    }
    profiles.sort_by(|left, right| right.last_used_at.cmp(&left.last_used_at));
    Ok(profiles)
}

fn save_profiles(app: &AppHandle, profiles: &[JellyfinProfile]) -> Result<(), String> {
    let path = profiles_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let contents = serde_json::to_string_pretty(profiles).map_err(|error| error.to_string())?;
    fs::write(path, contents)
        .map_err(|error| format!("Matinee could not save remembered profiles: {error}"))
}

#[tauri::command]
pub fn list_jellyfin_profiles(app: AppHandle) -> Result<Vec<JellyfinProfile>, String> {
    load_profiles(&app)
}

#[tauri::command]
pub fn remember_jellyfin_profile(
    app: AppHandle,
    profile: RememberJellyfinProfile,
) -> Result<JellyfinProfile, String> {
    let profile = validate_profile(profile)?;
    let id = stable_profile_id(&profile.server_url, &profile.user_id);
    let mut profiles = load_profiles(&app)?;
    keyring_entry(&id)?
        .set_password(&profile.access_token)
        .map_err(|error| {
            format!("Matinee could not store this Jellyfin session securely: {error}")
        })?;

    let remembered = JellyfinProfile {
        id: id.clone(),
        server_url: profile.server_url,
        user_id: profile.user_id,
        user_name: profile.user_name,
        primary_image_tag: profile.primary_image_tag,
        last_used_at: now(),
    };
    profiles.retain(|candidate| candidate.id != id);
    profiles.insert(0, remembered.clone());
    for evicted in profiles.drain(MAX_PROFILES.min(profiles.len())..) {
        if let Ok(entry) = keyring_entry(&evicted.id) {
            let _ = entry.delete_credential();
        }
    }
    save_profiles(&app, &profiles)?;
    Ok(remembered)
}

#[tauri::command]
pub fn restore_jellyfin_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<RestoredJellyfinSession, String> {
    validate_profile_id(&profile_id)?;
    let mut profiles = load_profiles(&app)?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| "That remembered Jellyfin account no longer exists.".to_string())?;
    let access_token = keyring_entry(&profile.id)?
        .get_password()
        .map_err(|error| match error {
            KeyringError::NoEntry => {
                "This remembered Jellyfin session has expired. Sign in again to reconnect it."
                    .to_string()
            }
            other => format!("Matinee could not unlock this Jellyfin session: {other}"),
        })?;
    if access_token.is_empty() || access_token.chars().count() > MAX_TOKEN_CHARACTERS {
        return Err(
            "This remembered Jellyfin session is invalid. Sign in again to reconnect it.".into(),
        );
    }
    profile.last_used_at = now();
    let restored = RestoredJellyfinSession {
        server_url: profile.server_url.clone(),
        access_token,
        user: RestoredJellyfinUser {
            id: profile.user_id.clone(),
            name: profile.user_name.clone(),
            primary_image_tag: profile.primary_image_tag.clone(),
        },
    };
    save_profiles(&app, &profiles)?;
    Ok(restored)
}

#[tauri::command]
pub fn forget_jellyfin_profile(app: AppHandle, profile_id: String) -> Result<(), String> {
    validate_profile_id(&profile_id)?;
    let mut profiles = load_profiles(&app)?;
    if !profiles.iter().any(|profile| profile.id == profile_id) {
        return Ok(());
    }
    match keyring_entry(&profile_id)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => {}
        Err(error) => return Err(format!("Matinee could not forget this session: {error}")),
    }
    profiles.retain(|profile| profile.id != profile_id);
    save_profiles(&app, &profiles)
}

#[cfg(test)]
mod tests {
    use super::{stable_profile_id, validate_profile, RememberJellyfinProfile};

    fn profile() -> RememberJellyfinProfile {
        RememberJellyfinProfile {
            server_url: "http://jellyfin.local:8096".into(),
            access_token: "token".into(),
            user_id: "user-1".into(),
            user_name: "Sean".into(),
            primary_image_tag: None,
        }
    }

    #[test]
    fn creates_stable_account_ids_without_exposing_account_data() {
        let first = stable_profile_id("http://jellyfin.local:8096", "user-1");
        assert_eq!(
            first,
            stable_profile_id("http://jellyfin.local:8096", "user-1")
        );
        assert_ne!(
            first,
            stable_profile_id("http://jellyfin.local:8096", "user-2")
        );
        assert!(!first.contains("user"));
    }

    #[test]
    fn validates_required_profile_fields_and_protocol() {
        assert!(validate_profile(profile()).is_ok());
        let mut invalid = profile();
        invalid.server_url = "file:///tmp/jellyfin".into();
        assert!(validate_profile(invalid).is_err());
    }
}
