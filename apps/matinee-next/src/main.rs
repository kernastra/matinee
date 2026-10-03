//! Matinee Next shell.
//!
//! Adopts Atelier's native window, theme, and a generic toolbar and sidebar.
//! It has no product features: no library, no server, and no playback.

use atelier_app::{
    AppInfo, AtelierApp, ChromeIntent, Platform, WindowSpec, install_window_input, open_window,
    resolve_chrome, titlebar_leading, titlebar_spacer,
};
use atelier_ui::prelude::*;
use matinee_ui::matinee_theme;

struct Shell {
    focus: FocusHandle,
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        install_window_input(window);
        let theme = cx.theme().clone();
        let chrome = resolve_chrome(Platform::current(), ChromeIntent::PlatformDefault);
        let row_height = if chrome.band_height > 0.0 {
            chrome.band_height
        } else {
            40.0
        };
        let sidebar = Sidebar::new(
            "shell-nav",
            vec![SidebarSection::new(vec![SidebarItem::new("Overview")]).label("Window")],
        )
        .selected(Some(0));

        v_stack(Space::S0)
            .id("matinee-shell")
            .track_focus(&self.focus)
            .size_full()
            .bg(theme.colors.surface.canvas)
            .font_family(theme.typography.families.interface)
            .text_color(theme.colors.text.primary)
            .child(
                h_stack(Space::S3)
                    .w_full()
                    .h(px(row_height))
                    .flex_none()
                    .items_center()
                    .px(Space::S4.px())
                    .border_b_1()
                    .border_color(theme.colors.border.subtle)
                    .when(chrome.leading_inset > 0.0, |row| {
                        row.child(titlebar_leading(chrome.leading_inset, chrome.content_drag))
                    })
                    .child(Text::new("Matinee").role(TextRole::Heading))
                    .child(titlebar_spacer(chrome.content_drag)),
            )
            .child(
                div().flex_1().min_h(px(0.0)).child(
                    SplitView::new(
                        "shell-split",
                        sidebar,
                        v_stack(Space::S3)
                            .p(Space::S8.px())
                            .child(Text::new("Matinee").role(TextRole::Display))
                            .child(
                                Text::new(
                                    "Native window. Toolbar and sidebar only. Features have not been migrated.",
                                )
                                .role(TextRole::Metadata)
                                .tone(TextTone::Muted),
                            ),
                    )
                    .leading_width(200.0)
                    .limits(160.0, 280.0),
                ),
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
            WindowSpec::new("Matinee Next", (1200.0, 760.0))
                .min_size((960.0, 620.0))
                .restoration_key("shell"),
            |window, cx| {
                cx.new(|cx| {
                    let shell = Shell {
                        focus: cx.focus_handle(),
                    };
                    window.focus(&shell.focus);
                    shell
                })
            },
        )
        .expect("failed to open window");
    });
}
