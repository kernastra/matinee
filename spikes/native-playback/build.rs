fn main() {
    println!("cargo:rerun-if-env-changed=MPV_LIB_DIR");
    if std::env::var_os("CARGO_FEATURE_MPV").is_none() {
        return;
    }
    // Windows/macOS builds usually point at a prebuilt libmpv (e.g. shinchiro's
    // mpv-dev zip or a Homebrew prefix) instead of relying on pkg-config.
    if let Some(dir) = std::env::var_os("MPV_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", dir.to_string_lossy());
        println!("cargo:rustc-link-lib=mpv");
        return;
    }
    pkg_config::Config::new()
        .atleast_version("2.0")
        .probe("mpv")
        .expect("libmpv (client API >= 2.0) not found; install libmpv-dev or set MPV_LIB_DIR");
}
