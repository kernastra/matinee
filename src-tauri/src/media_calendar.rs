use keyring::{Entry, Error as KeyringError};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

const KEYRING_SERVICE: &str = "dev.sean.matinee.media-integrations";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationKeyStatus {
    provider: String,
    configured: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationConnection {
    provider: String,
    configured: bool,
    server_url: String,
    version: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredIntegration {
    server_url: String,
    api_key: String,
}

fn supported_provider(provider: &str) -> Result<&str, String> {
    match provider {
        "radarr" | "sonarr" => Ok(provider),
        _ => Err("Choose Radarr or Sonarr.".into()),
    }
}

fn keyring_entry(provider: &str) -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, supported_provider(provider)?).map_err(|error| error.to_string())
}

fn stored_integration(provider: &str) -> Result<StoredIntegration, String> {
    let stored = keyring_entry(provider)?
        .get_password()
        .map_err(|error| match error {
            KeyringError::NoEntry => format!("No {provider} integration is configured."),
            other => other.to_string(),
        })?;
    serde_json::from_str(&stored).map_err(|_| {
        format!(
            "Reconnect {provider} once to securely bind its saved API key to the server address."
        )
    })
}

fn normalize_server_url(value: &str) -> Result<String, String> {
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() {
        return Err("Enter the server address first.".into());
    }
    let url = Url::parse(value)
        .map_err(|_| "Enter a complete address beginning with http:// or https://.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Integration addresses must begin with http:// or https://.".into());
    }
    if url.host_str().is_none() {
        return Err("Enter a valid Radarr or Sonarr server address.".into());
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Integration addresses cannot contain credentials, a query, or a fragment.".into(),
        );
    }
    Ok(value.to_string())
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| error.to_string())
}

async fn response_json(response: reqwest::Response, provider: &str) -> Result<Value, String> {
    let status = response.status();
    if status.is_success() {
        return response
            .json::<Value>()
            .await
            .map_err(|error| format!("{provider} returned an unreadable response: {error}"));
    }
    let detail = response.text().await.unwrap_or_default();
    let detail = detail.trim();
    let suffix = if detail.is_empty() {
        String::new()
    } else {
        format!(" {}", detail.chars().take(180).collect::<String>())
    };
    Err(format!("{provider} returned HTTP {status}.{suffix}"))
}

#[tauri::command]
pub fn integration_key_status(provider: String) -> Result<IntegrationKeyStatus, String> {
    let provider = supported_provider(&provider)?.to_string();
    let configured = match keyring_entry(&provider)?.get_password() {
        Ok(value) => serde_json::from_str::<StoredIntegration>(&value).is_ok_and(|stored| {
            !stored.api_key.trim().is_empty() && !stored.server_url.trim().is_empty()
        }),
        Err(KeyringError::NoEntry) => false,
        Err(error) => return Err(error.to_string()),
    };
    Ok(IntegrationKeyStatus {
        provider,
        configured,
    })
}

#[tauri::command]
pub async fn test_and_save_integration(
    provider: String,
    server_url: String,
    api_key: Option<String>,
) -> Result<IntegrationConnection, String> {
    let provider = supported_provider(&provider)?.to_string();
    let server_url = normalize_server_url(&server_url)?;
    let supplied_key = api_key.unwrap_or_default();
    let supplied_key = supplied_key.trim();
    let key = if supplied_key.is_empty() {
        let stored = stored_integration(&provider)?;
        if stored.server_url != server_url {
            return Err(format!(
                "The saved {provider} key is bound to a different server. Enter the API key again to change addresses."
            ));
        }
        stored.api_key
    } else {
        supplied_key.to_string()
    };
    if key.len() < 8 {
        return Err("Enter a complete API key before testing the connection.".into());
    }

    let response = client()?
        .get(format!("{server_url}/api/v3/system/status"))
        .header("X-Api-Key", &key)
        .send()
        .await
        .map_err(|error| format!("Matinee could not reach {provider}: {error}"))?;
    let body = response_json(response, &provider).await?;
    let app_name = body
        .get("appName")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !app_name.contains(&provider) {
        return Err(format!(
            "That address responded, but it does not appear to be {provider}."
        ));
    }

    let stored = StoredIntegration {
        server_url: server_url.clone(),
        api_key: key,
    };
    keyring_entry(&provider)?
        .set_password(&serde_json::to_string(&stored).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;

    Ok(IntegrationConnection {
        provider,
        configured: true,
        server_url,
        version: body
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[tauri::command]
pub fn remove_integration_key(provider: String) -> Result<IntegrationKeyStatus, String> {
    let provider = supported_provider(&provider)?.to_string();
    match keyring_entry(&provider)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(IntegrationKeyStatus {
            provider,
            configured: false,
        }),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub async fn fetch_integration_calendar(
    provider: String,
    start: String,
    end: String,
) -> Result<Value, String> {
    let provider = supported_provider(&provider)?.to_string();
    let stored = stored_integration(&provider)?;
    let server_url = normalize_server_url(&stored.server_url)?;
    let mut request = client()?
        .get(format!("{server_url}/api/v3/calendar"))
        .header("X-Api-Key", stored.api_key)
        .query(&[
            ("start", start.as_str()),
            ("end", end.as_str()),
            ("unmonitored", "false"),
        ]);
    if provider == "sonarr" {
        request = request.query(&[("includeSeries", "true"), ("includeEpisodeImages", "true")]);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("Matinee could not reach {provider}: {error}"))?;
    response_json(response, &provider).await
}

#[cfg(test)]
mod tests {
    use super::{normalize_server_url, supported_provider};

    #[test]
    fn accepts_supported_integration_providers() {
        assert_eq!(supported_provider("radarr"), Ok("radarr"));
        assert_eq!(supported_provider("sonarr"), Ok("sonarr"));
        assert!(supported_provider("jellyseerr").is_err());
    }

    #[test]
    fn normalizes_http_server_addresses() {
        assert_eq!(
            normalize_server_url(" http://radarr.local:7878/ "),
            Ok("http://radarr.local:7878".into())
        );
        assert!(normalize_server_url("radarr.local:7878").is_err());
        assert!(normalize_server_url("file:///tmp/radarr").is_err());
        assert!(normalize_server_url("http://user:secret@radarr.local:7878").is_err());
        assert!(normalize_server_url("http://radarr.local:7878?key=secret").is_err());
    }
}
