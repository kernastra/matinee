//! Anchored popover.
//!
//! The trigger is the caller's element. Placement is a preferred side plus
//! start/end/center alignment. GPUI flips the layer when it would leave the
//! window. Outside pointer presses and Escape dismiss it. Focus moves into
//! the popover (or into a menu, when one is the body) and returns to the
//! trigger on close unless focus has already moved on.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnchoredPositionMode, AnimationExt, AnyElement, App, Bounds, ElementId, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, Size, Styled, Window, anchored, canvas, deferred, div,
    point, px, size,
};

use crate::{
    ActiveTheme, StyledExt,
    components::{
        Menu, MenuEntry,
        focus_gate::FocusGate,
        keybindings::{Dismiss, POPOVER_CONTEXT},
    },
    motion,
    overlay::{Alignment, AnchorBox, Placement, anchor_origin, center_offset},
    tokens::{Elevation, MotionDuration, Radius, Space},
};

const POPOVER_PRIORITY: usize = 10;
const GAP: f32 = Space::S1.value();

type DismissHandler = Rc<dyn Fn(&mut Window, &mut App)>;

enum Body {
    Content(AnyElement),
    Menu(Vec<MenuEntry>),
}

struct PopoverState {
    gate: FocusGate,
    anchor: RefCell<Bounds<gpui::Pixels>>,
    measured: RefCell<Size<gpui::Pixels>>,
}

/// A non-modal layer anchored to a trigger.
#[derive(IntoElement)]
pub struct Popover {
    id: ElementId,
    open: bool,
    placement: Placement,
    align: Alignment,
    on_dismiss: Option<DismissHandler>,
    trigger: Option<AnyElement>,
    body: Option<Body>,
}

impl Popover {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: false,
            placement: Placement::Bottom,
            align: Alignment::Start,
            on_dismiss: None,
            trigger: None,
            body: None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    pub fn align(mut self, align: Alignment) -> Self {
        self.align = align;
        self
    }

    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }

    pub fn trigger(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    /// Arbitrary content. The popover takes focus.
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.body = Some(Body::Content(content.into_any_element()));
        self
    }

    /// A menu body. The menu takes focus and handles its own keyboard.
    pub fn menu(mut self, entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        self.body = Some(Body::Menu(entries.into_iter().collect()));
        self
    }
}

impl RenderOnce for Popover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id.clone();
        let model = window.use_keyed_state(id.clone(), cx, |_, cx| PopoverState {
            gate: FocusGate::new(cx.focus_handle().tab_stop(true)),
            anchor: RefCell::new(Bounds::default()),
            measured: RefCell::new(size(px(0.0), px(0.0))),
        });
        let manages = !matches!(self.body, Some(Body::Menu(_)));
        let _ = model.read(cx).gate.focus.clone().tab_stop(manages);
        let mut open = self.open;
        if !model.update(cx, |state, cx| state.gate.sync(open, manages, window, cx)) {
            open = false;
            if let Some(dismiss) = self.on_dismiss.clone() {
                dismiss(window, cx);
            }
        }
        let anchor = *model.read(cx).anchor.borrow();
        let measured = *model.read(cx).measured.borrow();
        let origin = anchor_origin(
            AnchorBox {
                x: f32::from(anchor.origin.x),
                y: f32::from(anchor.origin.y),
                width: f32::from(anchor.size.width),
                height: f32::from(anchor.size.height),
            },
            self.placement,
            self.align,
            GAP,
        );
        let (shift_x, shift_y) = center_offset(
            self.align,
            self.placement,
            f32::from(measured.width),
            f32::from(measured.height),
        );
        let theme = cx.theme().clone();
        let dismiss = self.on_dismiss.clone();
        let anchor_box = anchor;

        let layer = open.then(|| {
            let dismiss_key = dismiss.clone();
            let dismiss_out = dismiss.clone();
            let model_size = model.clone();
            let body = match self.body {
                Some(Body::Menu(entries)) => {
                    let mut menu = Menu::new((id.clone(), "menu"))
                        .entries(entries)
                        .gate(&model.read(cx).gate);
                    if let Some(dismiss) = dismiss.clone() {
                        menu = menu.on_dismiss(move |window, cx| dismiss(window, cx));
                    }
                    menu.into_any_element()
                }
                Some(Body::Content(content)) => content,
                None => div().into_any_element(),
            };
            let mut panel = div()
                .key_context(POPOVER_CONTEXT)
                .when(manages, |this| this.track_focus(&model.read(cx).gate.focus))
                .min_w(px(160.0))
                .when(manages, |this| {
                    this.p(Space::S3.px())
                        .bg(theme.colors.surface.elevated)
                        .border_1()
                        .border_color(theme.colors.border.default)
                        .corner_radius(&theme, Radius::Medium)
                        .elevation(&theme, Elevation::Overlay)
                })
                .child(
                    canvas(
                        {
                            let model_size = model_size.clone();
                            move |bounds, _, cx| {
                                *model_size.read(cx).measured.borrow_mut() = bounds.size;
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(body)
                .on_mouse_down_out(move |event, window, cx| {
                    if anchor_box.contains(&event.position) {
                        return;
                    }
                    if let Some(dismiss) = dismiss_out.clone() {
                        dismiss(window, cx);
                    }
                });
            if manages && let Some(dismiss_key) = dismiss_key {
                panel = panel.on_action(move |_: &Dismiss, window, cx| dismiss_key(window, cx));
            }
            let panel = reveal(id.clone(), panel, cx);
            deferred(
                anchored()
                    .anchor(origin.corner)
                    .position(point(px(origin.x + shift_x), px(origin.y + shift_y)))
                    .position_mode(AnchoredPositionMode::Window)
                    .snap_to_window_with_margin(px(8.0))
                    .child(panel),
            )
            .with_priority(POPOVER_PRIORITY)
            .into_any_element()
        });

        div()
            .id(id)
            .relative()
            .child(
                canvas(
                    {
                        let model = model.clone();
                        move |bounds, _, cx| {
                            *model.read(cx).anchor.borrow_mut() = bounds;
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(self.trigger)
            .children(layer)
    }
}

fn reveal(id: ElementId, panel: gpui::Div, cx: &App) -> AnyElement {
    if let Some(animation) = motion::timed(cx, MotionDuration::Fast) {
        panel
            .with_animation((id, "reveal"), animation, |panel, delta| {
                panel.opacity(delta)
            })
            .into_any_element()
    } else {
        panel.into_any_element()
    }
}
