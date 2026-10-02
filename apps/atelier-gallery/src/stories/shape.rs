use atelier_ui::{
    prelude::*,
    tokens::{Elevation, Radius},
};

use super::{caption, example, metadata};

pub fn render(_window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme().clone();
    let radii = h_stack(Space::S6)
        .items_start()
        .children(Radius::ALL.iter().map(|radius| {
            v_stack(Space::S2)
                .child(
                    div()
                        .size(px(72.0))
                        .corner_radius(&theme, *radius)
                        .bg(theme.colors.control.neutral)
                        .border_1()
                        .border_color(theme.colors.border.strong),
                )
                .child(metadata(radius.token_name()))
                .child(caption(match radius {
                    Radius::Full => "capsule".to_string(),
                    r => format!("{}px", theme.radius.get(*r)),
                }))
        }));

    let elevations =
        h_stack(Space::S8)
            .items_start()
            .p(Space::S4.px())
            .children(Elevation::ALL.iter().map(|level| {
                v_stack(Space::S2)
                    .child(
                        div()
                            .w(px(132.0))
                            .h(px(88.0))
                            .corner_radius(&theme, Radius::Large)
                            .bg(theme.colors.surface.elevated)
                            .elevation(&theme, *level),
                    )
                    .child(metadata(format!("{level:?}").to_lowercase()))
            }));

    v_stack(Space::S8)
        .child(example(
            "Radius",
            "Themes set the character of corners; components choose a role.",
            radii,
        ))
        .child(example(
            "Elevation",
            "Two-layer shadows: a tight contact shadow plus a soft ambient one.",
            elevations,
        ))
        .into_any_element()
}
