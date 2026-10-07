use atelier_ui::prelude::*;

use super::{caption, example};

const COUNTS: [usize; 3] = [100, 1_000, 10_000];

struct Demo {
    state: VirtualGridState,
    count: usize,
    activated: Option<usize>,
}

fn sizing() -> GridSizing {
    GridSizing {
        min_cell_width: 96.0,
        max_cell_width: 132.0,
        aspect: 1.0,
        extra_height: Space::S6.value(),
        column_gap: Space::S4.value(),
        row_gap: Space::S5.value(),
        inset_x: Space::S2.value(),
        inset_top: Space::S2.value(),
        inset_bottom: Space::S2.value(),
        max_columns: 10,
        overscan_rows: 1,
    }
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("virtual-grid-demo", cx, |_, cx| Demo {
        state: VirtualGridState::new(cx),
        count: COUNTS[1],
        activated: None,
    });
    let (state, count, activated) = {
        let demo = demo.read(cx);
        (demo.state.clone(), demo.count, demo.activated)
    };
    let theme = cx.theme().clone();
    let selected = COUNTS.iter().position(|c| *c == count).unwrap_or(0);
    let pick = demo.clone();
    let press = demo.clone();
    let cell_theme = theme.clone();

    let grid = VirtualGrid::new("gallery-grid", &state, count, move |cell, _, _| {
        let radius = cell_theme.radius.get(Radius::Medium);
        v_stack(Space::S2)
            .size_full()
            .child(
                div()
                    .relative()
                    .w_full()
                    .flex_1()
                    .rounded(px(radius))
                    .bg(cell_theme.colors.surface.elevated)
                    .border_1()
                    .border_color(cell_theme.colors.border.subtle)
                    .when(cell.focused, |frame| {
                        frame.child(FocusRing::new(radius, 1.0))
                    }),
            )
            .child(Text::new(format!("Item {}", cell.index + 1)).role(TextRole::Caption))
            .into_any_element()
    })
    .sizing(sizing())
    .on_activate(move |index, _, cx| {
        press.update(cx, |demo, cx| {
            demo.activated = Some(index);
            cx.notify();
        });
    });

    v_stack(Space::S8)
        .child(example(
            "Virtual Grid",
            "Fixed-size cells in as many columns as the width allows. Only the visible rows and one row either side are built, whatever the item count. Tab into the grid, then use the arrows, Page Up, Page Down, Home, and End; Enter or Space activates. Resize the window: the focused item stays the same item.",
            v_stack(Space::S3)
                .w_full()
                .child(
                    h_stack(Space::S3)
                        .items_center()
                        .child(
                            SegmentedControl::new(
                                "grid-count",
                                COUNTS
                                    .iter()
                                    .map(|count| Segment::new(format!("{count} items")))
                                    .collect(),
                                selected,
                            )
                            .on_change(move |index, _, cx| {
                                pick.update(cx, |demo, cx| {
                                    demo.count = COUNTS[index];
                                    demo.activated = None;
                                    demo.state.reset();
                                    cx.notify();
                                });
                            }),
                        )
                        .child(caption(format!(
                            "{} cells built · focused {} · activated {}",
                            state.rendered_cells(),
                            state
                                .focused()
                                .map_or("none".to_string(), |index| (index + 1).to_string()),
                            activated.map_or("none".to_string(), |index| (index + 1).to_string()),
                        ))),
                )
                .child(div().w_full().h(px(420.0)).child(grid)),
        ))
        .into_any_element()
}
