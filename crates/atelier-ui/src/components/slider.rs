//! Generic slider.
//!
//! A value control only. Timeline-specific behavior belongs in the
//! application. Component metrics: track height 4, thumb 16,
//! hit target height 28. `step <= 0` uses one hundredth of the span for both
//! pointer and keyboard snapping. Pointer drags do not take keyboard focus.
//!
//! Thumb position is `left` as a percentage of the track. The last painted
//! bounds of a probe element map pointer position to a value; GPUI has no
//! layout query outside paint.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, Bounds, Element, ElementId, FocusHandle, GlobalElementId, InspectorElementId,
    InteractiveElement, IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, RenderOnce, Style, Styled, Window, div, px, relative,
};

use crate::{
    ActiveTheme,
    components::{
        FocusRing,
        keybindings::{
            NudgeDown, NudgeLeft, NudgePageDown, NudgePageUp, NudgeRight, NudgeToEnd, NudgeToStart,
            NudgeUp, SLIDER_CONTEXT,
        },
    },
    focus::{self, focus_visible},
    inspect::{self, Inspection},
    tokens::Color,
};

const TRACK_H: f32 = 4.0;
const THUMB: f32 = 16.0;
const HIT_H: f32 = 28.0;
const WIDTH: f32 = 220.0;
const BORDER: f32 = 1.0;
const PAGE: i32 = 10;

type ChangeHandler = Rc<dyn Fn(f32, &mut Window, &mut App)>;

/// Step used for both pointer and keyboard. Non-positive steps become
/// one hundredth of the span.
pub fn effective_step(min: f32, max: f32, step: f32) -> f32 {
    if step > 0.0 {
        step
    } else {
        ((max - min) / 100.0).abs()
    }
}

pub fn snap_to_step(value: f32, min: f32, max: f32, step: f32) -> f32 {
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    let step = effective_step(min, max, step);
    if step == 0.0 {
        return value.clamp(min, max);
    }
    let steps = ((value.clamp(min, max) - min) / step).round();
    (min + steps * step).clamp(min, max)
}

pub fn slider_ratio(value: f32, min: f32, max: f32) -> f32 {
    let span = max - min;
    if span.abs() < f32::EPSILON {
        0.0
    } else {
        ((value - min) / span).clamp(0.0, 1.0)
    }
}

pub fn value_from_ratio(ratio: f32, min: f32, max: f32, step: f32) -> f32 {
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    snap_to_step(min + (max - min) * ratio.clamp(0.0, 1.0), min, max, step)
}

pub fn nudge(value: f32, min: f32, max: f32, step: f32, steps: i32) -> f32 {
    let delta = effective_step(min, max, step) * steps as f32;
    snap_to_step(value + delta, min, max, step)
}

/// A horizontal slider.
#[derive(IntoElement)]
pub struct Slider {
    id: ElementId,
    min: f32,
    max: f32,
    value: f32,
    step: f32,
    disabled: bool,
    on_change: Option<ChangeHandler>,
    inspected: bool,
}

impl Slider {
    pub fn new(id: impl Into<ElementId>, value: f32) -> Self {
        Self {
            id: id.into(),
            min: 0.0,
            max: 1.0,
            value,
            step: 0.0,
            disabled: false,
            on_change: None,
            inspected: false,
        }
    }

    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(mut self, handler: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    pub fn inspected(mut self, inspected: bool) -> Self {
        self.inspected = inspected;
        self
    }
}

struct SliderState {
    focus: FocusHandle,
    bounds: RefCell<Option<Bounds<Pixels>>>,
    dragging: bool,
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let model = window.use_keyed_state(self.id.clone(), cx, |_, cx| SliderState {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            bounds: RefCell::new(None),
            dragging: false,
        });
        let focus = model.read(cx).focus.clone();
        let dragging = model.read(cx).dragging;
        let focused = !self.disabled && focus.is_focused(window);
        let value = snap_to_step(self.value, self.min, self.max, self.step);
        let ratio = slider_ratio(value, self.min, self.max);
        if self.inspected {
            inspect::report_inspection(Inspection {
                name: "Slider",
                focused,
                value: format!("{value:.2}"),
            });
        }
        let colors = &theme.colors;
        let enabled = !self.disabled;
        let on_change = self.on_change.clone();
        let min = self.min;
        let max = self.max;
        let step = self.step;

        let mut control = div()
            .id(self.id.clone())
            .key_context(SLIDER_CONTEXT)
            .relative()
            .w(px(WIDTH))
            .h(px(HIT_H))
            .when(enabled, |this| this.track_focus(&focus).cursor_pointer())
            .child(BoundsProbe {
                slot: model.clone(),
            })
            .child(track_visual(
                colors.border.strong,
                colors.control.accent,
                ratio,
            ))
            .when(focus_visible(focused, cx), |this| {
                this.child(FocusRing::new(THUMB / 2.0, 0.0))
            });

