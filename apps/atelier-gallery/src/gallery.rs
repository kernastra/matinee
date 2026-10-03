use atelier_app::{
    Command, FRAMEWORK_NAME, Platform, command::OpenSettings, set_motion_preference, set_theme,
};
use atelier_ui::prelude::*;
use atelier_ui::{Inspection, clear_inspection, current_inspection};

use crate::story::{STORIES, Section, Story, find};

const SIDEBAR_WIDTH: f32 = 232.0;
const TOOLBAR_HEIGHT: f32 = 52.0;

pub struct Gallery {
    focus_handle: FocusHandle,
    nav_focus: Vec<FocusHandle>,
    themes: Vec<Theme>,
    theme_index: usize,
    selected: &'static Story,
    show_preview_controls: bool,
    focus_testing: bool,
}

impl Gallery {
    pub fn new(
        themes: Vec<Theme>,
        initial_story: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        let selected = initial_story
            .as_deref()
            .and_then(find)
            .unwrap_or(&STORIES[0]);
        let nav_focus = STORIES
            .iter()
            .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
            .collect();
        Self {
            focus_handle,
            nav_focus,
            themes,
            theme_index: 0,
            selected,
            show_preview_controls: true,
            focus_testing: false,
        }
    }

    fn select_theme(&mut self, index: usize, cx: &mut Context<Self>) {
        self.theme_index = index;
        set_theme(cx, self.themes[index].clone());
    }

    fn toggle_reduced_motion(&mut self, cx: &mut Context<Self>) {
        let next = match cx.ui_preferences().motion() {
            MotionPreference::Full => MotionPreference::Reduced,
            MotionPreference::Reduced => MotionPreference::Full,
        };
        set_motion_preference(cx, next);
    }

    fn toggle_focus_testing(&mut self, cx: &mut Context<Self>) {
        self.focus_testing = !self.focus_testing;
        cx.notify();
    }

    fn render_sidebar(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let platform = Platform::current();
        let settings = Command::OpenSettings
            .shortcut(platform)
            .map(|s| s.label(platform))
            .unwrap_or_default();

        let mut nav = v_stack(Space::S4).flex_1();
        for section in Section::ALL {
            let mut group = v_stack(Space::Half).child(
                div().px(Space::S2.px()).pb(Space::S1.px()).child(
                    Text::new(section.title())
                        .role(TextRole::Caption)
                        .tone(TextTone::Muted),
                ),
            );
            for (index, story) in STORIES.iter().enumerate() {
                if story.section == section {
                    group = group.child(self.nav_item(index, theme, window, cx));
                }
            }
            nav = nav.child(group);
        }

        v_stack(Space::S6)
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .p(Space::S3.px())
            .bg(theme.colors.surface.panel)
            .border_r_1()
            .border_color(theme.colors.border.subtle)
            .child(
                v_stack(Space::S0)
                    .px(Space::S2.px())
                    .pt(Space::S2.px())
                    .child(Text::new(FRAMEWORK_NAME).role(TextRole::Heading))
                    .child(
                        Text::new("Component Gallery")
                            .role(TextRole::Metadata)
                            .tone(TextTone::Muted),
                    ),
            )
            .child(
                div()
                    .id("gallery-nav")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(nav),
            )
            .child(
                div().px(Space::S2.px()).child(
                    Text::new(format!("{} · Preview options {settings}", platform.name()))
                        .role(TextRole::Caption)
                        .tone(TextTone::Muted),
                ),
            )
    }

