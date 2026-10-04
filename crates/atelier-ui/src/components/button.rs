use std::rc::Rc;

use gpui::{
    App, ClickEvent, ElementId, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};

use crate::{
    ActiveTheme, Theme,
    components::{FocusRing, Icon, IconName, IconSize, Text, tooltip::WithTooltip},
    tokens::{Color, Radius, Space, TextRole},
};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ButtonVariant {
    /// The single most important action in a context.
    Primary,
    /// Standard actions.
    #[default]
    Secondary,
    /// Chromeless actions in toolbars and dense layouts.
    Subtle,
    /// Actions that destroy data. Use sparingly.
    Destructive,
}

impl ButtonVariant {
    pub const ALL: [ButtonVariant; 4] = [
        ButtonVariant::Primary,
        ButtonVariant::Secondary,
        ButtonVariant::Subtle,
        ButtonVariant::Destructive,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            ButtonVariant::Primary => "Primary",
            ButtonVariant::Secondary => "Secondary",
            ButtonVariant::Subtle => "Subtle",
            ButtonVariant::Destructive => "Destructive",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ButtonSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl ButtonSize {
    pub const ALL: [ButtonSize; 3] = [ButtonSize::Small, ButtonSize::Medium, ButtonSize::Large];

    pub const fn name(self) -> &'static str {
        match self {
            ButtonSize::Small => "Small",
            ButtonSize::Medium => "Medium",
            ButtonSize::Large => "Large",
        }
    }

    pub const fn metrics(self) -> ButtonMetrics {
        match self {
            ButtonSize::Small => ButtonMetrics {
                height: 24.0,
                padding_x: Space::S2,
                gap: Space::S1,
                icon: IconSize::Small,
                text: TextRole::Label,
            },
            ButtonSize::Medium => ButtonMetrics {
                height: 30.0,
                padding_x: Space::S3,
                gap: Space::S2,
                icon: IconSize::Medium,
                text: TextRole::Label,
            },
            ButtonSize::Large => ButtonMetrics {
                height: 38.0,
                padding_x: Space::S4,
                gap: Space::S2,
                icon: IconSize::Large,
                text: TextRole::Subheading,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonMetrics {
    pub height: f32,
    pub padding_x: Space,
    pub gap: Space,
    pub icon: IconSize,
    pub text: TextRole,
}

/// Behavioral status of a control, independent of transient pointer state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ButtonStatus {
    #[default]
    Enabled,
    /// Not interactive and not focusable.
    Disabled,
    /// Work in progress: stays focusable (so focus is not lost mid-action)
    /// but ignores activation.
    Loading,
}

impl ButtonStatus {
    pub const fn accepts_activation(self) -> bool {
        matches!(self, ButtonStatus::Enabled)
    }

    pub const fn is_focusable(self) -> bool {
        !matches!(self, ButtonStatus::Disabled)
    }
}

/// Fully resolved colors for every interaction state of a button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonColors {
    pub background: Color,
    pub background_hover: Color,
    pub background_pressed: Color,
    pub foreground: Color,
    pub border: Color,
}

impl ButtonColors {
    /// Pure mapping from theme + variant + status to colors. Kept separate
    /// from rendering so state behavior is unit-testable without a window.
    pub fn resolve(theme: &Theme, variant: ButtonVariant, status: ButtonStatus) -> Self {
        let c = &theme.colors;
        if status == ButtonStatus::Disabled {
            let background = match variant {
                ButtonVariant::Subtle => Color::TRANSPARENT,
                _ => c.control.disabled,
            };
            return Self {
                background,
                background_hover: background,
                background_pressed: background,
                foreground: c.text.disabled,
                border: Color::TRANSPARENT,
            };
        }

        let (background, background_hover, background_pressed, foreground, border) = match variant {
            ButtonVariant::Primary => (
                c.control.accent,
                c.control.accent_hover,
                c.control.accent_pressed,
                c.text.on_accent,
                Color::TRANSPARENT,
            ),
            ButtonVariant::Secondary => (
                c.control.neutral,
                c.control.neutral_hover,
                c.control.neutral_pressed,
                c.text.primary,
                c.border.default,
            ),
            ButtonVariant::Subtle => (
                Color::TRANSPARENT,
                c.control.subtle_hover,
                c.control.subtle_pressed,
                c.text.primary,
                Color::TRANSPARENT,
            ),
            ButtonVariant::Destructive => (
                c.control.destructive,
                c.control.destructive_hover,
                c.control.destructive_pressed,
                c.text.on_destructive,
                Color::TRANSPARENT,
            ),
        };

        if status == ButtonStatus::Loading {
            return Self {
                background,
                background_hover: background,
                background_pressed: background,
                foreground,
                border,
            };
        }

        Self {
            background,
            background_hover,
            background_pressed,
            foreground,
            border,
        }
    }
}

/// Shared interaction model for every button-like control: pointer states,
/// focus ring, keyboard activation (Enter/Space via GPUI keyboard clicks),
/// and the rule that pointer presses do not move keyboard focus.
struct ButtonChrome {
    id: ElementId,
    variant: ButtonVariant,
    size: ButtonSize,
    status: ButtonStatus,
    on_click: Option<ClickHandler>,
    tooltip: Option<SharedString>,
    focus: Option<FocusHandle>,
}

/// Per-button state that must survive across frames.
///
/// Pointer-press state is owned by the component rather than GPUI's
/// `active` style: GPUI registers its release listener only on the frame
/// after the press, so a press and release within one frame leave `active`
/// stuck. The focus handle is explicit so the ring can be rendered as an
/// element (see [`FocusRing`]).
struct ChromeState {
    pressed: bool,
    focus: FocusHandle,
}

const BORDER_WIDTH: f32 = 1.0;

impl ButtonChrome {
    fn render(
        self,
        width: Option<f32>,
        fill: bool,
        content: impl IntoElement,
        loading_indicator: Option<Icon>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let press = window.use_keyed_state(self.id.clone(), cx, |_, cx| ChromeState {
            pressed: false,
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
        });
        let pressed = press.read(cx).pressed;
        let focus = self
            .focus
            .clone()
            .unwrap_or_else(|| press.read(cx).focus.clone());
        let focusable = self.status.is_focusable();
        let focused = focusable && focus.is_focused(window);
        let theme = cx.theme();
        let metrics = self.size.metrics();
        let colors = ButtonColors::resolve(theme, self.variant, self.status);
        let corner_radius = theme.radius.get(Radius::Medium);
        let activates = self.status.accepts_activation();
        let pointer_background = if pressed {
            colors.background_pressed
        } else {
            colors.background_hover
        };
        let set_pressed = move |value: bool, cx: &mut App| {
            press.update(cx, |state, cx| {
                if state.pressed != value {
                    state.pressed = value;
                    cx.notify();
                }
            });
        };

        div()
            .id(self.id)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(px(metrics.height))
            .map(|this| {
                if fill {
                    this.w_full()
                } else {
                    match width {
                        Some(w) => this.w(px(w)),
                        None => this.px(metrics.padding_x.px()),
                    }
                }
            })
            .rounded(px(corner_radius))
            .bg(colors.background)
            .border(px(BORDER_WIDTH))
            .border_color(colors.border)
            .text_color(colors.foreground)
            .when(focusable, |this| this.track_focus(&focus))
            .when(activates && pressed, |this| {
                this.bg(colors.background_pressed)
            })
            .when(activates, |this| {
                this.cursor_pointer()
                    .hover(move |style| style.bg(pointer_background))
                    .on_mouse_down(MouseButton::Left, {
                        let set_pressed = set_pressed.clone();
                        move |_, window, cx| {
                            // Pointer presses must not move keyboard focus
                            // or show the keyboard focus ring.
                            window.prevent_default();
                            crate::note_pointer_interaction(cx);
                            set_pressed(true, cx);
                        }
                    })
                    .on_mouse_up(MouseButton::Left, {
                        let set_pressed = set_pressed.clone();
                        move |_, _, cx| set_pressed(false, cx)
                    })
                    .on_mouse_up_out(MouseButton::Left, move |_, _, cx| set_pressed(false, cx))
            })
            .when_some(self.on_click.filter(|_| activates), |this, handler| {
                this.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .when_some(self.tooltip, |this, text| this.text_tooltip(text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(metrics.gap.px())
                    .when(loading_indicator.is_some(), |this| this.opacity(0.0))
                    .child(content),
            )
            .when_some(loading_indicator, |this, spinner| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(spinner),
                )
            })
            .when(crate::focus_visible(focused, cx), |this| {
                this.child(FocusRing::new(corner_radius, BORDER_WIDTH))
            })
    }
}

/// A labelled push button.
///
/// The label doubles as the accessible name. While `loading`, the label is
/// kept in layout (so the button does not change width) and an activity
/// indicator is drawn over it.
#[derive(IntoElement)]
pub struct Button {
    chrome: ButtonChrome,
    label: SharedString,
    icon: Option<IconName>,
    fill: bool,
    /// When loading, keep the label visible and place the activity icon beside it.
    show_label_while_loading: bool,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            chrome: ButtonChrome {
                id: id.into(),
                variant: ButtonVariant::default(),
                size: ButtonSize::default(),
                status: ButtonStatus::default(),
                on_click: None,
                tooltip: None,
                focus: None,
            },
            label: label.into(),
            icon: None,
            fill: false,
            show_label_while_loading: false,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.chrome.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.chrome.size = size;
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        if disabled {
            self.chrome.status = ButtonStatus::Disabled;
        } else if self.chrome.status == ButtonStatus::Disabled {
            self.chrome.status = ButtonStatus::Enabled;
        }
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        if loading && self.chrome.status != ButtonStatus::Disabled {
            self.chrome.status = ButtonStatus::Loading;
        } else if !loading && self.chrome.status == ButtonStatus::Loading {
            self.chrome.status = ButtonStatus::Enabled;
        }
        self
    }

    /// Stretch to the width of the parent.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    /// Show the label next to the activity icon while loading.
    ///
    /// Without this, the label stays in the layout at zero opacity and the
    /// activity icon is centered over it, so the button width does not change.
    pub fn show_label_while_loading(mut self, show: bool) -> Self {
        self.show_label_while_loading = show;
        self
    }

    pub fn status(&self) -> ButtonStatus {
        self.chrome.status
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.chrome.on_click = Some(Rc::new(handler));
        self
    }

    /// Use an existing focus handle so a dialog can focus this action.
    pub fn focus_handle(mut self, focus: FocusHandle) -> Self {
        self.chrome.focus = Some(focus);
        self
    }
}

impl std::fmt::Debug for Button {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Button")
            .field("label", &self.label)
            .field("variant", &self.chrome.variant)
            .field("size", &self.chrome.size)
            .field("status", &self.chrome.status)
            .finish()
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let metrics = self.chrome.size.metrics();
        let foreground =
            ButtonColors::resolve(cx.theme(), self.chrome.variant, self.chrome.status).foreground;
        let loading = self.chrome.status == ButtonStatus::Loading;
        let spinner = loading.then(|| {
            Icon::new(IconName::Spinner)
                .size(metrics.icon)
                .color(foreground)
                .spinning(true)
        });
        let show_label = self.show_label_while_loading;
        let content = div()
            .flex()
            .items_center()
            .gap(metrics.gap.px())
            .when_some(spinner.filter(|_| show_label), |this, spinner| {
                this.child(spinner)
            })
            .when_some(
                self.icon.filter(|_| !(loading && show_label)),
                |this, icon| this.child(Icon::new(icon).size(metrics.icon).color(foreground)),
            )
            .child(
                Text::new(self.label)
                    .role(metrics.text)
                    .color(foreground)
                    .truncate(),
            );
        let overlay = if loading && !show_label {
            Some(
                Icon::new(IconName::Spinner)
                    .size(metrics.icon)
                    .color(foreground)
                    .spinning(true),
            )
        } else {
            None
        };
        self.chrome
            .render(None, self.fill, content, overlay, window, cx)
    }
}

/// A square, icon-only button. A label is mandatory: it is shown as a
/// tooltip and is the control's accessible name.
#[derive(IntoElement)]
pub struct IconButton {
    chrome: ButtonChrome,
    icon: IconName,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            chrome: ButtonChrome {
                id: id.into(),
                variant: ButtonVariant::Subtle,
                size: ButtonSize::default(),
                status: ButtonStatus::default(),
                on_click: None,
                tooltip: Some(label.into()),
                focus: None,
            },
            icon,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.chrome.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.chrome.size = size;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.chrome.status = if disabled {
            ButtonStatus::Disabled
        } else {
            ButtonStatus::Enabled
        };
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.chrome.on_click = Some(Rc::new(handler));
        self
    }
}

impl std::fmt::Debug for IconButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IconButton")
            .field("icon", &self.icon)
            .field("label", &self.chrome.tooltip)
            .field("variant", &self.chrome.variant)
            .field("status", &self.chrome.status)
            .finish()
    }
}

