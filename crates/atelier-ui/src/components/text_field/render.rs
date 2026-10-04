//! Visual composition of the field, its label, and the trailing action.

use gpui::prelude::FluentBuilder;
use gpui::{
    App, Bounds, ContentMask, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    GlobalElementId, InspectorElementId, InteractiveElement, IntoElement, LayoutId, MouseButton,
    MouseMoveEvent, ParentElement, Pixels, RenderOnce, SharedString, StatefulInteractiveElement,
    Style, Styled, Window, div, point, px, relative,
};

use crate::{
    ActiveTheme,
    components::{Icon, IconName, IconSize, Text, tooltip::WithTooltip, v_stack},
    editing::sanitize_single_line,
    focus,
    inspect::{self, Inspection},
    tokens::{Color, Radius, Space, TextRole, Weight},
};

use super::{
    TextField,
    input::bind_editing,
    layout::{self, FieldPrepaint, LineStyle},
    mask::inspection_value,
    state::{FieldState, TrailingHandler},
};

/// Height aligned with [`crate::components::ButtonSize::Medium`].
const FIELD_HEIGHT: f32 = 30.0;
const BORDER_WIDTH: f32 = 1.0;

impl RenderOnce for TextField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let model = window.use_keyed_state(self.id.clone(), cx, {
            let initial = sanitize_single_line(&self.value);
            move |_, cx| FieldState::new(initial, cx)
        });

        let focused = model.read(cx).focus.is_focused(window) && !self.disabled;
        model.update(cx, |state, cx| {
            state.disabled = self.disabled;
            state.masked = self.masked;
            state.on_change.clone_from(&self.on_change);
            state.on_focus_change.clone_from(&self.on_focus_change);
            if !self.disabled {
                state.sync_external(&self.value);
            }
            let reported = state.reported_focus.get();
            if reported != Some(focused) {
                state.reported_focus.set(Some(focused));
                if let Some(handler) = state.on_focus_change.clone() {
                    cx.defer(move |cx| handler(focused, cx));
                }
            }
        });

        if self.inspected {
            inspect::report_inspection(Inspection {
                name: self.inspection_name,
                focused,
                value: inspection_value(&self.value, self.masked),
            });
        }

        let colors = &theme.colors;
        let border = if self.invalid {
            colors.control.destructive
        } else if focused {
            colors.focus.ring
        } else {
            colors.border.default
        };
        let hover_border = if self.invalid {
            colors.control.destructive
        } else if focused {
            colors.focus.ring
        } else {
            colors.border.strong
        };
        let text_color = if self.disabled {
            colors.text.disabled
        } else {
            colors.text.primary
        };
        let supporting_color = if self.invalid && !self.disabled {
            colors.text.danger
        } else {
            colors.text.muted
        };
        let style = theme.typography.style(TextRole::Body);
        let family = theme.typography.family(TextRole::Body);
        let radius = theme.radius.get(Radius::Medium);
        let focus_handle = model.read(cx).focus.clone();

        let field = div()
            .id(self.id.clone())
            .key_context(self.key_context)
            .when(!self.disabled, |this| this.track_focus(&focus_handle))
            .when_some(self.width, |this, width| this.w(px(width)))
            .when(self.width.is_none(), |this| this.w_full().min_w(px(0.0)))
            .h(px(FIELD_HEIGHT))
            .px(Space::S2.px())
            .flex()
            .flex_row()
            .items_center()
            .gap(Space::S2.px())
            .rounded(px(radius))
            .bg(if self.disabled {
                colors.control.disabled
            } else {
                colors.surface.elevated
            })
            .border(px(BORDER_WIDTH))
            .border_color(border)
            .when(!self.disabled, |this| {
                this.cursor(CursorStyle::IBeam)
                    .hover(move |style| style.border_color(hover_border))
            })
            .when(!self.disabled, |this| bind_editing(this, model.clone()))
            .when(!self.disabled, |this| {
                let down = model.clone();
                let up = model.clone();
                let up_out = model.clone();
                let moved = model.clone();
                this.on_mouse_down(MouseButton::Left, move |event, _, cx| {
                    focus::note_pointer_interaction(cx);
                    down.update(cx, |state, cx| state.mouse_down(event, cx));
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    up.update(cx, |state, _| state.selecting.set(false));
                })
                .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                    up_out.update(cx, |state, _| state.selecting.set(false));
                })
                .on_mouse_move(move |event: &MouseMoveEvent, _, cx| {
                    moved.update(cx, |state, cx| state.mouse_move(event, cx));
                })
            })
            .when_some(self.leading_icon, |this, icon| {
                this.child(
                    Icon::new(icon)
                        .size(IconSize::Small)
                        .color(if self.disabled {
                            colors.text.disabled
                        } else {
                            colors.text.muted
                        }),
                )
            })
            .child(FieldText {
                model: model.clone(),
                placeholder: self.placeholder.clone(),
                font_family: family.to_string().into(),
                font_weight: style.weight,
                font_size: style.size,
                line_height: (FIELD_HEIGHT - BORDER_WIDTH * 2.0).min(style.line_height),
                text_color,
                placeholder_color: colors.text.muted,
                selection_color: colors.focus.ring.with_alpha(0.35),
                caret_color: colors.text.primary,
                focused,
                disabled: self.disabled,
            })
            .when_some(
                self.trailing_icon.zip(self.trailing_label),
                |this, (icon, label)| {
                    let action = self.trailing_action.clone();
                    this.child(trailing_button(
                        icon,
                        label,
                        colors.text.secondary,
                        theme.radius.get(Radius::Small),
                        action,
                    ))
                },
            );

        v_stack(Space::S1)
            .when(self.width.is_none(), |this| this.w_full())
            .when_some(self.label, |this, label| {
                this.child(
                    Text::new(label)
                        .role(TextRole::Label)
                        .color(if self.disabled {
                            colors.text.disabled
                        } else {
                            colors.text.secondary
                        }),
                )
            })
            .child(field)
            .when_some(self.supporting_text, |this, text| {
                this.child(
                    Text::new(text)
                        .role(TextRole::Caption)
                        .color(supporting_color),
                )
            })
    }
}

