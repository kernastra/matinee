//! Modal dialog.
//!
//! The scrim covers the window. Focus moves to a safe action on open and
//! cannot tab behind the modal. Escape runs the cancel action when one
//! exists. Enter is the focused button's own activation, so a destructive
//! action is never the control focused on open. Closing restores the focus
//! that was current before the dialog opened.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnchoredPositionMode, AnimationExt, AnyElement, App, ElementId, FocusHandle,
    InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, Styled, Window,
    anchored, deferred, div, point, px,
};

use crate::{
    ActiveTheme, StyledExt,
    components::{
        Button, ButtonVariant, Text,
        focus_gate::FocusGate,
        keybindings::{DIALOG_CONTEXT, Dismiss, FocusNext, FocusPrevious},
    },
    motion,
    overlay::{DialogActionRole, initial_dialog_focus},
    tokens::{Elevation, MotionDuration, Radius, Space, TextRole},
};

const DIALOG_PRIORITY: usize = 30;
const MAX_WIDTH: f32 = 420.0;
/// How far a backward wrap may walk the tab order looking for the last stop
/// still inside the dialog. A dialog does not contain a large tab cycle.
const TRAP_SCAN: usize = 64;

type ActionHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// One dialog button. The label is the accessible name.
pub struct DialogAction {
    label: gpui::SharedString,
    role: DialogActionRole,
    on_activate: Option<ActionHandler>,
}

impl DialogAction {
    pub fn new(label: impl Into<gpui::SharedString>, role: DialogActionRole) -> Self {
        Self {
            label: label.into(),
            role,
            on_activate: None,
        }
    }

    pub fn on_activate(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_activate = Some(Rc::new(handler));
        self
    }

    pub fn role(&self) -> DialogActionRole {
        self.role
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}

struct DialogState {
    gate: FocusGate,
    actions: RefCell<Vec<FocusHandle>>,
}

/// A modal layer. Click-outside dismisses only when explicitly enabled.
#[derive(IntoElement)]
pub struct Dialog {
    id: ElementId,
    open: bool,
    title: gpui::SharedString,
    content: Option<AnyElement>,
    actions: Vec<DialogAction>,
    dismiss_on_outside: bool,
    on_dismiss: Option<ActionHandler>,
}

impl Dialog {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: false,
            title: "Dialog".into(),
            content: None,
            actions: Vec::new(),
            dismiss_on_outside: false,
            on_dismiss: None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn title(mut self, title: impl Into<gpui::SharedString>) -> Self {
        self.title = title.into();
        self
    }

    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }

    pub fn action(mut self, action: DialogAction) -> Self {
        self.actions.push(action);
        self
    }

    /// When true, a pointer press on the scrim dismisses the dialog.
    /// Destructive confirmations leave this off.
    pub fn dismiss_on_outside(mut self, dismiss: bool) -> Self {
        self.dismiss_on_outside = dismiss;
        self
    }

    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id.clone();
        let model = window.use_keyed_state(id.clone(), cx, |_, cx| DialogState {
            gate: FocusGate::new(cx.focus_handle().tab_stop(false)),
            actions: RefCell::new(Vec::new()),
        });
        ensure_handles(&model, self.actions.len(), cx);
        let handles = model.read(cx).actions.borrow().clone();
        let roles: Vec<DialogActionRole> = self.actions.iter().map(DialogAction::role).collect();
        let initial = initial_dialog_focus(&roles).and_then(|index| handles.get(index).cloned());
        let was_open = model.read(cx).gate.was_open();
        let open = self.open;
        let contained = model.update(cx, |state, cx| state.gate.sync(open, true, window, cx));
        if open && !was_open {
            if let Some(handle) = initial.clone() {
                window.focus(&handle);
            }
        } else if open && !contained {
            let fallback = initial
                .clone()
                .unwrap_or_else(|| model.read(cx).gate.focus.clone());
            window.focus(&fallback);
        }
        if !open {
            return div().id(id).into_any_element();
        }

        let theme = cx.theme().clone();
        let viewport = window.viewport_size();
        let container = model.read(cx).gate.focus.clone();
        let dismiss = self.on_dismiss.clone();
        let cancel = self
            .actions
            .iter()
            .find(|action| action.role() == DialogActionRole::Cancel)
            .and_then(|action| action.on_activate.clone());
        let outside = self.dismiss_on_outside.then(|| dismiss.clone()).flatten();