        if enabled {
            let model_down = model.clone();
            let change_down = on_change.clone();
            control = control.on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    focus::note_pointer_interaction(cx);
                    model_down.update(cx, |state, cx| {
                        state.dragging = true;
                        cx.notify();
                    });
                    apply_pointer(
                        &model_down,
                        event.position,
                        PointerMap { min, max, step },
                        change_down.clone(),
                        window,
                        cx,
                    );
                },
            );
            if dragging {
                let model_move = model.clone();
                let model_up = model.clone();
                let change_move = on_change.clone();
                control = control
                    .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                        apply_pointer(
                            &model_move,
                            event.position,
                            PointerMap { min, max, step },
                            change_move.clone(),
                            window,
                            cx,
                        );
                    })
                    .on_mouse_up(MouseButton::Left, {
                        let model = model_up.clone();
                        move |_: &MouseUpEvent, _, cx| {
                            model.update(cx, |state, cx| {
                                state.dragging = false;
                                cx.notify();
                            });
                        }
                    })
                    .on_mouse_up_out(MouseButton::Left, {
                        let model = model_up;
                        move |_, _, cx| {
                            model.update(cx, |state, cx| {
                                state.dragging = false;
                                cx.notify();
                            });
                        }
                    });
            }
            let change = on_change.clone();
            control = control
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeLeft, window, cx| {
                        emit(nudge(value, min, max, step, -1), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeDown, window, cx| {
                        emit(nudge(value, min, max, step, -1), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeRight, window, cx| {
                        emit(nudge(value, min, max, step, 1), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeUp, window, cx| {
                        emit(nudge(value, min, max, step, 1), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgePageDown, window, cx| {
                        emit(
                            nudge(value, min, max, step, -PAGE),
                            change.clone(),
                            window,
                            cx,
                        )
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgePageUp, window, cx| {
                        emit(
                            nudge(value, min, max, step, PAGE),
                            change.clone(),
                            window,
                            cx,
                        )
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeToStart, window, cx| {
                        emit(
                            snap_to_step(min, min, max, step),
                            change.clone(),
                            window,
                            cx,
                        )
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeToEnd, window, cx| {
                        emit(
                            snap_to_step(max, min, max, step),
                            change.clone(),
                            window,
                            cx,
                        )
                    }
                });
        }

        control
    }
}

fn emit(value: f32, on_change: Option<ChangeHandler>, window: &mut Window, cx: &mut App) {
    if let Some(on_change) = on_change {
        on_change(value, window, cx);
    }
}

struct PointerMap {
    min: f32,
    max: f32,
    step: f32,
}

fn apply_pointer(
    model: &gpui::Entity<SliderState>,
    position: gpui::Point<Pixels>,
    map: PointerMap,
    on_change: Option<ChangeHandler>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(bounds) = *model.read(cx).bounds.borrow() else {
        return;
    };
    let width = bounds.size.width;
    if width <= px(0.0) {
        return;
    }
    let ratio = ((position.x - bounds.left()) / width).clamp(0.0, 1.0);
    emit(
        value_from_ratio(ratio, map.min, map.max, map.step),
        on_change,
        window,
        cx,
    );
}

fn track_visual(track: Color, fill: Color, ratio: f32) -> impl IntoElement {
    div()
        .absolute()
        .top(px((HIT_H - TRACK_H) / 2.0))
        .left(px(THUMB / 2.0))
        .right(px(THUMB / 2.0))
        .h(px(TRACK_H))
        .rounded(px(TRACK_H / 2.0))
        .bg(track)
        .child(
            div()
                .h_full()
                .w(relative(ratio))
                .rounded(px(TRACK_H / 2.0))
                .bg(fill),
        )
        .child(
            div()
                .absolute()
                .top(px((TRACK_H - THUMB) / 2.0))
                .left(relative(ratio))
                .ml(px(-THUMB / 2.0))
                .size(px(THUMB))
                .rounded(px(THUMB / 2.0))
                .bg(fill)
                .border(px(BORDER))
                .border_color(fill),
        )
}

struct BoundsProbe {
    slot: gpui::Entity<SliderState>,
}

impl IntoElement for BoundsProbe {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BoundsProbe {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        *self.slot.read(cx).bounds.borrow_mut() = Some(bounds);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_and_keyboard_stay_inside_the_range() {
        assert!((snap_to_step(0.26, 0.0, 1.0, 0.25) - 0.25).abs() < 1e-5);
        assert!((nudge(1.0, 0.0, 1.0, 0.25, 1) - 1.0).abs() < 1e-5);
        assert!((nudge(0.0, 0.0, 1.0, 0.25, -1) - 0.0).abs() < 1e-5);
        assert!((nudge(0.0, 0.0, 1.0, 0.0, 1) - 0.01).abs() < 1e-5);
        assert!((value_from_ratio(1.2, 0.0, 10.0, 1.0) - 10.0).abs() < 1e-5);
        assert!((slider_ratio(5.0, 0.0, 10.0) - 0.5).abs() < 1e-5);
    }
}
