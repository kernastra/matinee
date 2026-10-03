use atelier_ui::{prelude::*, tokens::TextRole};

use super::{caption, example};

pub fn render(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let tones = v_stack(Space::S2).children(
        [
            (TextTone::Primary, "Primary — main content"),
            (TextTone::Secondary, "Secondary — supporting content"),
            (TextTone::Muted, "Muted — de-emphasized metadata"),
            (TextTone::Disabled, "Disabled — unavailable"),
        ]
        .into_iter()
        .map(|(tone, label)| Text::new(label).tone(tone)),
    );
    let truncation = div().w(px(260.0)).child(
        Text::new("A long line of text that does not fit and is clipped with an ellipsis")
            .truncate(),
    );
    let roles = v_stack(Space::S1).children(
        [TextRole::Heading, TextRole::Body, TextRole::Metadata]
            .into_iter()
            .map(|role| Text::new(format!("Role: {}", role.name())).role(role)),
    );
    v_stack(Space::S8)
        .child(example("Tones", "Semantic foreground colors.", tones))
        .child(example(
            "Roles",
            "See Typography for the full scale.",
            roles,
        ))
        .child(example(
            "Truncation",
            "Single-line truncation for constrained layouts.",
            v_stack(Space::S2)
                .child(truncation)
                .child(caption("Width: 260px")),
        ))
        .into_any_element()
}
