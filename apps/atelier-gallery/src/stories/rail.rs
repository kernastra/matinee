use atelier_ui::prelude::*;

use super::{caption, example};

const COUNT: usize = 14;

struct Demo {
    state: RailState,
    focus: Vec<FocusHandle>,
    pressed: Option<usize>,
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("rail-demo", cx, |_, cx| Demo {
        state: RailState::new(),
        focus: (0..COUNT)
            .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
            .collect(),
        pressed: None,
    });
    let (state, focus, pressed) = {
        let demo = demo.read(cx);
        (demo.state.clone(), demo.focus.clone(), demo.pressed)
    };
    let theme = cx.theme().clone();
    let mut rail = Rail::new("gallery-rail", &state).w_full().p(Space::S1.px());
    for (index, handle) in focus.into_iter().enumerate() {
        let model = demo.clone();
        rail = rail.item(
            handle.clone(),
            Pressable::new(("rail-tile", index), format!("Tile {}", index + 1))
                .focus_handle(handle)
                .flex_none()
                .p(Space::S1.px())
                .on_press(move |_, _, cx| {
                    model.update(cx, |demo, cx| {
                        demo.pressed = Some(index);
                        cx.notify();
                    });
                })
                .child(
                    v_stack(Space::S2)
                        .child(
                            div()
                                .w(px(120.0))
                                .h(px(72.0))
                                .rounded(px(theme.radius.get(Radius::Medium)))
                                .bg(theme.colors.surface.elevated),
                        )
                        .child(Text::new(format!("Tile {}", index + 1)).role(TextRole::Label)),
                ),
        );
    }
    let back = state.clone();
    let forward = state.clone();
    let model = demo.clone();
    let model_forward = demo.clone();

    v_stack(Space::S8)
        .child(example(
            "Rail",
            "Tab into the row, then use Left, Right, Home, and End. Focus that lands off screen scrolls into view. A vertical mouse wheel scrolls the page, not the row; a trackpad or Shift-wheel moves it sideways.",
            v_stack(Space::S3)
                .w_full()
                .child(
                    h_stack(Space::S2)
                        .items_center()
                        .child(
                            IconButton::new("rail-back", IconName::ChevronLeft, "Scroll back")
                                .disabled(!state.can_page_back())
                                .on_click(move |_, _, cx| {
                                    back.page(false);
                                    model.update(cx, |_, cx| cx.notify());
                                }),
                        )
                        .child(
                            IconButton::new("rail-forward", IconName::ChevronRight, "Scroll forward")
                                .disabled(!state.can_page_forward())
                                .on_click(move |_, _, cx| {
                                    forward.page(true);
                                    model_forward.update(cx, |_, cx| cx.notify());
                                }),
                        )
                        .child(caption(match pressed {
                            Some(index) => format!("Pressed tile {}", index + 1),
                            None => "Nothing pressed yet".to_string(),
                        })),
                )
                .child(rail),
        ))
        .into_any_element()
}
