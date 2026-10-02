use atelier_ui::prelude::*;

use super::{example, row};

struct Demo {
    on: bool,
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("switch-demo", cx, |_, _| Demo { on: true });
    let on = demo.read(cx).on;
    let model = demo.clone();
    let toggle = move |next: bool, _: &mut Window, cx: &mut App| {
        model.update(cx, |demo, cx| {
            demo.on = next;
            cx.notify();
        });
    };

    v_stack(Space::S8)
        .child(example(
            "Interactive",
            "Click, or focus it and press Space or Enter. Reduced motion snaps the thumb.",
            Switch::new("interactive-switch", on)
                .label(if on { "On" } else { "Off" })
                .inspected(true)
                .on_change(toggle),
        ))
        .child(example(
            "States",
            "Default, disabled off, and disabled on.",
            v_stack(Space::S3)
                .child(row("Default", Switch::new("state-off", false).label("Off")))
                .child(row(
                    "Disabled",
                    Switch::new("state-disabled", false)
                        .label("Off")
                        .disabled(true),
                ))
                .child(row(
                    "Disabled on",
                    Switch::new("state-disabled-on", true)
                        .label("On")
                        .disabled(true),
                )),
        ))
        .into_any_element()
}
