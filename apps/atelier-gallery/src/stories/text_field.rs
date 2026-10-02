use atelier_ui::prelude::*;

use super::{example, metadata, row};

struct Demo {
    value: SharedString,
    focused: bool,
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("text-field-demo", cx, |_, _| Demo {
        value: "Matinee".into(),
        focused: false,
    });
    let value = demo.read(cx).value.clone();
    let focused = demo.read(cx).focused;
    let model = demo.clone();

    v_stack(Space::S8)
        .child(example(
            "Interactive",
            "Type, select, and move the caret. The value is controlled by this story.",
            v_stack(Space::S3)
                .child(
                    TextField::new("interactive-field", value.clone())
                        .placeholder("Type a title")
                        .label("Title")
                        .supporting_text(
                            "Enter commits the value. The caret is the focus indicator.",
                        )
                        .leading_icon(IconName::Info)
                        .inspected(true)
                        .on_focus_change({
                            let model = model.clone();
                            move |focused, cx| {
                                model.update(cx, |demo, cx| {
                                    demo.focused = focused;
                                    cx.notify();
                                });
                            }
                        })
                        .on_change({
                            let model = model.clone();
                            move |value, _, cx| {
                                model.update(cx, |demo, cx| {
                                    demo.value = value;
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(metadata(format!(
                    "Focused {focused} · {} characters",
                    value.len()
                ))),
        ))
        .child(example(
            "States",
            "Placeholder, invalid, disabled, and a trailing action.",
            v_stack(Space::S4)
                .child(row(
                    "Placeholder",
                    TextField::new("placeholder-field", "")
                        .placeholder("Search is a different control"),
                ))
                .child(row(
                    "Invalid",
                    TextField::new("invalid-field", "??")
                        .invalid(true)
                        .supporting_text("Enter a title"),
                ))
                .child(row(
                    "Disabled",
                    TextField::new("disabled-field", "Locked").disabled(true),
                ))
                .child(row(
                    "Trailing",
                    TextField::new("trailing-field", value).trailing(IconName::Close, "Clear", {
                        let model = model.clone();
                        move |_, cx| {
                            model.update(cx, |demo, cx| {
                                demo.value = SharedString::default();
                                cx.notify();
                            });
                        }
                    }),
                )),
        ))
        .into_any_element()
}