    fn nav_item(
        &self,
        index: usize,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let story = &STORIES[index];
        let focus = &self.nav_focus[index];
        let selected = std::ptr::eq(story, self.selected);
        let colors = &theme.colors;
        let hover = colors.control.subtle_hover;
        div()
            .id(story.id)
            .track_focus(focus)
            .relative()
            .px(Space::S2.px())
            .py(Space::S1.px())
            .corner_radius(theme, Radius::Medium)
            .cursor_pointer()
            .when(selected, |this| this.bg(colors.control.subtle_pressed))
            .when(!selected, |this| this.hover(move |s| s.bg(hover)))
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                note_pointer_interaction(cx);
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = story;
                cx.notify();
            }))
            .child(
                Text::new(story.title)
                    .role(TextRole::Label)
                    .tone(if selected {
                        TextTone::Primary
                    } else {
                        TextTone::Secondary
                    }),
            )
            .when(focus_visible(focus.is_focused(window), cx), |this| {
                this.child(FocusRing::new(theme.radius.get(Radius::Medium), 0.0))
            })
    }

    fn render_toolbar(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let reduced = cx.ui_preferences().motion() == MotionPreference::Reduced;
        let mut themes = h_stack(Space::S1).child(
            div().pr(Space::S1.px()).child(
                Text::new("Theme")
                    .role(TextRole::Caption)
                    .tone(TextTone::Muted),
            ),
        );
        for (index, preview) in self.themes.iter().enumerate() {
            let active = index == self.theme_index;
            themes = themes.child(
                Button::new(("theme", index), preview.name)
                    .size(ButtonSize::Small)
                    .variant(if active {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Subtle
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.select_theme(index, cx))),
            );
        }

        let system = match cx.ui_preferences().system_motion {
            Some(MotionPreference::Reduced) => "reduced",
            Some(MotionPreference::Full) => "full",
            None => "unavailable",
        };
        let scale = format!("Scale {:.2}×", window.scale_factor());

        h_stack(Space::S4)
            .flex_none()
            .min_h(px(TOOLBAR_HEIGHT))
            .px(Space::S6.px())
            .py(Space::S2.px())
            .flex_wrap()
            .justify_between()
            .items_center()
            .border_b_1()
            .border_color(theme.colors.border.subtle)
            .child(Text::new(self.selected.title).role(TextRole::Subheading))
            .when(self.show_preview_controls, |this| {
                this.child(
                    h_stack(Space::S3)
                        .flex_wrap()
                        .child(themes)
                        .child(
                            Button::new("reduced-motion", "Reduced motion")
                                .size(ButtonSize::Small)
                                .variant(if reduced {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Subtle
                                })
                                .icon(if reduced {
                                    IconName::Check
                                } else {
                                    IconName::Sliders
                                })
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.toggle_reduced_motion(cx)),
                                ),
                        )
                        .child(
                            Button::new("focus-testing", "Focus testing")
                                .size(ButtonSize::Small)
                                .variant(if self.focus_testing {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Subtle
                                })
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.toggle_focus_testing(cx)),
                                ),
                        )
                        .child(
                            Text::new(scale)
                                .role(TextRole::Caption)
                                .tone(TextTone::Muted),
                        )
                        .child(
                            Text::new(format!("System motion: {system}"))
                                .role(TextRole::Caption)
                                .tone(TextTone::Muted),
                        ),
                )
            })
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let story = self.selected;
        clear_inspection();
        let content = (story.render)(window, cx);
        let inspection = current_inspection();
        let modality = match input_modality(cx) {
            InputModality::Keyboard => "keyboard",
            InputModality::Pointer => "pointer",
        };
        let reduced = cx.ui_preferences().motion() == MotionPreference::Reduced;

        h_stack(Space::S0)
            .id("gallery")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &OpenSettings, _, cx| {
                this.show_preview_controls = !this.show_preview_controls;
                cx.notify();
            }))
            .size_full()
            .items_start()
            .bg(theme.colors.surface.canvas)
            .font_family(theme.typography.families.interface)
            .text_color(theme.colors.text.primary)
            .child(self.render_sidebar(&theme, window, cx))
            .child(
                v_stack(Space::S0)
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .child(self.render_toolbar(&theme, window, cx))
                    .child(
                        div()
                            .id(SharedString::from(format!("scroll-{}", story.id)))
                            .flex_1()
                            .overflow_y_scroll()
                            .child(
                                v_stack(Space::S8)
                                    .p(Space::S8.px())
                                    .max_w(px(1040.0))
                                    .child(
                                        v_stack(Space::S2)
                                            .child(Text::new(story.title).role(TextRole::Title))
                                            .child(
                                                Text::new(story.summary).tone(TextTone::Secondary),
                                            ),
                                    )
                                    .child(content)
                                    .when(self.focus_testing, |this| this.child(focus_note(&theme)))
                                    .child(inspector(
                                        &theme,
                                        inspection.as_ref(),
                                        modality,
                                        reduced,
                                    )),
                            ),
                    ),
            )
    }
}

fn focus_note(theme: &Theme) -> impl IntoElement {
    Surface::new(SurfaceLevel::Panel)
        .padding(Space::S4)
        .child(
            v_stack(Space::S2)
                .child(Text::new("Focus testing").role(TextRole::Subheading))
                .child(
                    Text::new(
                        "Tab moves keyboard focus and shows the focus ring. A pointer press records pointer modality, so the ring hides. Text fields still focus on click: they show a caret and a 1px border, not the ring. Scale above is the window's live scale factor. GPUI 0.2.2 does not let an app change it, so the Gallery does not simulate one.",
                    )
                    .tone(TextTone::Secondary),
                )
                .child(
                    Text::new(format!("Active theme: {}", theme.name))
                        .role(TextRole::Caption)
                        .tone(TextTone::Muted),
                ),
        )
}

fn inspector(
    theme: &Theme,
    inspection: Option<&Inspection>,
    modality: &str,
    reduced: bool,
) -> impl IntoElement {
    let (name, focused, value) = match inspection {
        Some(inspection) => (
            inspection.name,
            inspection.focused.to_string(),
            inspection.value.clone(),
        ),
        None => ("—", "—".to_string(), "—".to_string()),
    };
    Surface::new(SurfaceLevel::Elevated)
        .padding(Space::S4)
        .child(
            v_stack(Space::S2)
                .child(Text::new("Inspector").role(TextRole::Label))
                .child(meta_line(theme, "Control", name))
                .child(meta_line(theme, "Focused", &focused))
                .child(meta_line(theme, "Value", &value))
                .child(meta_line(theme, "Input", modality))
                .child(meta_line(
                    theme,
                    "Reduced motion",
                    if reduced { "true" } else { "false" },
                )),
        )
}

fn meta_line(theme: &Theme, label: &str, value: &str) -> impl IntoElement {
    h_stack(Space::S3)
        .child(
            div().w(px(140.0)).child(
                Text::new(label.to_string())
                    .role(TextRole::Caption)
                    .color(theme.colors.text.muted),
            ),
        )
        .child(Text::new(value.to_string()).role(TextRole::Metadata))
}
