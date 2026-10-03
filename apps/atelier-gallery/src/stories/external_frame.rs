//! Latest-frame surface. The picture is synthetic BGRA. This story does not
//! open a media engine.

use atelier_ui::prelude::*;

use super::example;

struct Demo {
    surface: Entity<ExternalFrameSurface>,
    empty: Entity<ExternalFrameSurface>,
    fit: usize,
    landscape: bool,
    wide_box: bool,
    menu_open: bool,
    generation: u64,
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("external-frame-demo", cx, |window, cx| {
        let surface = cx.new(|cx| ExternalFrameSurface::new("No frame yet", window, cx));
        let empty = cx.new(|cx| ExternalFrameSurface::new("No frame yet", window, cx));
        let _ = surface.read(cx).mailbox().publish(bars(320, 180, 1));
        Demo {
            surface,
            empty,
            fit: 0,
            landscape: true,
            wide_box: true,
            menu_open: false,
            generation: 1,
        }
    });

    let fit = demo.read(cx).fit;
    let wide_box = demo.read(cx).wide_box;
    let menu_open = demo.read(cx).menu_open;
    let generation = demo.read(cx).generation;
    let surface = demo.read(cx).surface.clone();
    let presented = surface.read(cx).frame_size();
    let shown = surface.read(cx).presented_count();

    let toggle_fit = demo.clone();
    let toggle_menu = demo.clone();
    let dismiss_menu = demo.clone();
    let swap = demo.clone();
    let shape = demo.clone();

    v_stack(Space::S8)
        .child(example(
            "Fit and fill",
            "The picture keeps its aspect ratio. Fit letterboxes. Fill covers and clips. A menu opens over the picture.",
            v_stack(Space::S3)
                .child(
                    h_stack(Space::S3)
                        .items_center()
                        .child(
                            SegmentedControl::new(
                                "frame-fit",
                                vec![Segment::new("Fit"), Segment::new("Fill")],
                                fit,
                            )
                            .on_change(move |index, _, cx| {
                                toggle_fit.update(cx, |demo, cx| {
                                    demo.fit = index;
                                    let mode = if index == 0 {
                                        ImageFit::Fit
                                    } else {
                                        ImageFit::Fill
                                    };
                                    demo.surface.update(cx, |surface, cx| {
                                        surface.set_fit(mode, cx);
                                    });
                                    cx.notify();
                                });
                            }),
                        )
                        .child(
                            Button::new("frame-swap", "Swap size")
                                .size(ButtonSize::Small)
                                .on_click(move |_, _, cx| {
                                    swap.update(cx, |demo, cx| {
                                        demo.generation += 1;
                                        demo.landscape = !demo.landscape;
                                        let (width, height) = if demo.landscape {
                                            (320, 180)
                                        } else {
                                            (180, 240)
                                        };
                                        let generation = demo.generation;
                                        let _ = demo.surface.read(cx).mailbox().publish(bars(
                                            width, height, generation,
                                        ));
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            Button::new(
                                "frame-shape",
                                if wide_box { "Square box" } else { "Wide box" },
                            )
                            .size(ButtonSize::Small)
                            .on_click(move |_, _, cx| {
                                shape.update(cx, |demo, cx| {
                                    demo.wide_box = !demo.wide_box;
                                    cx.notify();
                                });
                            }),
                        ),
                )
                .child(frame_stage(
                    surface,
                    wide_box,
                    menu_open,
                    toggle_menu,
                    dismiss_menu,
                ))
                .child(Text::new(match presented {
                    Some((width, height)) => {
                        format!("Presented {width}×{height} · generation {generation} · updates {shown}")
                    }
                    None => "Nothing presented".to_string(),
                }).role(TextRole::Caption).tone(TextTone::Muted)),
        ))
        .child(example(
            "Empty",
            "Before the first picture, the surface shows its label on the canvas color.",
            div().w(px(360.0)).h(px(160.0)).child(demo.read(cx).empty.clone()),
        ))
        .into_any_element()
}

fn frame_stage(
    surface: Entity<ExternalFrameSurface>,
    wide_box: bool,
    menu_open: bool,
    toggle_menu: Entity<Demo>,
    dismiss_menu: Entity<Demo>,
) -> impl IntoElement {
    let width = if wide_box { 420.0 } else { 280.0 };
    div()
        .relative()
        .w(px(width))
        .h(px(240.0))
        .child(surface)
        .child(
            div()
                .absolute()
                .top(Space::S3.px())
                .left(Space::S3.px())
                .child(
                    Popover::new("frame-menu")
                        .open(menu_open)
                        .on_dismiss(move |_, cx| {
                            dismiss_menu.update(cx, |demo, cx| {
                                demo.menu_open = false;
                                cx.notify();
                            });
                        })
                        .trigger(
                            Button::new("frame-menu-trigger", "View")
                                .size(ButtonSize::Small)
                                .on_click(move |_, _, cx| {
                                    toggle_menu.update(cx, |demo, cx| {
                                        demo.menu_open = !demo.menu_open;
                                        cx.notify();
                                    });
                                }),
                        )
                        .menu(vec![
                            MenuEntry::Item(MenuItem::new("Picture").disabled(true)),
                            MenuEntry::Separator(MenuSeparator),
                            MenuEntry::Item(MenuItem::new("Fit is the default")),
                        ]),
                ),
        )
}

fn bars(width: u32, height: u32, generation: u64) -> BgraFrame {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let index = ((y * width + x) * 4) as usize;
            let band = (x * 8 / width.max(1)) as u8;
            pixels[index] = 40 + band * 24;
            pixels[index + 1] = 70 + (y * 140 / height.max(1)) as u8;
            pixels[index + 2] = 180u8.saturating_sub(band * 16);
            pixels[index + 3] = 255;
        }
    }
    BgraFrame::new(width, height, width * 4, pixels, generation).expect("bars")
}
