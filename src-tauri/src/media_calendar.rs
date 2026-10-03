//! Tauri adapter for Radarr and Sonarr.
//!
//! Normalization, the credential payload, and the five-minute cache live in
//! `matinee-integrations`. These commands keep the shipping invoke names.

use matinee_integrations::{
    IntegrationConnection, IntegrationKeyStatus, IntegrationProvider, UpcomingQuery,
    UpcomingRelease, UpcomingResult,
};
use matinee_secrets::Secret;
use tauri::State;

use crate::services::CalendarService;

fn parse_provider(value: &str) -> Result<IntegrationProvider, String> {
    IntegrationProvider::parse(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn integration_key_status(
    services: State<'_, CalendarService>,
    provider: String,
) -> Result<IntegrationKeyStatus, String> {
    services
        .key_status(parse_provider(&provider)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn test_and_save_integration(
    services: State<'_, CalendarService>,
    provider: String,
    server_url: String,
    api_key: Option<String>,
) -> Result<IntegrationConnection, String> {
    services
        .test_and_save(
            parse_provider(&provider)?,
            &server_url,
            api_key.map(Secret::new),
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remove_integration_key(
    services: State<'_, CalendarService>,
    provider: String,
) -> Result<IntegrationKeyStatus, String> {
    services
        .remove(parse_provider(&provider)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn fetch_integration_calendar(
    services: State<'_, CalendarService>,
    provider: String,
    start: String,
    end: String,
) -> Result<Vec<UpcomingRelease>, String> {
    services
        .fetch_calendar(parse_provider(&provider)?, &start, &end)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn fetch_upcoming_releases(
    services: State<'_, CalendarService>,
    radarr_url: String,
    sonarr_url: String,
    start: String,
    end: String,
) -> Result<UpcomingResult, String> {
    Ok(services
        .upcoming(UpcomingQuery::new(radarr_url, sonarr_url, start, end))
        .await)
}

#[tauri::command]
pub fn clear_integration_calendar_cache(services: State<'_, CalendarService>) {
    services.invalidate_cache();
}
