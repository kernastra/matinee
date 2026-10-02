use atelier_ui::prelude::*;

use super::{caption, example};

pub fn render(_window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme().clone();
    let grid = div()
        .flex()
        .flex_wrap()
        .gap(Space::S3.px())
        .children(IconName::ALL.iter().map(|icon| {
            v_stack(Space::S2)
                .w(px(96.0))
                .py(Space::S3.px())
                .items_center()
                .corner_radius(&theme, Radius::Medium)
                .bg(theme.colors.surface.canvas)
                .child(Icon::new(*icon).size(IconSize::Large))
                .child(caption(icon.file_name()))
        }));
    let sizes = h_stack(Space::S6).children(
        [IconSize::Small, IconSize::Medium, IconSize::Large]
            .into_iter()
            .map(|size| {
                h_stack(Space::S2)
                    .child(Icon::new(IconName::Heart).size(size))
                    .child(caption(format!("{}px", size.value())))
            }),
    );
    v_stack(Space::S8)
        .child(example("Set", "Hand-drawn 24px stroke glyphs.", grid))
        .child(example(
            "Sizes",
            "Icon sizes pair with control sizes.",
            sizes,
        ))
        .into_any_element()
}
