use atelier_ui::prelude::*;

use super::{example, row};

struct Demo {
    state: CheckboxState,
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("checkbox-demo", cx, |_, _| Demo {
        state: CheckboxState::Indeterminate,
    });
    let state = demo.read(cx).state;
    let model = demo.clone();

    v_stack(Space::S8)
        .child(example(
            "Interactive",
            "Indeterminate becomes checked. Checked becomes unchecked.",
            Checkbox::new("interactive-checkbox", state)
                .label(state.name())
                .inspected(true)
                .on_change(move |next, _, cx| {
                    model.update(cx, |demo, cx| {
                        demo.state = next;
                        cx.notify();
                    });
                }),
        ))
        .child(example(
            "States",
            "The three values, plus a disabled checked box.",
            v_stack(Space::S3)
                .child(row(
                    "Unchecked",
                    Checkbox::new("state-unchecked", CheckboxState::Unchecked).label("Unchecked"),
                ))
                .child(row(
                    "Checked",
                    Checkbox::new("state-checked", CheckboxState::Checked).label("Checked"),
                ))
                .child(row(
                    "Indeterminate",
                    Checkbox::new("state-mixed", CheckboxState::Indeterminate).label("Mixed"),
                ))
                .child(row(
                    "Disabled",
                    Checkbox::new("state-disabled", CheckboxState::Checked)
                        .label("Checked")
                        .disabled(true),
                )),
        ))
        .into_any_element()
}
