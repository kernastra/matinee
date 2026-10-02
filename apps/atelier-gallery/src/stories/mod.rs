pub mod buttons;
pub mod checkbox;
pub mod colors;
pub mod composition;
pub mod desktop;
pub mod icon_buttons;
pub mod icons;
pub mod motion;
pub mod search_field;
pub mod segmented;
pub mod shape;
pub mod slider;
pub mod spacing;
pub mod surfaces;
pub mod switch;
pub mod text;
pub mod text_field;
pub mod typography;

use atelier_ui::{prelude::*, tokens::Color};

/// A titled demo block. Every story is a column of these.
pub fn example(
    title: &'static str,
    note: impl Into<SharedString>,
    content: impl IntoElement,
) -> impl IntoElement {
    v_stack(Space::S3)
        .child(
            v_stack(Space::S1)
                .child(Text::new(title).role(TextRole::Subheading))
                .child(Text::new(note).role(TextRole::Body).tone(TextTone::Muted)),
        )
        .child(
            Surface::new(SurfaceLevel::Panel)
                .padding(Space::S6)
                .child(content),
        )
}

pub fn hex(color: Color) -> String {
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let rgb = format!(
        "#{:02x}{:02x}{:02x}",
        channel(color.r),
        channel(color.g),
        channel(color.b)
    );
    if color.a < 1.0 {
        format!("{rgb} · {:.0}%", color.a * 100.0)
    } else {
        rgb
    }
}

pub fn caption(text: impl Into<SharedString>) -> Text {
    Text::new(text)
        .role(TextRole::Caption)
        .tone(TextTone::Muted)
}

pub fn row(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    h_stack(Space::S4)
        .items_center()
        .child(div().w(px(160.0)).child(caption(label)))
        .child(control)
}

pub fn metadata(text: impl Into<SharedString>) -> Text {
    Text::new(text)
        .role(TextRole::Metadata)
        .tone(TextTone::Secondary)
}
