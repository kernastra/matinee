//! Atelier Gallery — the framework's living catalog (Storybook / SwiftUI
//! Previews equivalent). Every reusable component gets a story here.

mod gallery;
mod stories;
mod story;

use atelier_app::{AppInfo, AtelierApp, FRAMEWORK_NAME, WindowSpec, open_window};
use atelier_ui::prelude::*;

use crate::gallery::Gallery;

/// Themes offered by the preview switcher, in order. The first is the
/// default. Product themes are opt-in through cargo features so the
/// gallery itself stays product-neutral.
fn preview_themes() -> Vec<Theme> {
    #[allow(unused_mut)]
    let mut themes = vec![Theme::neutral_dark(), Theme::neutral_light()];
    #[cfg(feature = "matinee-theme")]
    themes.push(matinee_ui::matinee_theme());
    themes
}

fn main() {
    let themes = preview_themes();
    let initial_story = std::env::args().nth(1);
    AtelierApp::new(AppInfo {
        name: "Atelier Gallery",
        app_id: "dev.sean.atelier.gallery",
    })
    .theme(themes[0].clone())
    .run(move |cx| {
        open_window(
            cx,
            WindowSpec {
                title: format!("{FRAMEWORK_NAME} Gallery").into(),
                size: (1180.0, 800.0),
                min_size: (760.0, 520.0),
            },
            move |window, cx| cx.new(|cx| Gallery::new(themes, initial_story, window, cx)),
        )
        .expect("failed to open gallery window");
    });
}
