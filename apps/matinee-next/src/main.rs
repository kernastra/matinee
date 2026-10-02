//! Matinee Next — Phase 0 shell.
//!
//! Proves that an application crate can boot on the framework with the
//! Matinee theme without touching GPUI directly. It intentionally contains
//! no product features; the Tauri app remains the reference implementation.

use atelier_app::{AppInfo, AtelierApp, WindowSpec, open_window};
use atelier_ui::prelude::*;
use matinee_ui::matinee_theme;

struct Shell;

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_stack(Space::S3)
            .size_full()
            .items_center()
            .justify_center()
            .bg(theme.colors.surface.canvas)
            .child(Text::new("Matinee").role(TextRole::Display))
            .child(
                Text::new("Native foundation — Phase 0 shell. Features have not been migrated.")
                    .role(TextRole::Metadata)
                    .tone(TextTone::Muted),
            )
    }
}

fn main() {
    AtelierApp::new(AppInfo {
        name: "Matinee Next",
        app_id: "dev.sean.matinee.next",
    })
    .theme(matinee_theme())
    .run(|cx| {
        if let Err(error) = matinee_ui::load_bundled_fonts(cx) {
            eprintln!("failed to load Matinee fonts: {error}");
        }
        open_window(
            cx,
            WindowSpec {
                title: "Matinee Next".into(),
                size: (1200.0, 760.0),
                min_size: (960.0, 620.0),
            },
            |_, cx| cx.new(|_| Shell),
        )
        .expect("failed to open window");
    });
}
