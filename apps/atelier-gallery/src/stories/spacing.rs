use atelier_ui::{prelude::*, tokens::Space};

use super::{caption, example, metadata};

pub fn render(_window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme().clone();
    let rows = v_stack(Space::S3).children(Space::ALL.iter().map(|space| {
        h_stack(Space::S4)
            .child(div().w(px(96.0)).child(metadata(space.token_name())))
            .child(
                div()
                    .w(px(48.0))
                    .child(caption(format!("{}px", space.value()))),
            )
            .child(
                div()
                    .h(px(12.0))
                    .w(space.px())
                    .corner_radius(&theme, Radius::Small)
                    .bg(theme.colors.control.accent),
            )
    }));
    example(
        "Scale",
        "Multiples of a 4px unit (with a 2px half step for hairline gaps).",
        rows,
    )
    .into_any_element()
}
