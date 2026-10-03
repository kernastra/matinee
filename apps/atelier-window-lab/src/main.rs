//! Window Lab.
//!
//! A dedicated window for inspecting titlebar insets, dragging, minimum
//! size, fullscreen, scale, and the resolved platform strategy. It is not a
//! Gallery story and it has no product content.

use atelier_app::{
    AppInfo, AtelierApp, ChromeIntent, Platform, WindowSpec, on_fullscreen_escape, open_window,
    resolve_chrome, titlebar_leading, titlebar_spacer,
};
use atelier_ui::prelude::*;

const MIN_SIZE: (f32, f32) = (720.0, 480.0);
const TOOLBAR_ROW: f32 = 40.0;

struct Lab {
    focus: FocusHandle,
    section: usize,
    query: SharedString,
    split: f32,
}

impl Lab {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            section: 0,
            query: SharedString::default(),
            split: 200.0,
        }
    }
}

impl Render for Lab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let chrome = resolve_chrome(Platform::current(), ChromeIntent::PlatformDefault);
        let platform = Platform::current();
        let bounds = window.bounds();
        let viewport = window.viewport_size();
        let row_height = if chrome.band_height > 0.0 {
            chrome.band_height
        } else {
            TOOLBAR_ROW
        };
        let fullscreen = window.is_fullscreen();
        let section = self.section;
        let split = self.split;

        let sidebar = Sidebar::new(
            "window-lab-nav",
            vec![
                SidebarSection::new(vec![
                    SidebarItem::new("Window").icon(IconName::Info),
                    SidebarItem::new("Layout").icon(IconName::Sliders),
                ])
                .label("Inspect"),
            ],
        )
        .selected(Some(section))
        .on_select({
            let lab = cx.entity();
            move |index, _, cx| {
                lab.update(cx, |this, cx| {
                    this.section = index;
                    cx.notify();
                });
            }
        });

        let detail = if section == 0 {
            v_stack(Space::S3)
                .child(Text::new("Window").role(TextRole::Title))
                .child(fact(
                    "Platform",
                    platform.name(),
                ))
                .child(fact(
                    "Decorations",
                    "Server-side. Linux and Windows keep the system frame.",
                ))
                .child(fact(
                    "Titlebar band",
                    format!("{:.0} px", chrome.band_height),
                ))
                .child(fact(
                    "Leading inset",
                    format!("{:.0} px", chrome.leading_inset),
                ))
                .child(fact(
                    "Content drag",
                    if chrome.content_drag { "yes" } else { "no" },
                ))
                .child(fact(
                    "Scale",
                    format!("{:.2}×", window.scale_factor()),
                ))
                .child(fact(
                    "Origin",
                    format!(
                        "{:.0}, {:.0}",
                        f32::from(bounds.origin.x),
                        f32::from(bounds.origin.y)
                    ),
                ))
                .child(fact(
                    "Frame",
                    format!(
                        "{:.0} × {:.0}",
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height)
                    ),
                ))
                .child(fact(
                    "Viewport",
                    format!(
                        "{:.0} × {:.0}",
                        f32::from(viewport.width),
                        f32::from(viewport.height)
                    ),
                ))
                .child(fact(
                    "Minimum",
                    format!("{:.0} × {:.0}", MIN_SIZE.0, MIN_SIZE.1),
                ))
                .child(fact(
                    "Maximized",
                    if window.is_maximized() { "yes" } else { "no" },
                ))
                .child(fact(
                    "Full screen",
                    if fullscreen { "yes" } else { "no" },
                ))
                .child(fact(
                    "Restore position",
                    if platform.restores_window_origin() {
                        "saved origin"
                    } else {
                        "centered (Wayland)"
                    },
                ))
                .child(
                    Text::new(
                        "Empty regions of this toolbar drag when the platform draws an in-client titlebar. The search field and buttons do not. Escape leaves full screen when nothing else consumed it.",
                    )
                    .tone(TextTone::Secondary),
                )
        } else {
            v_stack(Space::S3)
                .child(Text::new("Layout").role(TextRole::Title))
                .child(
                    Text::new(
                        "The sidebar sits under the titlebar band. Drag the divider. The window's minimum size keeps both panes on screen.",
                    )
                    .tone(TextTone::Secondary),
                )
                .child(fact("Split", format!("{split:.0} px")))
        };

        v_stack(Space::S0)
            .id("window-lab")
            .track_focus(&self.focus)
            .on_key_down(on_fullscreen_escape)
            .size_full()
            .bg(theme.colors.surface.canvas)
            .font_family(theme.typography.families.interface)
            .text_color(theme.colors.text.primary)
            .child(titlebar(
                &theme,
                row_height,
                chrome.leading_inset,
                chrome.content_drag,
                fullscreen,
                self.query.clone(),
                cx,
            ))
            .child(
                div().flex_1().min_h(px(0.0)).child(
                    SplitView::new("window-lab-split", sidebar, detail.p(Space::S6.px()))
                        .leading_width(split)
                        .limits(160.0, 280.0)
                        .on_resize({
                            let lab = cx.entity();
                            move |width, _, cx| {
                                lab.update(cx, |this, cx| {
                                    this.split = width;
                                    cx.notify();
                                });
                            }
                        }),
                ),
            )
    }
}

fn titlebar(
    theme: &Theme,
    height: f32,
    leading: f32,
    drag: bool,
    fullscreen: bool,
    query: SharedString,
    cx: &mut Context<Lab>,
) -> impl IntoElement {
    h_stack(Space::S2)
        .w_full()
        .h(px(height))
        .flex_none()
        .items_center()
        .px(Space::S3.px())
        .bg(theme.colors.surface.panel)
        .border_b_1()
        .border_color(theme.colors.border.subtle)
        .when(leading > 0.0, |row| {
            row.child(titlebar_leading(leading, drag))
        })
        .child(Text::new("Window Lab").role(TextRole::Heading))
        .child(titlebar_spacer(drag))
        .child(
            div().w(px(180.0)).child(
                SearchField::new("window-lab-search", query)
                    .placeholder("Filter")
                    .on_change({
                        let lab = cx.entity();
                        move |value, _, cx| {
                            lab.update(cx, |this, cx| {
                                this.query = value;
                                cx.notify();
                            });
                        }
                    }),
            ),
        )
        .child(
            Button::new(
                "fullscreen",
                if fullscreen {
                    "Exit full screen"
                } else {
                    "Full screen"
                },
            )
            .size(ButtonSize::Small)
            .on_click(|_, window, _| window.toggle_fullscreen()),
        )
        .child(titlebar_spacer(drag))
}

fn fact(label: &'static str, value: impl Into<SharedString>) -> impl IntoElement {
    h_stack(Space::S3)
        .child(
            div().w(px(140.0)).child(
                Text::new(label)
                    .role(TextRole::Caption)
                    .tone(TextTone::Muted),
            ),
        )
        .child(Text::new(value).role(TextRole::Metadata))
}

fn main() {
    AtelierApp::new(AppInfo {
        name: "Window Lab",
        app_id: "dev.sean.atelier.window-lab",
    })
    .theme(Theme::neutral_dark())
    .run(|cx| {
        open_window(
            cx,
            WindowSpec::new("Window Lab", (960.0, 640.0))
                .min_size(MIN_SIZE)
                .max_size((1600.0, 1000.0))
                .restoration_key("window-lab"),
            |window, cx| {
                cx.new(|cx| {
                    let lab = Lab::new(cx);
                    window.focus(&lab.focus);
                    lab
                })
            },
        )
        .expect("failed to open window lab");
    });
}
