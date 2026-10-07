//! Stories for scrolling, lists, images, progress, and overlays.

use atelier_ui::gpui::Point;
use atelier_ui::prelude::*;

use super::{caption, example};

pub fn scroll_view(window: &mut Window, cx: &mut App) -> AnyElement {
    let vertical = window.use_keyed_state("scroll-vertical", cx, |_, _| ScrollControl::new());
    let horizontal = window.use_keyed_state("scroll-horizontal", cx, |_, _| ScrollControl::new());
    let vertical = vertical.read(cx).clone();
    let horizontal = horizontal.read(cx).clone();
    let offset = vertical.offset();

    v_stack(Space::S6)
        .child(example(
            "Vertical",
            "Wheel, trackpad, and — when the view itself is focused — arrow keys. The list keeps the arrows unless this view is focusable.",
            v_stack(Space::S2)
                .child(
                    ScrollView::vertical("scroll-demo")
                        .control(vertical)
                        .focusable(true)
                        .h(px(160.0))
                        .w(px(280.0))
                        .child(
                            v_stack(Space::S2).children((0..24).map(|index| {
                                Text::new(format!("Line {index}")).role(TextRole::Label)
                            })),
                        ),
                )
                .child(caption(format!(
                    "Offset y {:.0}",
                    f32::from(offset.y)
                ))),
        ))
        .child(example(
            "Horizontal",
            "The horizontal axis clips and scrolls independently.",
            ScrollView::new("scroll-x")
                .axis(ScrollAxis::Horizontal)
                .control(horizontal)
                .h(px(72.0))
                .w(px(280.0))
                .child(
                    h_stack(Space::S2).children((0..12).map(|index| {
                        Surface::new(SurfaceLevel::Elevated)
                            .padding(Space::S3)
                            .child(Text::new(format!("Item {index}")).role(TextRole::Label))
                    })),
                ),
        ))
        .into_any_element()
}

