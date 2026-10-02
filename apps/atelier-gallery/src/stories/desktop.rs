//! A generic documents window built only from Atelier components.

use atelier_ui::gpui::{Point, point, px};
use atelier_ui::prelude::*;

struct Doc {
    title: &'static str,
    detail: &'static str,
    favorite: bool,
    downloaded: bool,
}

const DOCS: &[Doc] = &[
    Doc {
        title: "Field notes",
        detail: "Updated today",
        favorite: true,
        downloaded: false,
    },
    Doc {
        title: "Quarterly plan",
        detail: "Shared with the team",
        favorite: true,
        downloaded: true,
    },
    Doc {
        title: "Inventory",
        detail: "Counted on Monday",
        favorite: false,
        downloaded: false,
    },
    Doc {
        title: "Route map",
        detail: "Local copy",
        favorite: false,
        downloaded: true,
    },
    Doc {
        title: "Meeting notes",
        detail: "Yesterday",
        favorite: true,
        downloaded: false,
    },
    Doc {
        title: "Packing list",
        detail: "Draft",
        favorite: false,
        downloaded: true,
    },
    Doc {
        title: "Budget draft",
        detail: "Needs review",
        favorite: false,
        downloaded: false,
    },
    Doc {
        title: "Site survey",
        detail: "Photos attached",
        favorite: true,
        downloaded: true,
    },
];

#[derive(Clone)]
struct Desk {
    section: usize,
    query: SharedString,
    selected: Option<usize>,
    overflow: bool,
    info: bool,
    confirm: bool,
    context: bool,
    context_at: Point<Pixels>,
    hidden: Vec<bool>,
    split: f32,
    details: bool,
}

impl Default for Desk {
    fn default() -> Self {
        Self {
            section: 0,
            query: SharedString::default(),
            selected: Some(0),
            overflow: false,
            info: false,
            confirm: false,
            context: false,
            context_at: point(px(0.0), px(0.0)),
            hidden: vec![false; DOCS.len()],
            split: 200.0,
            details: true,
        }
    }
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let model = window.use_keyed_state("desktop-composition", cx, |_, _| Desk::default());
    let desk = model.read(cx).clone();
    let theme = cx.theme().clone();
    let rows = visible(&desk);
    let selected = desk
        .selected
        .filter(|index| rows.iter().any(|(source, _)| *source == *index));
    let scroll = window.use_keyed_state("desktop-scroll", cx, |_, _| ScrollControl::new());
    let scroll = scroll.read(cx).clone();

    let shell = div()
        .h(px(560.0))
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme.colors.surface.canvas)
        .border_1()
        .border_color(theme.colors.border.default)
        .corner_radius(&theme, Radius::Large)
        .child(toolbar(&desk, &model))
        .child(
            div().flex_1().min_h(px(0.0)).child(
                SplitView::new(
                    "desktop-split",
                    sidebar(&desk, &model),
                    content(&desk, &rows, selected, scroll, &model, &theme),
                )
                .leading_width(desk.split)
                .on_resize({
                    let model = model.clone();
                    move |next, _, cx| {
                        model.update(cx, |desk, cx| {
                            desk.split = next;
                            cx.notify();
                        });
                    }
                }),
            ),
        );

    div()
        .relative()
        .child(shell)
        .child(menus(&desk, selected, &model))
        .into_any_element()
}

fn toolbar(desk: &Desk, model: &Entity<Desk>) -> impl IntoElement {
    let query = desk.query.clone();
    let edit = model.clone();
    let toggle = model.clone();
    let info = model.clone();
    let info_open = desk.info;
    Toolbar::new()
        .leading(Text::new("Documents").role(TextRole::Subheading))
        .trailing(
            SearchField::new("desktop-search", query)
                .placeholder("Search")
                .on_change(move |value, _, cx| {
                    edit.update(cx, |desk, cx| {
                        desk.query = value;
                        cx.notify();
                    });
                }),
        )
        .trailing(
            Popover::new("desktop-info")
                .open(info_open)
                .placement(Placement::Bottom)
                .align(Alignment::End)
                .on_dismiss({
                    let info = info.clone();
                    move |_, cx| {
                        info.update(cx, |desk, cx| {
                            desk.info = false;
                            cx.notify();
                        });
                    }
                })
                .trigger(
                    IconButton::new("desktop-info-button", IconName::Info, "About this list")
                        .on_click(move |_, _, cx| {
                            info.update(cx, |desk, cx| {
                                desk.info = !desk.info;
                                cx.notify();
                            });
                        }),
                )
                .content(
                    v_stack(Space::S1)
                        .w(px(200.0))
                        .child(Text::new("Recently added").role(TextRole::Label))
                        .child(
                            Text::new("Newest documents in the selected section.")
                                .role(TextRole::Caption)
                                .tone(TextTone::Secondary),
                        ),
                ),
        )
        .trailing(
            Popover::new("desktop-overflow")
                .open(desk.overflow)
                .placement(Placement::Bottom)
                .align(Alignment::End)
                .on_dismiss({
                    let toggle = toggle.clone();
                    move |_, cx| {
                        toggle.update(cx, |desk, cx| {
                            desk.overflow = false;
                            cx.notify();
                        });
                    }
                })
                .trigger(
                    IconButton::new("desktop-more", IconName::More, "More").on_click(
                        move |_, _, cx| {
                            toggle.update(cx, |desk, cx| {
                                desk.overflow = !desk.overflow;
                                cx.notify();
                            });
                        },
                    ),
                )
                .menu(overflow_entries(model)),
        )
}

