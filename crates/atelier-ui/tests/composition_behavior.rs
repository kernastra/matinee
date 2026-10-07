//! Headless interaction tests for lists, menus, dialogs, sidebars, and scrolling.

use atelier_ui::{
    Button, ComponentKeymap, ContextMenu, Dialog, DialogAction, DialogActionRole, FocusNext,
    FocusPrevious, List, ListRow, Menu, MenuEntry, MenuItem, MenuSeparator, Popover, ScrollAxis,
    ScrollControl, ScrollView, Sidebar, SidebarItem, SidebarSection, install_component_keybindings,
};
use gpui::{
    Context, IntoElement, KeyBinding, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers,
    ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

fn install_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrevious, None),
    ]);
    install_component_keybindings(
        cx,
        &ComponentKeymap {
            primary: "ctrl",
            word: "ctrl",
            emacs_line_keys: false,
            character_palette: false,
        },
    );
}

fn press(cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
}

fn focus_first(cx: &mut VisualTestContext) {
    cx.update(|window, _| {
        window.focus_next();
    });
    cx.run_until_parked();
}

struct ListHarness {
    selected: Option<usize>,
    activated: Option<usize>,
}

impl Render for ListHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected;
        let view = cx.entity();
        let activate = cx.entity();
        div().size_full().child(
            List::new(
                "list",
                vec![
                    ListRow::new("One"),
                    ListRow::new("Two").disabled(true),
                    ListRow::new("Three"),
                ],
            )
            .selected(selected)
            .on_select(move |index, _, cx| {
                view.update(cx, |this, cx| {
                    this.selected = Some(index);
                    cx.notify();
                });
            })
            .on_activate(move |index, _, cx| {
                activate.update(cx, |this, cx| {
                    this.activated = Some(index);
                    cx.notify();
                });
            }),
        )
    }
}

#[gpui::test]
fn list_keyboard_skips_disabled_and_activates(cx: &mut TestAppContext) {
    cx.update(install_keys);
    let (view, window) = cx.add_window_view(|_, _| ListHarness {
        selected: None,
        activated: None,
    });
    window.run_until_parked();
    focus_first(window);
    press(window, "down");
    window.run_until_parked();
    press(window, "down");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(this.selected, Some(2), "down skips the disabled row");
        });
    });
    press(window, "down");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(this.selected, Some(2), "movement does not wrap");
        });
    });
    press(window, "up");
    window.run_until_parked();
    press(window, "enter");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(this.selected, Some(0));
            assert_eq!(this.activated, Some(0));
        });
    });
}

struct SidebarHarness {
    selected: Option<usize>,
}

impl Render for SidebarHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected;
        let view = cx.entity();
        div().size(px(240.0)).child(
            Sidebar::new(
                "sidebar",
                vec![SidebarSection::new(vec![
                    SidebarItem::new("Home"),
                    SidebarItem::new("Archive").disabled(true),
                    SidebarItem::new("Settings"),
                ])],
            )
            .selected(selected)
            .on_select(move |index, _, cx| {
                view.update(cx, |this, cx| {
                    this.selected = Some(index);
                    cx.notify();
                });
            }),
        )
    }
}

#[gpui::test]
fn sidebar_keyboard_skips_disabled(cx: &mut TestAppContext) {
    cx.update(install_keys);
    let (view, window) = cx.add_window_view(|_, _| SidebarHarness { selected: Some(0) });
    window.run_until_parked();
    focus_first(window);
    press(window, "down");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(this.selected, Some(2));
        });
    });
    press(window, "home");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(this.selected, Some(0));
        });
    });
}

struct MenuHarness {
    dismissed: bool,
    activated: Option<&'static str>,
}

impl Render for MenuHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let dismiss = cx.entity();
        div().child(
            Menu::new("menu")
                .on_dismiss(move |_, cx| {
                    dismiss.update(cx, |this, cx| {
                        this.dismissed = true;
                        cx.notify();
                    });
                })
                .entries(vec![
                    MenuEntry::Item(MenuItem::new("Alpha").on_activate({
                        let view = view.clone();
                        move |_, cx| {
                            view.update(cx, |this, cx| {
                                this.activated = Some("Alpha");
                                cx.notify();
                            });
                        }
                    })),
                    MenuEntry::Item(MenuItem::new("Beta").disabled(true).on_activate({
                        let view = view.clone();
                        move |_, cx| {
                            view.update(cx, |this, cx| {
                                this.activated = Some("Beta");
                                cx.notify();
                            });
                        }
                    })),
                    MenuEntry::Separator(MenuSeparator),
                    MenuEntry::Item(MenuItem::new("Gamma").on_activate(move |_, cx| {
                        view.update(cx, |this, cx| {
                            this.activated = Some("Gamma");
                            cx.notify();
                        });
                    })),
                ]),
        )
    }
}

