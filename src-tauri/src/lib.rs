mod window;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            window::close_window,
            window::minimize_window,
            window::toggle_maximize_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Matinee");
}
