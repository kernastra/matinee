use atelier_ui::prelude::*;

use super::{example, row};

struct Demo {
    selected: usize,
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("segmented-demo", cx, |_, _| Demo { selected: 0 });
    let selected = demo.read(cx).selected;
    let model = demo.clone();
    let segments = vec![
        Segment::new("Day").icon(IconName::Play),
        Segment::new("Week").disabled(true),
        Segment::new("Month").icon(IconName::Sliders),
        Segment::new("Year"),
    ];

    v_stack(Space::S8)
        .child(example(
            "Interactive",
            "Click a segment, or use the arrow keys. Week is disabled and skipped.",
            SegmentedControl::new("interactive-segments", segments, selected)
                .inspected(true)
                .on_change(move |index, _, cx| {
                    model.update(cx, |demo, cx| {
                        demo.selected = index;
                        cx.notify();
                    });
                }),
        ))
        .child(example(
            "States",
            "A compact group and a fully disabled group.",
            v_stack(Space::S3)
                .child(row(
                    "Compact",
                    SegmentedControl::new(
                        "compact-segments",
                        vec![Segment::new("List"), Segment::new("Grid")],
                        1,
                    ),
                ))
                .child(row(
                    "Disabled",
                    SegmentedControl::new(
                        "disabled-segments",
                        vec![Segment::new("One"), Segment::new("Two")],
                        0,
                    )
                    .disabled(true),
                )),
        ))
        .into_any_element()
}