fn trailing_button(
    icon: IconName,
    label: SharedString,
    color: Color,
    radius: f32,
    action: Option<TrailingHandler>,
) -> impl IntoElement {
    div()
        .id("text-field-trailing")
        .flex_none()
        .size(px(20.0))
        .rounded(px(radius))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_tooltip(label)
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            focus::note_pointer_interaction(cx);
        })
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            if let Some(action) = action.clone() {
                action(window, cx);
            }
        })
        .child(Icon::new(icon).size(IconSize::Small).color(color))
}

struct FieldText {
    model: Entity<FieldState>,
    placeholder: SharedString,
    font_family: SharedString,
    font_weight: Weight,
    font_size: f32,
    line_height: f32,
    text_color: Color,
    placeholder_color: Color,
    selection_color: Color,
    caret_color: Color,
    focused: bool,
    disabled: bool,
}

impl IntoElement for FieldText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FieldText {
    type RequestLayoutState = ();
    type PrepaintState = FieldPrepaint;

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
        style.size.height = px(self.line_height).into();
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let state = self.model.read(cx);
        layout::layout_field(
            state,
            &LineStyle {
                placeholder: self.placeholder.clone(),
                font_family: self.font_family.clone(),
                font_weight: self.font_weight,
                font_size: self.font_size,
                text_color: self.text_color,
                placeholder_color: self.placeholder_color,
                selection_color: self.selection_color,
                caret_color: self.caret_color,
                focused: self.focused,
                disabled: self.disabled,
            },
            bounds,
            window,
        )
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.focused && !self.disabled {
            let focus = self.model.read(cx).focus.clone();
            window.handle_input(
                &focus,
                ElementInputHandler::new(bounds, self.model.clone()),
                cx,
            );
        }
        let scroll = prepaint.scroll;
        let line = prepaint.line.take();
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection);
            }
            if let Some(line) = line {
                line.paint(
                    point(bounds.left() - scroll, bounds.top()),
                    px(self.line_height),
                    window,
                    cx,
                )
                .ok();
            }
            if let Some(caret) = prepaint.caret.take() {
                window.paint_quad(caret);
            }
        });
    }
}
