use atelier_ui::{prelude::*, tokens::ColorRole};

use super::{caption, example, hex, metadata};

const GROUPS: [(&str, &str, &[ColorRole]); 5] = [
    (
        "Text",
        "Foreground roles, from primary content to disabled.",
        &[
            ColorRole::TextPrimary,
            ColorRole::TextSecondary,
            ColorRole::TextMuted,
            ColorRole::TextDisabled,
            ColorRole::TextOnAccent,
            ColorRole::TextOnDestructive,
            ColorRole::TextDanger,
        ],
    ),
    (
        "Surface",
        "Backgrounds by level, plus the modal scrim.",
        &[
            ColorRole::SurfaceCanvas,
            ColorRole::SurfacePanel,
            ColorRole::SurfaceElevated,
            ColorRole::SurfaceOverlay,
        ],
    ),
    (
        "Border",
        "Separators and outlines.",
        &[
            ColorRole::BorderSubtle,
            ColorRole::BorderDefault,
            ColorRole::BorderStrong,
        ],
    ),
    (
        "Control",
        "Fills for interactive controls in each state.",
        &[
            ColorRole::ControlAccent,
            ColorRole::ControlAccentHover,
            ColorRole::ControlAccentPressed,
            ColorRole::ControlNeutral,
            ColorRole::ControlNeutralHover,
            ColorRole::ControlNeutralPressed,
            ColorRole::ControlSubtleHover,
            ColorRole::ControlSubtlePressed,
            ColorRole::ControlDestructive,
            ColorRole::ControlDestructiveHover,
            ColorRole::ControlDestructivePressed,
            ColorRole::ControlDisabled,
        ],
    ),
    (
        "Focus",
        "Keyboard focus indication.",
        &[ColorRole::FocusRing],
    ),
];

pub fn render(_window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme().clone();
    let issues = theme.validate();
    let mut page = v_stack(Space::S8).child(example(
        "Contrast contract",
        "Every theme is validated against WCAG ratios for text and focus roles.",
        if issues.is_empty() {
            h_stack(Space::S2)
                .child(Icon::new(IconName::Check).color(theme.colors.focus.ring))
                .child(Text::new(format!(
                    "{} passes all contrast checks.",
                    theme.name
                )))
                .into_any_element()
        } else {
            v_stack(Space::S1)
                .children(issues.iter().map(|i| metadata(i.to_string())))
                .into_any_element()
        },
    ));

    for (title, note, roles) in GROUPS {
        let swatches = div()
            .flex()
            .flex_wrap()
            .gap(Space::S4.px())
            .children(roles.iter().map(|role| {
                let color = theme.colors.get(*role);
                v_stack(Space::S2)
                    .w(px(148.0))
                    .child(
                        div()
                            .h(px(56.0))
                            .corner_radius(&theme, Radius::Medium)
                            .border_1()
                            .border_color(theme.colors.border.default)
                            .bg(theme.colors.surface.canvas)
                            .child(
                                div()
                                    .size_full()
                                    .corner_radius(&theme, Radius::Medium)
                                    .bg(color),
                            ),
                    )
                    .child(
                        v_stack(Space::S0)
                            .child(metadata(role.token_name()))
                            .child(caption(hex(color))),
                    )
            }));
        page = page.child(example(title, note, swatches));
    }
    page.into_any_element()
}