fn sidebar(desk: &Desk, model: &Entity<Desk>) -> impl IntoElement {
    let selected = Some(desk.section);
    let choose = model.clone();
    div().h_full().p(Space::S2.px()).child(
        Sidebar::new(
            "desktop-sidebar",
            vec![
                SidebarSection::new(vec![
                    SidebarItem::new("Home").icon(IconName::Home),
                    SidebarItem::new("Favorites").icon(IconName::Heart),
                    SidebarItem::new("Downloads").icon(IconName::Download),
                ])
                .label("Library"),
                SidebarSection::new(vec![SidebarItem::new("Settings").icon(IconName::Sliders)])
                    .label("Settings"),
            ],
        )
        .selected(selected)
        .on_select(move |index, _, cx| {
            choose.update(cx, |desk, cx| {
                desk.section = index;
                desk.selected = None;
                cx.notify();
            });
        }),
    )
}

fn content(
    desk: &Desk,
    rows: &[(usize, &Doc)],
    selected: Option<usize>,
    scroll: ScrollControl,
    model: &Entity<Desk>,
    theme: &Theme,
) -> impl IntoElement {
    let body = if desk.section == 3 {
        settings(desk, model).into_any_element()
    } else if rows.is_empty() {
        let clear = model.clone();
        EmptyState::new("No matches")
            .icon(IconName::Search)
            .description("Nothing in this section matches the search.")
            .primary_action(Button::new("desktop-clear", "Clear search").on_click(
                move |_, _, cx| {
                    clear.update(cx, |desk, cx| {
                        desk.query = SharedString::default();
                        cx.notify();
                    });
                },
            ))
            .into_any_element()
    } else {
        let select = model.clone();
        let context = model.clone();
        let list_rows = rows
            .iter()
            .map(|(index, doc)| {
                let mut row = ListRow::new(doc.title).leading(
                    Image::sample(("desktop-thumb", *index), *index)
                        .frame(44.0, 32.0)
                        .fit(ImageFit::Fill)
                        .label(doc.title),
                );
                if desk.details {
                    row = row.secondary(doc.detail);
                }
                row
            })
            .collect();
        let list_selected =
            selected.and_then(|source| rows.iter().position(|(index, _)| *index == source));
        ScrollView::vertical("desktop-list-scroll")
            .control(scroll.clone())
            .flex_1()
            .min_h(px(0.0))
            .child(
                List::new("desktop-list", list_rows)
                    .selected(list_selected)
                    .scroll_control(scroll)
                    .on_select(move |index, _, cx| {
                        select.update(cx, |desk, cx| {
                            desk.selected = visible(desk).get(index).map(|(source, _)| *source);
                            cx.notify();
                        });
                    })
                    .on_context_menu(move |index, position, _, cx| {
                        context.update(cx, |desk, cx| {
                            desk.selected = visible(desk).get(index).map(|(source, _)| *source);
                            desk.context = true;
                            desk.context_at = position;
                            desk.overflow = false;
                            cx.notify();
                        });
                    }),
            )
            .into_any_element()
    };

    div()
        .h_full()
        .flex()
        .flex_col()
        .child(
            div()
                .px(Space::S4.px())
                .py(Space::S3.px())
                .child(Text::new(section_title(desk.section)).role(TextRole::Heading)),
        )
        .child(div().flex_1().min_h(px(0.0)).px(Space::S3.px()).child(body))
        .child(
            div()
                .px(Space::S4.px())
                .py(Space::S2.px())
                .border_t_1()
                .border_color(theme.colors.border.subtle)
                .flex()
                .flex_row()
                .items_center()
                .gap(Space::S3.px())
                .child(
                    Text::new(format!("{} shown", rows.len()))
                        .role(TextRole::Caption)
                        .tone(TextTone::Muted),
                )
                .child(
                    div()
                        .w(px(120.0))
                        .child(ProgressBar::determinate(0.64).label("Indexed")),
                )
                .child(
                    Text::new("Indexed 64%")
                        .role(TextRole::Caption)
                        .tone(TextTone::Muted),
                ),
        )
}

