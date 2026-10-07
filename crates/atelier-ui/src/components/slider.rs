//! Generic slider.
//!
//! A value control only. Timeline-specific behavior belongs in the
//! application. Component metrics: track height 4, thumb 16,
//! hit target height 28. Pointer drags do not take keyboard focus.
//!
//! The range is conventional and ascending: `min` and `max` are finite and
//! `min <= max`. Reversed ranges are not supported. A non-finite endpoint or
//! `min > max` is unusable and collapses to the value `0` for rendering,
//! pointer input, and keyboard input. A zero-span range (`min == max`) is
//! that single value. A non-positive or non-finite step on a positive span
//! becomes one hundredth of the span. The same [`SliderRange`] is used for
//! the thumb, pointer mapping, and keyboard nudges.
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

/// Normalized ascending range shared by the thumb, pointer, and keyboard.
#[derive(Clone, Copy, Debug)]
struct SliderRange {
    min: f32,
    max: f32,
    step: f32,
}

impl SliderRange {
    /// `min` and `max` must be finite and `min <= max`. Otherwise the range
    /// collapses to `0`. A non-positive or non-finite `step` on a positive
    /// span becomes `span / 100`.
    fn conventional(min: f32, max: f32, step: f32) -> Self {
        if !(min.is_finite() && max.is_finite() && min <= max) {
            return Self {
                min: 0.0,
                max: 0.0,
                step: 0.0,
            };
        }
        let step = if max == min {
            0.0
        } else if step.is_finite() && step > 0.0 {
            step
        } else {
            let fallback = (max - min) / 100.0;
            if fallback.is_finite() && fallback > 0.0 {
                fallback
            } else {
                0.0
            }
        };
        Self { min, max, step }
    }

    fn resolve(self, value: f32) -> f32 {
        // NaN has no position. Infinities clamp to the corresponding end.
        let value = if value.is_nan() { self.min } else { value };
        let clamped = value.clamp(self.min, self.max);
        if self.step <= 0.0 {
            return clamped;
        }
        let steps = ((clamped - self.min) / self.step).round();
        if !steps.is_finite() {
            return clamped;
        }
        let snapped = self.min + steps * self.step;
        if snapped.is_finite() {
            snapped.clamp(self.min, self.max)
        } else {
            clamped
        }
    }

    /// Position of `value` along the track, after the value is resolved.
    /// Zero-span and unusable ranges sit at 0. This does not apply a second snap.
    fn position(self, value: f32) -> f32 {
        let span = self.max - self.min;
        if span <= 0.0 {
            return 0.0;
        }
        let resolved = self.resolve(value);
        ((resolved - self.min) / span).clamp(0.0, 1.0)
    }

