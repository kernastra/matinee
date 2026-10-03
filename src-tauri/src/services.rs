//! Process-wide service objects. Commands translate invoke arguments and call
//! these. Business rules live in the shared crates.

use matinee_integrations::{Integrations, ReqwestTransport};
use matinee_secrets::KeyringStore;
use matinee_studio::{ReqwestStudio, Studio, StudioPaths, TokioProcess};
use tauri::{AppHandle, Manager};

pub type CalendarService = Integrations<KeyringStore, ReqwestTransport>;
pub type PosterService = Studio<KeyringStore, ReqwestStudio, TokioProcess>;

pub fn install(app: &AppHandle) -> Result<(), String> {
    let calendar = CalendarService::new(
        KeyringStore::new(),
        ReqwestTransport::new().map_err(|error| error.to_string())?,
    );
    let posters = PosterService::new(
        KeyringStore::new(),
        ReqwestStudio::new().map_err(|error| error.to_string())?,
        TokioProcess,
    );
    app.manage(calendar);
    app.manage(posters);
    Ok(())
}

pub fn studio_paths(app: &AppHandle) -> Result<StudioPaths, String> {
    let home = app.path().home_dir().map_err(|error| error.to_string())?;
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let pictures = app.path().picture_dir().ok();
    Ok(StudioPaths::new(home, app_data, pictures))
}
