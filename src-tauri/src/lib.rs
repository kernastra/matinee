mod image_generation;
mod media_calendar;
mod services;
mod window;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            services::install(app.handle()).map_err(|error| -> Box<dyn std::error::Error> {
                std::io::Error::other(error).into()
            })?;
            let show = MenuItem::with_id(app, "show", "Show Matinee", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Matinee", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            let mut tray = TrayIconBuilder::with_id("matinee")
                .tooltip("Matinee")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                });

            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }

            tray.build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            image_generation::assign_generated_poster,
            image_generation::export_generated_image,
            image_generation::export_poster_to_media_folder,
            image_generation::generate_poster_image,
            image_generation::list_custom_posters,
            image_generation::load_movie_manifest,
            image_generation::provider_key_status,
            image_generation::remove_provider_key,
            image_generation::save_provider_key,
            image_generation::scan_local_image_provider,
            media_calendar::clear_integration_calendar_cache,
            media_calendar::fetch_integration_calendar,
            media_calendar::fetch_upcoming_releases,
            media_calendar::integration_key_status,
            media_calendar::remove_integration_key,
            media_calendar::test_and_save_integration,
            window::close_window,
            window::minimize_window,
            window::toggle_maximize_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Matinee");
}