pub fn list(window: &mut Window, cx: &mut App) -> AnyElement {
    let demo = window.use_keyed_state("list-demo", cx, |_, _| ListDemo::default());
    let selected = demo.read(cx).selected;
    let activated = demo.read(cx).activated;
    let scroll = window.use_keyed_state("list-scroll", cx, |_, _| ScrollControl::new());
    let scroll = scroll.read(cx).clone();
    let stress = window.use_keyed_state("list-stress-scroll", cx, |_, _| ScrollControl::new());
    let stress = stress.read(cx).clone();

    let rows = sample_rows();
    let select = demo.clone();
    let activate = demo.clone();

    v_stack(Space::S6)
        .child(example(
            "Selection",
            "One tab stop. Up and Down skip the disabled row. Enter activates. A pointer click selects without moving keyboard focus. Right-click is available to the caller.",
            v_stack(Space::S3)
                .child(
                    ScrollView::vertical("list-scroll-view")
                        .control(scroll.clone())
                        .h(px(220.0))
                        .child(
                            List::new("sample-list", rows)
                                .selected(selected)
                                .scroll_control(scroll)
                                .inspected(true)
                                .on_select(move |index, _, cx| {
                                    select.update(cx, |demo, cx| {
                                        demo.selected = Some(index);
                                        cx.notify();
                                    });
                                })
                                .on_activate(move |index, _, cx| {
                                    activate.update(cx, |demo, cx| {
                                        demo.activated = Some(index);
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(caption(match activated {
                    Some(index) => format!("Activated row {index}"),
                    None => "Enter activates the selection".to_string(),
                })),
        ))
        .child(example(
            "Stress",
            "A few hundred rows with sample frames. GPUI scrolls the parent view; this list is not virtualized.",
            ScrollView::vertical("stress-scroll")
                .control(stress.clone())
                .h(px(240.0))
                .child(
                    List::new(
                        "stress-list",
                        (0..360)
                            .map(|index| {
                                ListRow::new(format!("Record {index}"))
                                    .secondary("Secondary line")
                                    .leading(
                                        Image::sample(("stress-image", index), index)
                                            .frame(36.0, 28.0)
                                            .fit(ImageFit::Fill),
                                    )
                            })
                            .collect(),
                    )
                    .scroll_control(stress),
                ),
        ))
        .into_any_element()
}

pub fn image(_window: &mut Window, cx: &mut App) -> AnyElement {
    let decoded = decoded_sample(cx);
    v_stack(Space::S6)
        .child(example(
            "Fit and fill",
            "The frame is fixed. Fit keeps the whole image visible. Fill covers the frame and clips.",
            h_stack(Space::S4)
                .child(Image::sample("fit", 1).frame(120.0, 72.0).fit(ImageFit::Fit))
                .child(
                    Image::sample("fill", 1)
                        .frame(120.0, 72.0)
                        .fit(ImageFit::Fill)
                        .radius(Radius::Small),
                ),
        ))
        .child(example(
            "Placeholder and failure",
            "Pending stays on the loading placeholder. A failed source shows the fallback.",
            h_stack(Space::S4)
                .child(Image::pending("pending").frame(96.0, 72.0).label("Loading"))
                .child(Image::failed("failed").frame(96.0, 72.0).label("Unavailable")),
        ))
        .child(example(
            "Decoded bytes",
            "An application fetches bytes itself and decodes them off the UI thread. Damaged or unsupported bytes become a failure.",
            h_stack(Space::S4)
                .child(match decoded {
                    Some(image) => Image::decoded("decoded", image)
                        .frame(120.0, 72.0)
                        .fit(ImageFit::Fill)
                        .label("Decoded"),
                    None => Image::failed("decoded").frame(120.0, 72.0).label("Unavailable"),
                })
                .child(
                    match DecodedImage::decode(b"not an image", MAX_DECODED_SIDE) {
                        Ok(image) => Image::decoded("damaged", image),
                        Err(_) => Image::failed("damaged"),
                    }
                    .frame(96.0, 72.0)
                    .label("Damaged bytes"),
                ),
        ))
        .into_any_element()
}

pub fn pressable(_window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme().clone();
    let row = |id: &'static str,
               index: usize,
               title: &'static str,
               detail: &'static str,
               disabled: bool| {
        Pressable::new(id, title)
            .disabled(disabled)
            .on_press(|_, _, _| {})
            .w(px(420.0))
            .p(Space::S2.px())
            .child(
                h_stack(Space::S3)
                    .items_center()
                    .child(
                        Image::sample((id, index), index)
                            .frame(96.0, 54.0)
                            .fit(ImageFit::Fill),
                    )
                    .child(
                        v_stack(Space::S1)
                            .child(Text::new(title).role(TextRole::Label))
                            .child(
                                Text::new(detail)
                                    .role(TextRole::Caption)
                                    .tone(TextTone::Muted),
                            ),
                    ),
            )
    };
    v_stack(Space::S6)
        .child(example(
            "Rows",
            "Tab moves between rows. Enter or Space activates the focused row. A pointer press activates without a focus ring.",
            v_stack(Space::S1)
                .child(row("row-one", 0, "First row", "Image, title, and detail", false))
                .child(row("row-two", 1, "Second row", "Hover and pressed fills come from the theme", false))
                .child(row("row-three", 2, "Disabled row", "Visible, not a tab stop", true)),
        ))
        .child(example(
            "Tile",
            "The same control as a poster tile. Layout belongs to the caller.",
            Pressable::new("tile", "Tile")
                .on_press(|_, _, _| {})
                .w(px(140.0))
                .p(Space::S1.px())
                .child(
                    v_stack(Space::S2)
                        .child(Image::sample("tile-art", 3).frame(132.0, 198.0).fit(ImageFit::Fill))
                        .child(
                            Text::new("Tile title")
                                .role(TextRole::Label)
                                .color(theme.colors.text.primary),
                        ),
                ),
        ))
        .into_any_element()
}

/// A bundled sample decoded once from its encoded bytes, the way an
/// application decodes fetched artwork.
fn decoded_sample(cx: &mut App) -> Option<DecodedImage> {
    thread_local! {
        static DECODED: std::cell::OnceCell<Option<DecodedImage>> = const { std::cell::OnceCell::new() };
    }
    DECODED.with(|cell| {
        cell.get_or_init(|| {
            let bytes = cx.asset_source().load(&sample_asset(2)).ok()??;
            DecodedImage::decode(&bytes, MAX_DECODED_SIDE).ok()
        })
        .clone()
    })
}

pub fn progress(_window: &mut Window, _cx: &mut App) -> AnyElement {
    example(
        "Meters",
        "Determinate values clamp to the track. Indeterminate travel stops under reduced motion.",
        v_stack(Space::S4)
            .child(meter("Empty", ProgressBar::determinate(0.0)))
            .child(meter("Partial", ProgressBar::determinate(0.45)))
            .child(meter("Full", ProgressBar::determinate(1.2)))
            .child(meter(
                "Working",
                ProgressBar::indeterminate().label("Working"),
            ))
            .child(meter(
                "Disabled",
                ProgressBar::indeterminate().disabled(true),
            )),
    )
    .into_any_element()
}

pub fn tooltip(_window: &mut Window, _cx: &mut App) -> AnyElement {
    example(
        "Hover delay",
        "The tip uses GPUI's hover delay, about half a second, so a passing pointer does not flash it. It does not take focus.",
        h_stack(Space::S3)
            .child(IconButton::new("tip-info", IconName::Info, "Details"))
            .child(IconButton::new("tip-more", IconName::More, "More actions"))
            .child(
                Button::new("tip-save", "Save")
                    .icon(IconName::Check)
                    .on_click(|_, _, _| {}),
            ),
    )
    .into_any_element()
}

pub fn popover(window: &mut Window, cx: &mut App) -> AnyElement {
    let open = window.use_keyed_state("popover-open", cx, |_, _| false);
    let is_open = *open.read(cx);
    let toggle = open.clone();
    let dismiss = open.clone();
    example(
        "Anchored",
        "Preferred placement is below the trigger. The layer flips if it would leave the window. Escape and a press outside close it and return focus.",
        Popover::new("info-popover")
            .open(is_open)
            .placement(Placement::Bottom)
            .align(Alignment::Start)
            .on_dismiss(move |_, cx| {
                dismiss.update(cx, |open, cx| {
                    *open = false;
                    cx.notify();
                });
            })
            .trigger(
                Button::new("popover-trigger", if is_open { "Hide note" } else { "Show note" })
                    .on_click(move |_, _, cx| {
                        toggle.update(cx, |open, cx| {
                            *open = !*open;
                            cx.notify();
                        });
                    }),
            )
            .content(
                v_stack(Space::S2)
                    .w(px(220.0))
                    .child(Text::new("Note").role(TextRole::Label))
                    .child(
                        Text::new("A short explanation anchored to the button that opened it.")
                            .role(TextRole::Caption)
                            .tone(TextTone::Secondary),
                    ),
            ),
    )
    .into_any_element()
}

pub fn menu(window: &mut Window, cx: &mut App) -> AnyElement {
    let state = window.use_keyed_state("menu-demo", cx, |_, _| MenuDemo::default());
    let open = state.read(cx).open;
    let checked = state.read(cx).checked;
    let last = state.read(cx).last.clone();
    let toggle = state.clone();
    let dismiss = state.clone();
    let check = state.clone();
    let pick = state.clone();
    example(
        "Menu",
        "Up and Down skip disabled rows. Enter or Space activates. Escape closes. A destructive row is available but not the first stop.",
        v_stack(Space::S3)
            .child(
                Popover::new("sample-menu")
                    .open(open)
                    .on_dismiss(move |_, cx| {
                        dismiss.update(cx, |demo, cx| {
                            demo.open = false;
                            cx.notify();
                        });
                    })
                    .trigger(
                        Button::new("menu-trigger", "Actions")
                            .icon(IconName::More)
                            .on_click(move |_, _, cx| {
                                toggle.update(cx, |demo, cx| {
                                    demo.open = !demo.open;
                                    cx.notify();
                                });
                            }),
                    )
                    .menu(menu_entries(checked, check, pick)),
            )
            .child(caption(match last {
                Some(label) => format!("Last action: {label}"),
                None => "No action yet".to_string(),
            })),
    )
    .into_any_element()
}

pub fn context_menu(window: &mut Window, cx: &mut App) -> AnyElement {
    let state = window.use_keyed_state("context-demo", cx, |_, _| ContextDemo::default());
    let open = state.read(cx).open;
    let at = state.read(cx).at;
    let last = state.read(cx).last.clone();
    let open_at = state.clone();
    let dismiss = state.clone();
    let choose = state.clone();
    example(
        "Context menu",
        "Secondary click opens the same menu at the pointer. Escape or a press outside dismisses it.",
        v_stack(Space::S3)
            .child(
                div()
                    .id("context-target")
                    .h(px(96.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .bg(cx.theme().colors.surface.elevated)
                    .cursor_pointer()
                    .child(caption("Right-click"))
                    .on_mouse_down(MouseButton::Right, move |event, _, cx| {
                        let position = event.position;
                        open_at.update(cx, |demo, cx| {
                            demo.open = true;
                            demo.at = position;
                            cx.notify();
                        });
                    }),
            )
            .child(
                ContextMenu::new("sample-context")
                    .open(open)
                    .at(at)
                    .on_dismiss(move |_, cx| {
                        dismiss.update(cx, |demo, cx| {
                            demo.open = false;
                            cx.notify();
                        });
                    })
                    .entries(vec![
                        MenuEntry::Item(MenuItem::new("Open").on_activate({
                            let choose = choose.clone();
                            move |_, cx| {
                                choose.update(cx, |demo, cx| {
                                    demo.last = Some("Open".into());
                                    demo.open = false;
                                    cx.notify();
                                });
                            }
                        })),
                        MenuEntry::Separator(MenuSeparator),
                        MenuEntry::Item(
                            MenuItem::new("Remove").destructive(true).on_activate(move |_, cx| {
                                choose.update(cx, |demo, cx| {
                                    demo.last = Some("Remove".into());
                                    demo.open = false;
                                    cx.notify();
                                });
                            }),
                        ),
                    ]),
            )
            .child(caption(match last {
                Some(label) => format!("Last action: {label}"),
                None => "No action yet".to_string(),
            })),
    )
    .into_any_element()
}

pub fn dialog(window: &mut Window, cx: &mut App) -> AnyElement {
    let state = window.use_keyed_state("dialog-demo", cx, |_, _| DialogDemo::default());
    let open = state.read(cx).open;
    let destructive = state.read(cx).destructive;
    let last = state.read(cx).last.clone();
    let opener = state.clone();
    let safe = state.clone();
    let danger = state.clone();
    example(
        "Modal",
        "Focus moves inside and Tab stays there. A destructive confirmation focuses Cancel, so Enter does not confirm. Escape cancels.",
        v_stack(Space::S3)
            .child(
                h_stack(Space::S3)
                    .child(Button::new("open-safe", "Ask").on_click({
                        let opener = opener.clone();
                        move |_, _, cx| {
                            opener.update(cx, |demo, cx| {
                                demo.destructive = false;
                                demo.open = true;
                                cx.notify();
                            });
                        }
                    }))
                    .child(Button::new("open-danger", "Remove…").variant(ButtonVariant::Destructive).on_click(
                        move |_, _, cx| {
                            opener.update(cx, |demo, cx| {
                                demo.destructive = true;
                                demo.open = true;
                                cx.notify();
                            });
                        },
                    )),
            )
            .child(dialog_layer(open, destructive, safe, danger))
            .child(caption(match last {
                Some(label) => format!("Closed with: {label}"),
                None => "Not opened".to_string(),
            })),
    )
    .into_any_element()
}

pub fn sidebar(window: &mut Window, cx: &mut App) -> AnyElement {
    let selected = window.use_keyed_state("sidebar-demo", cx, |_, _| Some(0usize));
    let current = *selected.read(cx);
    let choose = selected.clone();
    example(
        "Sections",
        "Up and Down move the flat selection and skip disabled items. The footer stays at the bottom.",
        div().h(px(280.0)).w(px(240.0)).child(
            Sidebar::new(
                "sample-sidebar",
                vec![
                    SidebarSection::new(vec![
                        SidebarItem::new("Home").icon(IconName::Home),
                        SidebarItem::new("Favorites").icon(IconName::Heart),
                        SidebarItem::new("Archive").icon(IconName::Folder).disabled(true),
                    ])
                    .label("Library"),
                    SidebarSection::new(vec![SidebarItem::new("Preferences").icon(IconName::Sliders)])
                        .label("Settings"),
                ],
            )
            .selected(current)
            .on_select(move |index, _, cx| {
                choose.update(cx, |selected, cx| {
                    *selected = Some(index);
                    cx.notify();
                });
            })
            .footer(caption("4 items")),
        ),
    )
    .into_any_element()
}

pub fn toolbar(window: &mut Window, cx: &mut App) -> AnyElement {
    let query = window.use_keyed_state("toolbar-query", cx, |_, _| SharedString::default());
    let value = query.read(cx).clone();
    let edit = query.clone();
    example(
        "Leading, title, trailing",
        "The title sits in the center. The bar does not draw window controls.",
        Toolbar::new()
            .leading(
                Button::new("tb-new", "New")
                    .icon(IconName::Plus)
                    .size(ButtonSize::Small),
            )
            .center(Text::new("Documents").role(TextRole::Subheading))
            .trailing(
                SearchField::new("tb-search", value).on_change(move |value, _, cx| {
                    edit.update(cx, |query, cx| {
                        *query = value;
                        cx.notify();
                    });
                }),
            )
            .trailing(IconButton::new("tb-more", IconName::More, "More")),
    )
    .into_any_element()
}

pub fn split_view(window: &mut Window, cx: &mut App) -> AnyElement {
    let width = window.use_keyed_state("split-width", cx, |_, _| 200.0f32);
    let current = *width.read(cx);
    let resize = width.clone();
    example(
        "Divider",
        "Drag the divider. The leading pane is sized; the trailing pane takes the rest. An inspector would be a later pane after that flexible region.",
        v_stack(Space::S2)
            .child(
                div().h(px(200.0)).w_full().child(
                    SplitView::new(
                        "sample-split",
                        Surface::new(SurfaceLevel::Panel)
                            .padding(Space::S3)
                            .child(Text::new("Leading").role(TextRole::Label)),
                        Surface::new(SurfaceLevel::Elevated)
                            .padding(Space::S3)
                            .child(Text::new("Content").role(TextRole::Label)),
                    )
                    .leading_width(current)
                    .on_resize(move |next, _, cx| {
                        resize.update(cx, |width, cx| {
                            *width = next;
                            cx.notify();
                        });
                    }),
                ),
            )
            .child(caption(format!("Leading width {:.0}", current))),
    )
    .into_any_element()
}

pub fn empty_state(window: &mut Window, cx: &mut App) -> AnyElement {
    let cleared = window.use_keyed_state("empty-cleared", cx, |_, _| false);
    let done = *cleared.read(cx);
    let clear = cleared.clone();
    example(
        "Nothing to show",
        "Icon, title, description, and optional actions. The actions are the caller's buttons.",
        if done {
            Text::new("Search cleared")
                .role(TextRole::Body)
                .into_any_element()
        } else {
            EmptyState::new("No matches")
                .icon(IconName::Search)
                .description("Try a different search.")
                .primary_action(Button::new("empty-clear", "Clear search").on_click(
                    move |_, _, cx| {
                        clear.update(cx, |done, cx| {
                            *done = true;
                            cx.notify();
                        });
                    },
                ))
                .secondary_action(
                    Button::new("empty-close", "Close").variant(ButtonVariant::Subtle),
                )
                .into_any_element()
        },
    )
    .into_any_element()
}

#[derive(Default)]
struct ListDemo {
    selected: Option<usize>,
    activated: Option<usize>,
}

fn sample_rows() -> Vec<ListRow> {
    vec![
        ListRow::new("Field notes")
            .secondary("Updated today")
            .leading(thumb(0)),
        ListRow::new("Quarterly plan")
            .secondary("Shared")
            .leading(thumb(1)),
        ListRow::new("Inventory")
            .secondary("Unavailable")
            .disabled(true)
            .leading(thumb(2)),
        ListRow::new("Route map")
            .secondary("Local")
            .leading(thumb(3)),
    ]
}

fn thumb(index: usize) -> Image {
    Image::sample(("row-thumb", index), index)
        .frame(40.0, 32.0)
        .fit(ImageFit::Fill)
}

fn meter(label: &'static str, bar: ProgressBar) -> impl IntoElement {
    v_stack(Space::S1).child(caption(label)).child(bar)
}

#[derive(Default)]
struct MenuDemo {
    open: bool,
    checked: bool,
    last: Option<SharedString>,
}

fn menu_entries(checked: bool, check: Entity<MenuDemo>, pick: Entity<MenuDemo>) -> Vec<MenuEntry> {
    vec![
        MenuEntry::Item(MenuItem::new("New note").icon(IconName::Plus).on_activate({
            let pick = pick.clone();
            move |_, cx| record(&pick, "New note", cx)
        })),
        MenuEntry::Item(
            MenuItem::new("Pin")
                .checked(checked)
                .on_activate(move |_, cx| {
                    check.update(cx, |demo, cx| {
                        demo.checked = !demo.checked;
                        demo.last = Some("Pin".into());
                        demo.open = false;
                        cx.notify();
                    });
                }),
        ),
        MenuEntry::Item(MenuItem::new("Duplicate").disabled(true)),
        MenuEntry::Separator(MenuSeparator),
        MenuEntry::Item(
            MenuItem::new("Remove")
                .destructive(true)
                .on_activate(move |_, cx| record(&pick, "Remove", cx)),
        ),
    ]
}

fn record(entity: &Entity<MenuDemo>, label: &'static str, cx: &mut App) {
    entity.update(cx, |demo, cx| {
        demo.last = Some(label.into());
        demo.open = false;
        cx.notify();
    });
}

#[derive(Default)]
struct ContextDemo {
    open: bool,
    at: Point<Pixels>,
    last: Option<SharedString>,
}

#[derive(Default)]
struct DialogDemo {
    open: bool,
    destructive: bool,
    last: Option<SharedString>,
}

fn dialog_layer(
    open: bool,
    destructive: bool,
    safe: Entity<DialogDemo>,
    danger: Entity<DialogDemo>,
) -> Dialog {
    let mut dialog = Dialog::new("sample-dialog")
        .open(open)
        .title(if destructive {
            "Remove this document?"
        } else {
            "Save changes?"
        })
        .content(Text::new(if destructive {
            "This cannot be undone."
        } else {
            "You have unsaved edits."
        }))
        .on_dismiss({
            let safe = safe.clone();
            move |_, cx| close(&safe, "Dismiss", cx)
        })
        .action(
            DialogAction::new("Cancel", DialogActionRole::Cancel).on_activate({
                let safe = safe.clone();
                move |_, cx| close(&safe, "Cancel", cx)
            }),
        );
    if destructive {
        dialog = dialog.action(
            DialogAction::new("Remove", DialogActionRole::Destructive)
                .on_activate(move |_, cx| close(&danger, "Remove", cx)),
        );
    } else {
        dialog = dialog.action(
            DialogAction::new("Save", DialogActionRole::Default)
                .on_activate(move |_, cx| close(&danger, "Save", cx)),
        );
    }
    dialog
}

fn close(entity: &Entity<DialogDemo>, label: &'static str, cx: &mut App) {
    entity.update(cx, |demo, cx| {
        demo.open = false;
        demo.last = Some(label.into());
        cx.notify();
    });
}
