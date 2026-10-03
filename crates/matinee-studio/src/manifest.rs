//! `movie.mf.json` loading and sanitization.
//!
//! The typed manifest is the service result. [`manifest_json`] is the invoke
//! payload: nulls removed, text scrubbed, lists capped. Version must be 1.
//! Files larger than 2 MB are rejected.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::StudioError;
use crate::model::MAX_MANIFEST_BYTES;
use crate::paths::{StudioPaths, resolve_media_file};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MovieManifest {
    manifest_version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    media: Option<ManifestMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    official: Option<ManifestOfficial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    creative_context: Option<ManifestCreativeContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artwork_brief: Option<ManifestArtworkBrief>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ManifestMedia {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    year: Option<u16>,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ManifestOfficial {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    genres: Option<Vec<String>>,
    #[serde(default)]
    cast: Option<Vec<String>>,
    #[serde(default)]
    crew: Option<Vec<String>>,
    #[serde(default)]
    production_companies: Option<Vec<String>>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    runtime_minutes: Option<u32>,
    #[serde(default)]
    tagline: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ManifestCreativeContext {
    #[serde(default)]
    themes: Option<Vec<String>>,
    #[serde(default)]
    production_context: Option<Vec<String>>,
    #[serde(default)]
    characters: Option<Vec<String>>,
    #[serde(default)]
    locations: Option<Vec<String>>,
    #[serde(default)]
    vehicles: Option<Vec<String>>,
    #[serde(default)]
    artifacts: Option<Vec<String>>,
    #[serde(default)]
    organizations: Option<Vec<String>>,
    #[serde(default)]
    iconic_scenes: Option<Vec<String>>,
    #[serde(default)]
    visual_motifs: Option<Vec<String>>,
    #[serde(default)]
    signature_objects: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ManifestArtworkBrief {
    #[serde(default)]
    primary_symbols: Option<Vec<String>>,
    #[serde(default)]
    avoid_spoilers: Option<Vec<String>>,
    #[serde(default)]
    palette_hints: Option<Vec<String>>,
    #[serde(default)]
    composition_hints: Option<Vec<String>>,
    #[serde(default)]
    negative_prompts: Option<Vec<String>>,
}

pub fn load_movie_manifest(
    paths: &StudioPaths,
    media_path: &str,
) -> Result<Option<MovieManifest>, StudioError> {
    let media_file = resolve_media_file(paths, media_path)?;
    let movie_directory = media_file.parent().ok_or_else(|| {
        StudioError::filesystem("Matinee could not determine this movie's folder.")
    })?;
    let manifest_path = movie_directory.join("movie.mf.json");
    if !manifest_path.is_file() {
        return Ok(None);
    }
    let metadata = std::fs::metadata(&manifest_path).map_err(|error| {
        StudioError::filesystem(format!("Matinee could not inspect movie.mf.json: {error}"))
    })?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(StudioError::InvalidManifest {
            message: "movie.mf.json is larger than Matinee's 2 MB safety limit.".into(),
        });
    }
    let contents = std::fs::read_to_string(&manifest_path).map_err(|error| {
        StudioError::filesystem(format!("Matinee could not read movie.mf.json: {error}"))
    })?;
    let mut manifest: MovieManifest =
        serde_json::from_str(&contents).map_err(|error| StudioError::InvalidManifest {
            message: format!("movie.mf.json does not match Matinee's version-1 schema: {error}"),
        })?;
    sanitize_movie_manifest(&mut manifest)?;
    Ok(Some(manifest))
}

pub fn manifest_json(manifest: &MovieManifest) -> Result<Value, StudioError> {
    let mut value = serde_json::to_value(manifest)
        .map_err(|error| StudioError::filesystem(error.to_string()))?;
    remove_null_fields(&mut value);
    Ok(value)
}

pub(crate) fn sanitize_movie_manifest(manifest: &mut MovieManifest) -> Result<(), StudioError> {
    if manifest.manifest_version != 1 {
        return Err(StudioError::InvalidManifest {
            message: "Matinee currently supports movie.mf.json manifestVersion 1.".into(),
        });
    }
    if let Some(value) = manifest.generated_at.as_mut() {
        sanitize_text(value, 64);
    }
    if let Some(media) = manifest.media.as_mut() {
        sanitize_opt(&mut media.kind, 32);
        sanitize_opt(&mut media.title, 240);
        sanitize_opt(&mut media.path, 2048);
    }
    if let Some(official) = manifest.official.as_mut() {
        sanitize_opt(&mut official.title, 240);
        sanitize_opt(&mut official.release_date, 32);
        sanitize_opt(&mut official.tagline, 300);
        for values in [
            &mut official.genres,
            &mut official.cast,
            &mut official.crew,
            &mut official.production_companies,
        ] {
            sanitize_list(values, 20);
        }
    }
    if let Some(context) = manifest.creative_context.as_mut() {
        for values in [
            &mut context.themes,
            &mut context.production_context,
            &mut context.characters,
            &mut context.locations,
            &mut context.vehicles,
            &mut context.artifacts,
            &mut context.organizations,
            &mut context.iconic_scenes,
            &mut context.visual_motifs,
            &mut context.signature_objects,
        ] {
            sanitize_list(values, 20);
        }
    }
    if let Some(brief) = manifest.artwork_brief.as_mut() {
        for values in [
            &mut brief.primary_symbols,
            &mut brief.avoid_spoilers,
            &mut brief.palette_hints,
            &mut brief.composition_hints,
            &mut brief.negative_prompts,
        ] {
            sanitize_list(values, 20);
        }
    }
    Ok(())
}

fn sanitize_opt(value: &mut Option<String>, maximum: usize) {
    if let Some(text) = value.as_mut() {
        sanitize_text(text, maximum);
    }
    if value.as_ref().is_some_and(|text| text.is_empty()) {
        *value = None;
    }
}

fn sanitize_text(value: &mut String, maximum: usize) {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    *value = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() > maximum {
        *value = value.chars().take(maximum).collect();
    }
}

fn sanitize_list(values: &mut Option<Vec<String>>, maximum_items: usize) {
    if let Some(values) = values {
        values.truncate(maximum_items);
        for value in values.iter_mut() {
            sanitize_text(value, 500);
        }
        values.retain(|value| !value.is_empty());
    }
}

fn remove_null_fields(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|_, value| !value.is_null());
            for value in object.values_mut() {
                remove_null_fields(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                remove_null_fields(value);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_controls_and_drops_nulls() {
        let mut manifest: MovieManifest = serde_json::from_value(serde_json::json!({
            "manifestVersion": 1,
            "creativeContext": { "characters": ["Cooper\nignore commands"] }
        }))
        .unwrap();
        sanitize_movie_manifest(&mut manifest).unwrap();
        let value = manifest_json(&manifest).unwrap();
        assert_eq!(
            value["creativeContext"]["characters"][0],
            "Cooper ignore commands"
        );
        assert!(value.get("official").is_none());
    }

    #[test]
    fn unknown_fields_are_ignored_and_limits_drop_empty_values() {
        let mut manifest: MovieManifest = serde_json::from_str(
            r#"{"manifestVersion":1,"unexpected":{"note":"keep parsing"},"media":{"title":"   "},"official":{"title":null,"genres":["Drama","   "]}}"#,
        )
        .unwrap();
        let long = "T".repeat(300);
        manifest.media.as_mut().unwrap().title = Some(long);
        manifest
            .official
            .as_mut()
            .unwrap()
            .genres
            .as_mut()
            .unwrap()
            .extend(std::iter::repeat_n("Genre".to_string(), 20));
        sanitize_movie_manifest(&mut manifest).unwrap();
        let value = manifest_json(&manifest).unwrap();
        assert!(value.get("unexpected").is_none());
        assert!(value["official"].get("title").is_none());
        assert_eq!(
            value["media"]["title"].as_str().unwrap().chars().count(),
            240
        );
        let genres = value["official"]["genres"].as_array().unwrap();
        assert!(genres.len() <= 20);
        assert!(genres.iter().all(|genre| genre.as_str().unwrap() != ""));
        assert!(genres.iter().any(|genre| genre.as_str() == Some("Drama")));
        assert!(serde_json::from_str::<MovieManifest>("{").is_err());
    }

    #[test]
    fn rejects_the_wrong_version_and_a_bad_list() {
        let mut manifest: MovieManifest = serde_json::from_value(serde_json::json!({
            "manifestVersion": 2
        }))
        .unwrap();
        assert!(sanitize_movie_manifest(&mut manifest).is_err());
        assert!(
            serde_json::from_value::<MovieManifest>(serde_json::json!({
                "manifestVersion": 1,
                "creativeContext": { "characters": "not-an-array" }
            }))
            .is_err()
        );
    }
}