        let mut panel = div()
            .id((id.clone(), "panel"))
            .key_context(DIALOG_CONTEXT)
            .track_focus(&container)
            .w(px(MAX_WIDTH))
            .max_w_full()
            .flex()
            .flex_col()
            .gap(Space::S4.px())
            .p(Space::S5.px())
            .bg(theme.colors.surface.elevated)
            .border_1()
            .border_color(theme.colors.border.default)
            .corner_radius(&theme, Radius::Large)
            .elevation(&theme, Elevation::Overlay)
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_action({
                let container = container.clone();
                move |_: &FocusNext, window, cx| trap(true, &container, window, cx)
            })
            .on_action({
                let container = container.clone();
                move |_: &FocusPrevious, window, cx| trap(false, &container, window, cx)
            })
            .child(Text::new(self.title.clone()).role(TextRole::Heading))
            .children(self.content)
            .child(action_row(&self.actions, &handles));

        if let Some(cancel) = cancel.clone() {
            panel = panel.on_action(move |_: &Dismiss, window, cx| cancel(window, cx));
        } else if let Some(dismiss) = dismiss.clone() {
            panel = panel.on_action(move |_: &Dismiss, window, cx| dismiss(window, cx));
        }

        let panel = present(id.clone(), panel, cx);
        let mut scrim = div()
            .occlude()
            .w(viewport.width)
            .h(viewport.height)
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.colors.surface.overlay)
            .child(panel);
        if let Some(outside) = outside {
            scrim = scrim.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                outside(window, cx);
            });
        }

        deferred(
            anchored()
                .anchor(gpui::Corner::TopLeft)
                .position(point(px(0.0), px(0.0)))
                .position_mode(AnchoredPositionMode::Window)
                .child(scrim),
        )
        .with_priority(DIALOG_PRIORITY)
        .into_any_element()
    }
}

fn ensure_handles(model: &gpui::Entity<DialogState>, count: usize, cx: &mut App) {
    let mut handles = model.read(cx).actions.borrow_mut();
    while handles.len() < count {
        handles.push(cx.focus_handle().tab_stop(true));
    }
    handles.truncate(count);
}

fn action_row(actions: &[DialogAction], handles: &[FocusHandle]) -> impl IntoElement {
    let mut row = div()
        .w_full()
        .flex()
        .flex_row()
        .justify_end()
        .items_center()
        .gap(Space::S2.px());
    for (index, action) in actions.iter().enumerate() {
        let mut button = Button::new(("dialog-action", index), action.label.clone())
            .variant(variant_for(action.role()));
        if let Some(handle) = handles.get(index) {
            button = button.focus_handle(handle.clone());
        }
        if let Some(handler) = action.on_activate.clone() {
            button = button.on_click(move |_, window, cx| handler(window, cx));
        }
        row = row.child(button);
    }
    row
}

fn variant_for(role: DialogActionRole) -> ButtonVariant {
    match role {
        DialogActionRole::Default => ButtonVariant::Primary,
        DialogActionRole::Cancel | DialogActionRole::Normal => ButtonVariant::Secondary,
        DialogActionRole::Destructive => ButtonVariant::Destructive,
    }
}

fn present<E>(id: ElementId, panel: E, cx: &App) -> AnyElement
where
    E: AnimationExt + IntoElement + Styled + 'static,
{
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

/// Move one tab stop. If that lands outside the dialog, wrap to the first
/// stop inside (forward) or the last (backward).
fn trap(forward: bool, container: &FocusHandle, window: &mut Window, cx: &App) {
    if forward {
        window.focus_next();
    } else {
        window.focus_prev();
    }
    if container.contains_focused(window, cx) {
        return;
    }
    window.focus(container);
    if forward {
        window.focus_next();
        return;
    }
    let mut last_inside = None;
    for _ in 0..TRAP_SCAN {
        window.focus_next();
        if container.contains_focused(window, cx) {
            last_inside = window.focused(cx);
        } else if last_inside.is_some() {
            break;
        }
    }
    if let Some(handle) = last_inside {
        window.focus(&handle);
    }
}
