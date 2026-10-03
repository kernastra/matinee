use atelier_ui::{prelude::*, tokens::TextRole};

use super::example;

pub fn render(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let levels = h_stack(Space::S6).items_start().children(
        [
            (SurfaceLevel::Canvas, "Canvas"),
            (SurfaceLevel::Panel, "Panel"),
            (SurfaceLevel::Elevated, "Elevated"),
        ]
        .into_iter()
        .map(|(level, name)| {
            Surface::new(level).w(px(200.0)).child(
                v_stack(Space::S2)
                    .child(Text::new(name).role(TextRole::Subheading))
                    .child(
                        Text::new("Background, border, and elevation from tokens.")
                            .tone(TextTone::Secondary),
                    )
                    .child(
                        h_stack(Space::S2)
                            .pt(Space::S2.px())
                            .child(
                                Button::new(SharedString::from(format!("surface-{name}")), "Open")
                                    .size(ButtonSize::Small)
                                    .variant(ButtonVariant::Primary),
                            )
                            .child(
                                IconButton::new(
                                    SharedString::from(format!("surface-more-{name}")),
                                    IconName::More,
                                    "More actions",
                                )
                                .size(ButtonSize::Small),
                            ),
                    ),
            )
        }),
    );
    example(
        "Levels",
        "Shown on the panel surface of the example frame.",
        levels,
    )
    .into_any_element()
}
