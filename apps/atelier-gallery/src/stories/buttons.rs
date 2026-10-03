use std::time::Duration;

use atelier_ui::prelude::*;

use super::{caption, example, metadata};

#[derive(Default)]
struct Demo {
    activations: usize,
    last_source: Option<&'static str>,
    saving: bool,
}

fn source(event: &ClickEvent) -> &'static str {
    match event {
        ClickEvent::Mouse(_) => "pointer",
        ClickEvent::Keyboard(_) => "keyboard",
    }
}

fn sample_label(variant: ButtonVariant) -> &'static str {
    match variant {
        ButtonVariant::Primary => "Continue",
        ButtonVariant::Secondary => "Cancel",
        ButtonVariant::Subtle => "More Info",
        ButtonVariant::Destructive => "Delete",
    }
}

fn sample_icon(variant: ButtonVariant) -> IconName {
    match variant {
        ButtonVariant::Primary => IconName::Play,
        ButtonVariant::Secondary => IconName::Plus,
        ButtonVariant::Subtle => IconName::Info,
        ButtonVariant::Destructive => IconName::Trash,
    }
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("button-demo", cx, |_, _| Demo::default());
    let (activations, last_source, saving) = {
        let d = demo.read(cx);
        (d.activations, d.last_source, d.saving)
    };

    let variants = h_stack(Space::S3).children(ButtonVariant::ALL.iter().map(|variant| {
        Button::new(("variant", *variant as usize), sample_label(*variant)).variant(*variant)
    }));

    let with_icons = h_stack(Space::S3).children(ButtonVariant::ALL.iter().map(|variant| {
        Button::new(("icon", *variant as usize), sample_label(*variant))
            .variant(*variant)
            .icon(sample_icon(*variant))
    }));

    let sizes = v_stack(Space::S4).children(
        [ButtonVariant::Primary, ButtonVariant::Secondary]
            .into_iter()
            .map(|variant| {
                h_stack(Space::S3).children(ButtonSize::ALL.iter().map(move |size| {
                    Button::new(
                        SharedString::from(format!("size-{}-{}", variant.name(), size.name())),
                        size.name(),
                    )
                    .variant(variant)
                    .size(*size)
                    .icon(sample_icon(variant))
                }))
            }),
    );

    let header = |text: &'static str| div().w(px(150.0)).child(caption(text));
    let mut states = v_stack(Space::S3).child(
        h_stack(Space::S3)
            .child(div().w(px(110.0)))
            .child(header("Default"))
            .child(header("Disabled"))
            .child(header("Loading")),
    );
    for variant in ButtonVariant::ALL {
        let id = |state: &str| SharedString::from(format!("state-{}-{state}", variant.name()));
        states = states.child(
            h_stack(Space::S3)
                .child(div().w(px(110.0)).child(metadata(variant.name())))
                .child(
                    div()
                        .w(px(150.0))
                        .child(Button::new(id("default"), sample_label(variant)).variant(variant)),
                )
                .child(
                    div().w(px(150.0)).child(
                        Button::new(id("disabled"), sample_label(variant))
                            .variant(variant)
                            .disabled(true),
                    ),
                )
                .child(
                    div().w(px(150.0)).child(
                        Button::new(id("loading"), sample_label(variant))
                            .variant(variant)
                            .loading(true),
                    ),
                ),
        );
    }

    let counter = demo.clone();
    let saver = demo.clone();
    let interactive = v_stack(Space::S4)
        .child(
            h_stack(Space::S3)
                .child(
                    Button::new("activate", "Activate")
                        .variant(ButtonVariant::Primary)
                        .on_click(move |event, _, cx| {
                            let from = source(event);
                            counter.update(cx, |d, cx| {
                                d.activations += 1;
                                d.last_source = Some(from);
                                cx.notify();
                            });
                        }),
                )
                .child(
                    Button::new("save", if saving { "Saving…" } else { "Save changes" })
                        .icon(IconName::Check)
                        .loading(saving)
                        .on_click(move |_, _, cx| {
                            saver.update(cx, |d, cx| {
                                d.saving = true;
                                cx.notify();
                                cx.spawn(async move |this, cx| {
                                    cx.background_executor()
                                        .timer(Duration::from_millis(1600))
                                        .await;
                                    this.update(cx, |d, cx| {
                                        d.saving = false;
                                        cx.notify();
                                    })
                                    .ok();
                                })
                                .detach();
                            });
                        }),
                ),
        )
        .child(metadata(match last_source {
            Some(from) => format!("Activated {activations}× · last via {from}"),
            None => "Not activated yet".to_string(),
        }))
        .child(caption(
            "Pointer presses do not move keyboard focus. Tab to a button, then press Enter or Space.",
        ));

    v_stack(Space::S8)
        .child(example(
            "Interactive",
            "Activation reports whether it came from the pointer or the keyboard. Save shows the loading state.",
            interactive,
        ))
        .child(example("Variants", "Primary, secondary, subtle, destructive.", variants))
        .child(example("With icons", "A leading icon sized to the control.", with_icons))
        .child(example("Sizes", "Small, medium, and large share one geometry model.", sizes))
        .child(example(
            "States",
            "Disabled buttons are skipped by Tab; loading buttons keep focus and width but ignore activation.",
            states,
        ))
        .into_any_element()
}
