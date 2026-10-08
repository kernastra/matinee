use atelier_ui::gpui::KeyDownEvent;
use atelier_ui::prelude::*;

use super::{caption, example};

const COUNTS: [usize; 3] = [100, 1_000, 10_000];

struct Demo {
    state: VirtualGridState,
    count: usize,
    activated: Option<usize>,
    /// The focus-handoff example: a field the owner focuses, over a grid.
    field: FocusHandle,
    query: SharedString,
    handoff: VirtualGridState,
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
        field: cx.focus_handle(),
        query: SharedString::default(),
        handoff: VirtualGridState::new(cx),
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
        tile(&cell_theme, cell)
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
        .child(handoff(&theme, &demo, cx))
        .into_any_element()
}

fn tile(theme: &Theme, cell: GridCell) -> AnyElement {
    let radius = theme.radius.get(Radius::Medium);
    v_stack(Space::S2)
        .size_full()
        .child(
            div()
                .relative()
                .w_full()
                .flex_1()
                .rounded(px(radius))
                .bg(theme.colors.surface.elevated)
                .border_1()
                .border_color(theme.colors.border.subtle)
                .when(cell.focused, |frame| {
                    frame.child(FocusRing::new(radius, 1.0))
                }),
        )
        .child(Text::new(format!("Item {}", cell.index + 1)).role(TextRole::Caption))
        .into_any_element()
}

/// A field above a grid, as a search screen arranges them. The owner holds
/// the field's focus handle, so it can move focus into the field; the grid
/// reports a key that cannot move (`on_edge`), so Up on its first row can
/// hand focus back.
fn handoff(theme: &Theme, demo: &Entity<Demo>, cx: &mut App) -> impl IntoElement {
    let (field, query, grid) = {
        let demo = demo.read(cx);
        (demo.field.clone(), demo.query.clone(), demo.handoff.clone())
    };
    let cell_theme = theme.clone();
    let typed = demo.clone();
    let (down_field, down_grid) = (field.clone(), grid.clone());
    let up_field = field.clone();
    example(
        "Focus handoff",
        "Tab into the field and type. Down moves into the grid; Up on the grid's first row comes back to the field. Arrow keys inside the field still edit the text.",
        v_stack(Space::S3)
            .w_full()
            .child(
                div()
                    .w_full()
                    .max_w(px(420.0))
                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == "down" && down_field.is_focused(window) {
                            note_keyboard_navigation(cx);
                            down_grid.focus_index(Some(down_grid.focused().unwrap_or(0)));
                            window.focus(down_grid.focus_handle());
                            cx.stop_propagation();
                        }
                    })
                    .child(
                        SearchField::new("handoff-search", query.clone())
                            .placeholder("Filter")
                            .focus_handle(field)
                            .on_change(move |value, _, cx| {
                                typed.update(cx, |demo, cx| {
                                    demo.query = value;
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(caption(format!(
                "query {:?} · focused {}",
                query.as_ref(),
                grid.focused()
                    .map_or("none".to_string(), |index| (index + 1).to_string()),
            )))
            .child(
                div().w_full().h(px(260.0)).child(
                    VirtualGrid::new("handoff-grid", &grid, 60, move |cell, _, _| {
                        tile(&cell_theme, cell)
                    })
                    .sizing(sizing())
                    .on_edge(move |step, window, _| {
                        if step == GridStep::Up {
                            window.focus(&up_field);
                        }
                    }),
                ),
            ),
    )
}
