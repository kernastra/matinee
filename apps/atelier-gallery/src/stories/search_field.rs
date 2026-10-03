use atelier_ui::prelude::*;

use super::{example, metadata, row};

struct Demo {
    value: SharedString,
    dismissed: bool,
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("search-field-demo", cx, |_, _| Demo {
        value: SharedString::default(),
        dismissed: false,
    });
    let value = demo.read(cx).value.clone();
    let dismissed = demo.read(cx).dismissed;
    let model = demo.clone();

    v_stack(Space::S8)
        .child(example(
            "Interactive",
            "Escape clears the query, then dismisses when it is already empty.",
            v_stack(Space::S3)
                .child(
                    SearchField::new("interactive-search", value.clone())
                        .placeholder("Search")
                        .inspected(true)
                        .on_change({
                            let model = model.clone();
                            move |value, _, cx| {
                                model.update(cx, |demo, cx| {
                                    demo.value = value;
                                    demo.dismissed = false;
                                    cx.notify();
                                });
                            }
                        })
                        .on_dismiss({
                            let model = model.clone();
                            move |_, cx| {
                                model.update(cx, |demo, cx| {
                                    demo.dismissed = true;
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(metadata(if dismissed {
                    "Dismissed".to_string()
                } else {
                    format!("Query: {value}")
                })),
        ))
        .child(example(
            "States",
            "Filled and disabled.",
            v_stack(Space::S4)
                .child(row(
                    "Filled",
                    SearchField::new("filled-search", "north by northwest"),
                ))
                .child(row(
                    "Disabled",
                    SearchField::new("disabled-search", "archived").disabled(true),
                )),
        ))
        .into_any_element()
}
