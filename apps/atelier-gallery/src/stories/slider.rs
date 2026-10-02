use atelier_ui::prelude::*;

use super::{example, metadata, row};

struct Demo {
    value: f32,
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("slider-demo", cx, |_, _| Demo { value: 40.0 });
    let value = demo.read(cx).value;
    let model = demo.clone();

    v_stack(Space::S8)
        .child(example(
            "Interactive",
            "Drag, or use arrows, Home, End, Page Up, and Page Down. Step is 5.",
            v_stack(Space::S3)
                .child(
                    Slider::new("interactive-slider", value)
                        .range(0.0, 100.0)
                        .step(5.0)
                        .inspected(true)
                        .on_change(move |next, _, cx| {
                            model.update(cx, |demo, cx| {
                                demo.value = next;
                                cx.notify();
                            });
                        }),
                )
                .child(metadata(format!("{value:.0}"))),
        ))
        .child(example(
            "States",
            "A mid value and a disabled slider.",
            v_stack(Space::S3)
                .child(row(
                    "Mid",
                    Slider::new("state-mid", 25.0).range(0.0, 100.0).step(1.0),
                ))
                .child(row(
                    "Disabled",
                    Slider::new("state-disabled", 60.0)
                        .range(0.0, 100.0)
                        .step(1.0)
                        .disabled(true),
                )),
        ))
        .into_any_element()
}