    fn at_ratio(self, ratio: f32) -> f32 {
        let ratio = if ratio.is_finite() {
            ratio.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.resolve(self.min + (self.max - self.min) * ratio)
    }

    fn nudge(self, value: f32, steps: i32) -> f32 {
        let base = self.resolve(value);
        let delta = self.step * steps as f32;
        if !delta.is_finite() {
            return base;
        }
        self.resolve(base + delta)
    }
}

/// Step used for both pointer and keyboard, after the range is normalized.
pub fn effective_step(min: f32, max: f32, step: f32) -> f32 {
    SliderRange::conventional(min, max, step).step
}

pub fn snap_to_step(value: f32, min: f32, max: f32, step: f32) -> f32 {
    SliderRange::conventional(min, max, step).resolve(value)
}

pub fn slider_ratio(value: f32, min: f32, max: f32) -> f32 {
    SliderRange::conventional(min, max, 0.0).position(value)
}

pub fn value_from_ratio(ratio: f32, min: f32, max: f32, step: f32) -> f32 {
    SliderRange::conventional(min, max, step).at_ratio(ratio)
}

pub fn nudge(value: f32, min: f32, max: f32, step: f32, steps: i32) -> f32 {
    SliderRange::conventional(min, max, step).nudge(value, steps)
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
    /// Stretch to the parent width. The default is a fixed track.
    fill: bool,
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
            fill: false,
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

    /// Use the parent's width instead of the default track width.
    pub fn fill(mut self, fill: bool) -> Self {
        self.fill = fill;
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
        let range = SliderRange::conventional(self.min, self.max, self.step);
        let value = range.resolve(self.value);
        let ratio = range.position(value);
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

        let mut control = div()
            .id(self.id.clone())
            .key_context(SLIDER_CONTEXT)
            .relative()
            .when(self.fill, |this| this.w_full())
            .when(!self.fill, |this| this.w(px(WIDTH)))
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
                        PointerMap { range },
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
                            PointerMap { range },
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
                        emit(range.nudge(value, -1), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeDown, window, cx| {
                        emit(range.nudge(value, -1), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeRight, window, cx| {
                        emit(range.nudge(value, 1), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeUp, window, cx| {
                        emit(range.nudge(value, 1), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgePageDown, window, cx| {
                        emit(range.nudge(value, -PAGE), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgePageUp, window, cx| {
                        emit(range.nudge(value, PAGE), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeToStart, window, cx| {
                        emit(range.resolve(range.min), change.clone(), window, cx)
                    }
                })
                .on_action({
                    let change = change.clone();
                    move |_: &NudgeToEnd, window, cx| {
                        emit(range.resolve(range.max), change.clone(), window, cx)
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
    range: SliderRange,
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
    emit(map.range.at_ratio(ratio), on_change, window, cx);
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

    fn near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
    }

    #[test]
    fn snap_and_keyboard_stay_inside_the_range() {
        near(snap_to_step(0.26, 0.0, 1.0, 0.25), 0.25);
        near(nudge(1.0, 0.0, 1.0, 0.25, 1), 1.0);
        near(nudge(0.0, 0.0, 1.0, 0.25, -1), 0.0);
        near(nudge(0.0, 0.0, 1.0, 0.0, 1), 0.01);
        near(value_from_ratio(1.2, 0.0, 10.0, 1.0), 10.0);
        near(slider_ratio(5.0, 0.0, 10.0), 0.5);
    }

    #[test]
    fn unusable_ranges_collapse_to_zero_on_every_path() {
        for (min, max) in [
            (10.0, 0.0),
            (f32::NAN, 1.0),
            (0.0, f32::INFINITY),
            (f32::NEG_INFINITY, f32::NAN),
        ] {
            near(effective_step(min, max, 1.0), 0.0);
            near(snap_to_step(5.0, min, max, 1.0), 0.0);
            near(slider_ratio(5.0, min, max), 0.0);
            near(value_from_ratio(0.4, min, max, 1.0), 0.0);
            near(nudge(5.0, min, max, 1.0, 3), 0.0);
        }
    }

    #[test]
    fn zero_span_is_that_single_value() {
        near(effective_step(4.0, 4.0, 1.0), 0.0);
        near(snap_to_step(9.0, 4.0, 4.0, 1.0), 4.0);
        near(snap_to_step(f32::NAN, 4.0, 4.0, 1.0), 4.0);
        near(slider_ratio(9.0, 4.0, 4.0), 0.0);
        near(value_from_ratio(1.0, 4.0, 4.0, 1.0), 4.0);
        near(nudge(9.0, 4.0, 4.0, 1.0, 5), 4.0);
    }

    #[test]
    fn out_of_range_and_non_finite_values_clamp_then_snap() {
        near(snap_to_step(-10.0, 0.0, 10.0, 2.0), 0.0);
        near(snap_to_step(11.0, 0.0, 10.0, 2.0), 10.0);
        near(snap_to_step(f32::NAN, 2.0, 8.0, 2.0), 2.0);
        near(snap_to_step(f32::INFINITY, 0.0, 10.0, 2.0), 10.0);
        near(snap_to_step(f32::NEG_INFINITY, 0.0, 10.0, 2.0), 0.0);
        near(nudge(100.0, 0.0, 10.0, 2.0, 1), 10.0);
        near(nudge(-4.0, 0.0, 10.0, 2.0, -1), 0.0);
    }

    #[test]
    fn non_positive_steps_share_one_fallback() {
        near(effective_step(0.0, 100.0, 0.0), 1.0);
        near(effective_step(0.0, 100.0, -5.0), 1.0);
        near(effective_step(0.0, 100.0, f32::NAN), 1.0);
        near(effective_step(0.0, 100.0, f32::INFINITY), 1.0);
        near(nudge(0.0, 0.0, 1.0, -1.0, 1), 0.01);
        near(snap_to_step(0.264, 0.0, 1.0, 0.0), 0.26);
    }

    #[test]
    fn pointer_keyboard_and_thumb_use_the_same_range() {
        let min = 0.0;
        let max = 10.0;
        let step = 2.0;
        for raw in [0.0, 1.2, 3.0, 9.9, 20.0, -1.0, f32::NAN] {
            let resolved = snap_to_step(raw, min, max, step);
            let ratio = slider_ratio(resolved, min, max);
            near(value_from_ratio(ratio, min, max, step), resolved);
            near(nudge(resolved, min, max, step, 0), resolved);
        }
        near(
            value_from_ratio(0.0, min, max, step),
            snap_to_step(min, min, max, step),
        );
        near(
            value_from_ratio(1.0, min, max, step),
            snap_to_step(max, min, max, step),
        );
        near(nudge(4.0, min, max, step, 1), 6.0);
        near(nudge(4.0, min, max, step, -1), 2.0);
    }
}