impl RenderOnce for IconButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let metrics = self.chrome.size.metrics();
        let foreground =
            ButtonColors::resolve(cx.theme(), self.chrome.variant, self.chrome.status).foreground;
        let content = Icon::new(self.icon).size(metrics.icon).color(foreground);
        self.chrome
            .render(Some(metrics.height), false, content, None, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes() -> [Theme; 2] {
        [Theme::neutral_dark(), Theme::neutral_light()]
    }

    #[test]
    fn enabled_buttons_have_distinct_pointer_states() {
        for theme in themes() {
            for variant in ButtonVariant::ALL {
                let c = ButtonColors::resolve(&theme, variant, ButtonStatus::Enabled);
                assert_ne!(c.background, c.background_hover, "{variant:?} hover");
                assert_ne!(
                    c.background_hover, c.background_pressed,
                    "{variant:?} pressed"
                );
            }
        }
    }

    #[test]
    fn disabled_and_loading_buttons_have_no_pointer_feedback() {
        for theme in themes() {
            for variant in ButtonVariant::ALL {
                for status in [ButtonStatus::Disabled, ButtonStatus::Loading] {
                    let c = ButtonColors::resolve(&theme, variant, status);
                    assert_eq!(c.background, c.background_hover, "{variant:?} {status:?}");
                    assert_eq!(c.background, c.background_pressed, "{variant:?} {status:?}");
                }
                let disabled = ButtonColors::resolve(&theme, variant, ButtonStatus::Disabled);
                assert_eq!(disabled.foreground, theme.colors.text.disabled);
            }
        }
    }

    #[test]
    fn status_rules() {
        assert!(ButtonStatus::Enabled.accepts_activation());
        assert!(!ButtonStatus::Loading.accepts_activation());
        assert!(!ButtonStatus::Disabled.accepts_activation());
        assert!(ButtonStatus::Loading.is_focusable());
        assert!(!ButtonStatus::Disabled.is_focusable());
    }

    #[test]
    fn disabled_wins_over_loading() {
        let b = Button::new("b", "Save").disabled(true).loading(true);
        assert_eq!(b.status(), ButtonStatus::Disabled);
        let b = Button::new("b", "Save").loading(true).loading(false);
        assert_eq!(b.status(), ButtonStatus::Enabled);
    }

    #[test]
    fn sizes_grow_and_align_to_grid() {
        let heights: Vec<f32> = ButtonSize::ALL.iter().map(|s| s.metrics().height).collect();
        assert!(heights.windows(2).all(|w| w[0] < w[1]));
        assert!(heights.iter().all(|h| h % 2.0 == 0.0));
    }
}
