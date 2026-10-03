//! Two-pane split.
//!
//! The leading pane has a width. The trailing pane takes the rest. A divider
//! between them drags that width. An inspector would be another sized pane
//! after the flexible region; this view does not build it.
//!
//! The drag capture covers the window while the pointer is down, so the
//! resize continues if the pointer leaves the divider.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnchoredPositionMode, AnyElement, App, CursorStyle, ElementId, InteractiveElement, IntoElement,
    MouseButton, ParentElement, RenderOnce, Styled, Window, anchored, deferred, div, point, px,
};

use crate::{ActiveTheme, tokens::Space};

const DEFAULT_WIDTH: f32 = 220.0;
const DEFAULT_MIN: f32 = 160.0;
const DEFAULT_MAX: f32 = 420.0;
const DRAG_PRIORITY: usize = 5;

type ResizeHandler = Rc<dyn Fn(f32, &mut Window, &mut App)>;

struct DragState {
    active: bool,
    origin_x: f32,
    origin_width: f32,
}

/// Sidebar | content. Width is controlled by the caller.
#[derive(IntoElement)]
pub struct SplitView {
    id: ElementId,
    leading: AnyElement,
    trailing: AnyElement,
    leading_width: f32,
    min_width: f32,
    max_width: f32,
    on_resize: Option<ResizeHandler>,
}

impl SplitView {
    pub fn new(
        id: impl Into<ElementId>,
        leading: impl IntoElement,
        trailing: impl IntoElement,
    ) -> Self {
        Self {
            id: id.into(),
            leading: leading.into_any_element(),
            trailing: trailing.into_any_element(),
            leading_width: DEFAULT_WIDTH,
            min_width: DEFAULT_MIN,
            max_width: DEFAULT_MAX,
            on_resize: None,
        }
    }

    pub fn leading_width(mut self, width: f32) -> Self {
        self.leading_width = width;
        self
    }

    pub fn limits(mut self, min_width: f32, max_width: f32) -> Self {
        self.min_width = min_width;
        self.max_width = max_width.max(min_width);
        self
    }

    pub fn on_resize(mut self, handler: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_resize = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for SplitView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let id = self.id.clone();
        let model = window.use_keyed_state(id.clone(), cx, |_, _| {
            RefCell::new(DragState {
                active: false,
                origin_x: 0.0,
                origin_width: DEFAULT_WIDTH,
            })
        });
        let width = clamp_width(self.leading_width, self.min_width, self.max_width);
        let min_width = self.min_width;
        let max_width = self.max_width;
        let dragging = model.read(cx).borrow().active;
        let on_resize = self.on_resize.clone();

        let divider = div()
            .id((id.clone(), "divider"))
            .w(Space::S2.px())
            .h_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .child(div().w(px(1.0)).h_full().bg(theme.colors.border.subtle))
            .on_mouse_down(MouseButton::Left, {
                let model = model.clone();
                move |event, _, cx| {
                    cx.stop_propagation();
                    *model.read(cx).borrow_mut() = DragState {
                        active: true,
                        origin_x: f32::from(event.position.x),
                        origin_width: width,
                    };
                }
            });

        let capture = dragging.then(|| {
            let viewport = window.viewport_size();
            let model = model.clone();
            let on_resize = on_resize.clone();
            deferred(
                anchored()
                    .anchor(gpui::Corner::TopLeft)
                    .position(point(px(0.0), px(0.0)))
                    .position_mode(AnchoredPositionMode::Window)
                    .child(
                        div()
                            .occlude()
                            .w(viewport.width)
                            .h(viewport.height)
                            .cursor(CursorStyle::ResizeLeftRight)
                            .on_mouse_move({
                                let model = model.clone();
                                let on_resize = on_resize.clone();
                                move |event, window, cx| {
                                    resize_from(
                                        &model,
                                        on_resize.clone(),
                                        min_width,
                                        max_width,
                                        f32::from(event.position.x),
                                        window,
                                        cx,
                                    );
                                }
                            })
                            .on_mouse_up(MouseButton::Left, {
                                let model = model.clone();
                                move |_, _, cx| {
                                    model.read(cx).borrow_mut().active = false;
                                }
                            }),
                    ),
            )
            .with_priority(DRAG_PRIORITY)
        });

        div()
            .id(id)
            .w_full()
            .h_full()
            .flex()
            .flex_row()
            .overflow_hidden()
            .on_mouse_move({
                let model = model.clone();
                let on_resize = on_resize.clone();
                move |event, window, cx| {
                    if !model.read(cx).borrow().active {
                        return;
                    }
                    resize_from(
                        &model,
                        on_resize.clone(),
                        min_width,
                        max_width,
                        f32::from(event.position.x),
                        window,
                        cx,
                    );
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let model = model.clone();
                move |_, _, cx| {
                    model.read(cx).borrow_mut().active = false;
                }
            })
            .on_mouse_up_out(MouseButton::Left, {
                let model = model.clone();
                move |_, _, cx| {
                    model.read(cx).borrow_mut().active = false;
                }
            })
            .child(
                div()
                    .w(px(width))
                    .h_full()
                    .flex_none()
                    .overflow_hidden()
                    .child(self.leading),
            )
            .child(divider)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .overflow_hidden()
                    .child(self.trailing),
            )
            .children(capture)
    }
}

fn clamp_width(width: f32, min_width: f32, max_width: f32) -> f32 {
    if !width.is_finite() {
        return min_width;
    }
    width.clamp(min_width, max_width)
}

fn resize_from(
    model: &gpui::Entity<RefCell<DragState>>,
    on_resize: Option<ResizeHandler>,
    min_width: f32,
    max_width: f32,
    x: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let drag = model.read(cx).borrow();
    if !drag.active {
        return;
    }
    let next = clamp_width(drag.origin_width + x - drag.origin_x, min_width, max_width);
    drop(drag);
    if let Some(on_resize) = on_resize {
        on_resize(next, window, cx);
    }
}