fn settings(desk: &Desk, model: &Entity<Desk>) -> impl IntoElement {
    let on = desk.details;
    let toggle = model.clone();
    div().p(Space::S4.px()).child(
        h_stack(Space::S3)
            .child(Text::new("Show details").role(TextRole::Label))
            .child(
                Switch::new("desktop-details", on).on_change(move |on, _, cx| {
                    toggle.update(cx, |desk, cx| {
                        desk.details = on;
                        cx.notify();
                    });
                }),
            ),
    )
}

fn overflow_entries(model: &Entity<Desk>) -> Vec<MenuEntry> {
    let dismiss = model.clone();
    let remove = model.clone();
    vec![
        MenuEntry::Item(MenuItem::new("New note").icon(IconName::Plus).on_activate({
            let dismiss = dismiss.clone();
            move |_, cx| {
                dismiss.update(cx, |desk, cx| {
                    desk.overflow = false;
                    cx.notify();
                });
            }
        })),
        MenuEntry::Separator(MenuSeparator),
        MenuEntry::Item(
            MenuItem::new("Remove")
                .destructive(true)
                .on_activate(move |_, cx| {
                    remove.update(cx, |desk, cx| {
                        let chosen = desk.selected.filter(|index| {
                            visible(desk).iter().any(|(source, _)| source == index)
                        });
                        desk.overflow = false;
                        desk.confirm = chosen.is_some();
                        cx.notify();
                    });
                }),
        ),
    ]
}

fn menus(desk: &Desk, selected: Option<usize>, model: &Entity<Desk>) -> impl IntoElement {
    let context = model.clone();
    let confirm = model.clone();
    div()
        .child(
            ContextMenu::new("desktop-context")
                .open(desk.context)
                .at(desk.context_at)
                .on_dismiss({
                    let context = context.clone();
                    move |_, cx| {
                        context.update(cx, |desk, cx| {
                            desk.context = false;
                            cx.notify();
                        });
                    }
                })
                .entries(vec![
                    MenuEntry::Item(MenuItem::new("Open").on_activate({
                        let context = context.clone();
                        move |_, cx| {
                            context.update(cx, |desk, cx| {
                                desk.context = false;
                                cx.notify();
                            });
                        }
                    })),
                    MenuEntry::Separator(MenuSeparator),
                    MenuEntry::Item(MenuItem::new("Remove").destructive(true).on_activate({
                        let confirm = confirm.clone();
                        move |_, cx| {
                            confirm.update(cx, |desk, cx| {
                                desk.context = false;
                                desk.confirm = true;
                                cx.notify();
                            });
                        }
                    })),
                ]),
        )
        .child(remove_dialog(desk.confirm, selected, &confirm))
}

fn remove_dialog(open: bool, selected: Option<usize>, model: &Entity<Desk>) -> Dialog {
    let cancel = model.clone();
    let remove = model.clone();
    Dialog::new("desktop-remove")
        .open(open)
        .title("Remove this document?")
        .content(Text::new("This cannot be undone."))
        .on_dismiss({
            let cancel = cancel.clone();
            move |_, cx| {
                cancel.update(cx, |desk, cx| {
                    desk.confirm = false;
                    cx.notify();
                });
            }
        })
        .action(
            DialogAction::new("Cancel", DialogActionRole::Cancel).on_activate(move |_, cx| {
                cancel.update(cx, |desk, cx| {
                    desk.confirm = false;
                    cx.notify();
                });
            }),
        )
        .action(
            DialogAction::new("Remove", DialogActionRole::Destructive).on_activate(move |_, cx| {
                remove.update(cx, |desk, cx| {
                    if let Some(index) = selected
                        && let Some(flag) = desk.hidden.get_mut(index)
                    {
                        *flag = true;
                    }
                    desk.confirm = false;
                    desk.selected = None;
                    cx.notify();
                });
            }),
        )
}

fn visible(desk: &Desk) -> Vec<(usize, &'static Doc)> {
    let query = desk.query.to_lowercase();
    DOCS.iter()
        .enumerate()
        .filter(|(index, doc)| {
            if desk.hidden.get(*index).copied().unwrap_or(false) {
                return false;
            }
            let in_section = match desk.section {
                0 => true,
                1 => doc.favorite,
                2 => doc.downloaded,
                _ => false,
            };
            if !in_section {
                return false;
            }
            query.is_empty() || doc.title.to_lowercase().contains(&query)
        })
        .collect()
}

fn section_title(section: usize) -> &'static str {
    match section {
        0 => "Recently added",
        1 => "Favorites",
        2 => "Downloads",
        _ => "Settings",
    }
}
