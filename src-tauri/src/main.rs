#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
fn ensure_webkit_dmabuf_workaround() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    ensure_webkit_dmabuf_workaround();

    matinee_lib::run()
}