#[gpui::test]
fn menu_keyboard_skips_disabled_and_escape_dismisses(cx: &mut TestAppContext) {
    cx.update(install_keys);
    let (view, window) = cx.add_window_view(|_, _| MenuHarness {
        dismissed: false,
        activated: None,
    });
    window.run_until_parked();
    focus_first(window);
    press(window, "down");
    window.run_until_parked();
    press(window, "enter");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(this.activated, Some("Gamma"));
        });
    });
    press(window, "escape");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert!(this.dismissed);
        });
    });
}

struct DialogHarness {
    open: bool,
    log: Vec<&'static str>,
}

impl Render for DialogHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.open;
        let outside = cx.entity();
        let cancel = cx.entity();
        let remove = cx.entity();
        div()
            .size_full()
            .child(Button::new("outside", "Outside").on_click(move |_, _, cx| {
                outside.update(cx, |this, cx| {
                    this.log.push("outside");
                    cx.notify();
                });
            }))
            .child(
                Dialog::new("dialog")
                    .open(open)
                    .title("Remove this document?")
                    .content(div())
                    .action(
                        DialogAction::new("Cancel", DialogActionRole::Cancel).on_activate(
                            move |_, cx| {
                                cancel.update(cx, |this, cx| {
                                    this.log.push("cancel");
                                    this.open = false;
                                    cx.notify();
                                });
                            },
                        ),
                    )
                    .action(
                        DialogAction::new("Remove", DialogActionRole::Destructive).on_activate(
                            move |_, cx| {
                                remove.update(cx, |this, cx| {
                                    this.log.push("remove");
                                    this.open = false;
                                    cx.notify();
                                });
                            },
                        ),
                    ),
            )
    }
}

#[gpui::test]
fn dialog_focuses_cancel_traps_tab_and_restores_focus(cx: &mut TestAppContext) {
    cx.update(install_keys);
    let (view, window) = cx.add_window_view(|_, _| DialogHarness {
        open: false,
        log: Vec::new(),
    });
    window.run_until_parked();
    focus_first(window);
    window.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.open = true;
            cx.notify();
        });
    });
    window.run_until_parked();
    press(window, "enter");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(this.log, ["cancel"], "enter reaches cancel, not remove");
            assert!(!this.open);
        });
    });
    press(window, "enter");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(
                this.log.last().copied(),
                Some("outside"),
                "focus returns to the control that was focused before the dialog"
            );
            this.open = true;
            this.log.clear();
            cx.notify();
        });
    });
    window.run_until_parked();
    press(window, "escape");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.log, ["cancel"]);
            this.open = true;
            this.log.clear();
            cx.notify();
        });
    });
    window.run_until_parked();
    press(window, "tab");
    window.run_until_parked();
    press(window, "tab");
    window.run_until_parked();
    press(window, "enter");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(
                this.log.last().copied(),
                Some("cancel"),
                "tab stays inside the dialog"
            );
            assert!(!this.log.contains(&"outside"));
        });
    });
}

struct PopoverHarness {
    open: bool,
    dismissed: bool,
}

impl Render for PopoverHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.open;
        let toggle = cx.entity();
        let dismiss = cx.entity();
        div().size_full().child(
            Popover::new("popover")
                .open(open)
                .on_dismiss(move |_, cx| {
                    dismiss.update(cx, |this, cx| {
                        this.open = false;
                        this.dismissed = true;
                        cx.notify();
                    });
                })
                .trigger(Button::new("trigger", "Open").on_click(move |_, _, cx| {
                    toggle.update(cx, |this, cx| {
                        this.open = !this.open;
                        cx.notify();
                    });
                }))
                .content(div().w(px(120.0)).h(px(40.0)).child("Note")),
        )
    }
}

