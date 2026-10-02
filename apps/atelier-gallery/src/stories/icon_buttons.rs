use atelier_ui::prelude::*;

use super::{caption, example, metadata};

#[derive(Default)]
struct Demo {
    playing: bool,
    favorite: bool,
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("icon-button-demo", cx, |_, _| Demo::default());
    let (playing, favorite) = {
        let d = demo.read(cx);
        (d.playing, d.favorite)
    };

    let play = demo.clone();
    let fav = demo.clone();
    let toggles = h_stack(Space::S3)
        .child(
            IconButton::new(
                "play-toggle",
                if playing {
                    IconName::Pause
                } else {
                    IconName::Play
                },
                if playing { "Pause" } else { "Play" },
            )
            .variant(ButtonVariant::Primary)
            .size(ButtonSize::Large)
            .on_click(move |_, _, cx| {
                play.update(cx, |d, cx| {
                    d.playing = !d.playing;
                    cx.notify();
                })
            }),
        )
        .child(
            IconButton::new(
                "favorite-toggle",
                IconName::Heart,
                if favorite {
                    "Remove from favorites"
                } else {
                    "Add to favorites"
                },
            )
            .variant(if favorite {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Subtle
            })
            .size(ButtonSize::Large)
            .on_click(move |_, _, cx| {
                fav.update(cx, |d, cx| {
                    d.favorite = !d.favorite;
                    cx.notify();
                })
            }),
        )
        .child(metadata(format!(
            "{} · {}",
            if playing { "playing" } else { "paused" },
            if favorite { "favorite" } else { "not favorite" }
        )));

    let icons = [
        (IconName::Search, "Search"),
        (IconName::Plus, "Add"),
        (IconName::More, "More actions"),
        (IconName::Close, "Close"),
    ];
    let variants = v_stack(Space::S4).children(ButtonVariant::ALL.iter().map(|variant| {
        h_stack(Space::S3)
            .child(div().w(px(110.0)).child(metadata(variant.name())))
            .children(icons.iter().enumerate().map(move |(i, (icon, label))| {
                IconButton::new(
                    SharedString::from(format!("ib-{}-{i}", variant.name())),
                    *icon,
                    *label,
                )
                .variant(*variant)
            }))
            .child(
                IconButton::new(
                    SharedString::from(format!("ib-{}-disabled", variant.name())),
                    IconName::Trash,
                    "Delete",
                )
                .variant(*variant)
                .disabled(true),
            )
    }));

    let sizes = h_stack(Space::S3).children(ButtonSize::ALL.iter().map(|size| {
        IconButton::new(
            SharedString::from(format!("ib-size-{}", size.name())),
            IconName::Sliders,
            format!("{} settings", size.name()),
        )
        .variant(ButtonVariant::Secondary)
        .size(*size)
    }));

    v_stack(Space::S8)
        .child(example(
            "Toggles",
            "Icon and label update together; hover to see the label as a tooltip.",
            toggles,
        ))
        .child(example(
            "Variants",
            "Subtle is the default for icon buttons. The last column is disabled.",
            variants,
        ))
        .child(example(
            "Sizes",
            "Square at every control height.",
            v_stack(Space::S3)
                .child(sizes)
                .child(caption("Labels are mandatory in the constructor.")),
        ))
        .into_any_element()
}
