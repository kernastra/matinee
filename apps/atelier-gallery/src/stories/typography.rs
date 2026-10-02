use atelier_ui::{prelude::*, tokens::TextRole};

use super::{caption, example};

pub fn render(_window: &mut Window, cx: &mut App) -> AnyElement {
    let typography = cx.theme().typography.clone();
    let rows = v_stack(Space::S6).children(TextRole::ALL.iter().map(|role| {
        let style = typography.style(*role);
        v_stack(Space::S1)
            .child(caption(format!(
                "{} — {} {}/{} · {}",
                role.name(),
                typography.family(*role),
                style.size,
                style.line_height,
                style.weight.0
            )))
            .child(Text::new("The quick brown fox jumps over the lazy dog").role(*role))
    }));
    v_stack(Space::S8)
        .child(example(
            "Type scale",
            "Families come from the theme; fonts that are not installed fall back to the platform UI font.",
            rows,
        ))
        .into_any_element()
}