#[gpui::test]
fn popover_escape_and_outside_click_dismiss_and_restore_focus(cx: &mut TestAppContext) {
    cx.update(install_keys);
    let (view, window) = cx.add_window_view(|_, _| PopoverHarness {
        open: false,
        dismissed: false,
    });
    window.run_until_parked();
    focus_first(window);
    press(window, "enter");
    window.run_until_parked();
    press(window, "escape");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert!(this.dismissed);
            assert!(!this.open);
        });
    });
    press(window, "enter");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert!(this.open, "focus returned to the trigger");
            this.dismissed = false;
            cx.notify();
        });
    });
    window.simulate_click(point(px(400.0), px(400.0)), Modifiers::none());
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert!(this.dismissed, "a press outside closes the popover");
            assert!(!this.open);
        });
    });
}

struct ScrollHarness {
    control: ScrollControl,
    horizontal: bool,
}

impl Render for ScrollHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let child = if self.horizontal {
            div().w(px(800.0)).h(px(20.0))
        } else {
            div().w(px(40.0)).h(px(800.0))
        };
        div().size(px(100.0)).child(
            ScrollView::new("scroll")
                .axis(if self.horizontal {
                    ScrollAxis::Horizontal
                } else {
                    ScrollAxis::Vertical
                })
                .control(self.control.clone())
                .size_full()
                .child(child),
        )
    }
}

#[gpui::test]
fn scroll_view_reports_axis_bounds_and_clamps(cx: &mut TestAppContext) {
    let control = ScrollControl::new();
    let (_, window) = cx.add_window_view({
        let control = control.clone();
        move |_, _| ScrollHarness {
            control,
            horizontal: false,
        }
    });
    window.run_until_parked();
    let max = control.max_offset();
    assert!(
        f32::from(max.height) > 0.0,
        "vertical overflow is scrollable, got {max:?}"
    );
    assert_eq!(
        f32::from(max.width),
        0.0,
        "a vertical view does not scroll sideways"
    );
    control.set_offset(point(px(-50.0), px(-10_000.0)));
    let offset = control.offset();
    assert_eq!(f32::from(offset.x), 0.0);
    assert!(f32::from(offset.y) >= -f32::from(max.height) - 0.5);
    assert!(f32::from(offset.y) < 0.0);

    let wide = ScrollControl::new();
    let (_, window) = cx.add_window_view({
        let control = wide.clone();
        move |_, _| ScrollHarness {
            control,
            horizontal: true,
        }
    });
    window.run_until_parked();
    let max = wide.max_offset();
    assert!(
        f32::from(max.width) > 0.0,
        "horizontal overflow is scrollable"
    );
    assert_eq!(f32::from(max.height), 0.0);
}

#[gpui::test]
fn context_menu_escape_dismisses(cx: &mut TestAppContext) {
    cx.update(install_keys);
    let (view, window) = cx.add_window_view(|_, _| ContextHarness { open: true });
    window.run_until_parked();
    press(window, "escape");
    window.run_until_parked();
    window.update(|_, cx| {
        view.update(cx, |this, _| {
            assert!(!this.open);
        });
    });
}

struct ContextHarness {
    open: bool,
}

impl Render for ContextHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.open;
        let view = cx.entity();
        div().child(
            ContextMenu::new("context")
                .open(open)
                .at(point(px(20.0), px(20.0)))
                .on_dismiss(move |_, cx| {
                    view.update(cx, |this, cx| {
                        this.open = false;
                        cx.notify();
                    });
                })
                .entries(vec![MenuEntry::Item(MenuItem::new("Open"))]),
        )
    }
}

struct LongMenuHarness {
    activated: Option<usize>,
}

impl Render for LongMenuHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        div().child(
            Menu::new("long-menu")
                .max_height(120.0)
                .entries((0..30).map(|index| {
                    let view = view.clone();
                    MenuEntry::Item(MenuItem::new(format!("Row {index}")).on_activate(
                        move |_, cx| {
                            view.update(cx, |this, cx| {
                                this.activated = Some(index);
                                cx.notify();
                            });
                        },
                    ))
                })),
        )
    }
}

#[gpui::test]
fn a_long_menu_scrolls_and_keeps_keyboard_activation(cx: &mut TestAppContext) {
    cx.update(install_keys);
    let (view, window) = cx.add_window_view(|_, _| LongMenuHarness { activated: None });
    window.run_until_parked();
    focus_first(window);
    press(window, "end");
    window.run_until_parked();
    press(window, "enter");
    window.run_until_parked();
    assert_eq!(view.read_with(window, |this, _| this.activated), Some(29));
    press(window, "home");
    window.run_until_parked();
    press(window, "down");
    window.run_until_parked();
    press(window, "enter");
    window.run_until_parked();
    assert_eq!(view.read_with(window, |this, _| this.activated), Some(1));
}
