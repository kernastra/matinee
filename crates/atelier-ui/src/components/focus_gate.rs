//! Open/close focus for overlays.
//!
//! The gate remembers the focus handle that was current when the overlay
//! opened. On close it restores that handle only if focus is still inside.
//! A menu can claim the pending focus so the gate itself is not the focused
//! element. The pending flag is shared (`Rc`) so the menu and the shell see
//! the same cell.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, FocusHandle, Window};

use crate::overlay::{CloseFocus, focus_after_close};

#[derive(Clone)]
pub(crate) struct GateShare {
    pending: Rc<RefCell<bool>>,
    child: Rc<RefCell<Option<FocusHandle>>>,
}

impl GateShare {
    pub(crate) fn claim(&self, focus: &FocusHandle, window: &mut Window) {
        if *self.pending.borrow() {
            *self.child.borrow_mut() = Some(focus.clone());
            *self.pending.borrow_mut() = false;
            window.focus(focus);
        }
    }
}

pub(crate) struct FocusGate {
    pub focus: FocusHandle,
    restore: RefCell<Option<FocusHandle>>,
    share: GateShare,
    was_open: RefCell<bool>,
}

impl FocusGate {
    pub(crate) fn new(focus: FocusHandle) -> Self {
        Self {
            focus,
            restore: RefCell::new(None),
            share: GateShare {
                pending: Rc::new(RefCell::new(false)),
                child: Rc::new(RefCell::new(None)),
            },
            was_open: RefCell::new(false),
        }
    }

    pub(crate) fn share(&self) -> GateShare {
        self.share.clone()
    }

    pub(crate) fn was_open(&self) -> bool {
        *self.was_open.borrow()
    }

    pub(crate) fn focus_inside(&self, window: &Window, cx: &App) -> bool {
        if self.focus.contains_focused(window, cx) {
            return true;
        }
        self.share
            .child
            .borrow()
            .as_ref()
            .is_some_and(|handle| handle.is_focused(window) || handle.contains_focused(window, cx))
    }

    /// `false` means focus has left and the caller should dismiss.
    pub(crate) fn sync(
        &self,
        open: bool,
        manages_focus: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let was_open = *self.was_open.borrow();
        if open && was_open && !*self.share.pending.borrow() && !self.focus_inside(window, cx) {
            return false;
        }
        if open && !was_open {
            *self.restore.borrow_mut() = window.focused(cx);
            *self.was_open.borrow_mut() = true;
            if manages_focus {
                window.focus(&self.focus);
            } else {
                *self.share.pending.borrow_mut() = true;
            }
        } else if !open && was_open {
            self.close(window, cx);
        }
        true
    }

    pub(crate) fn close(&self, window: &mut Window, cx: &mut App) {
        let inside = self.focus_inside(window, cx);
        *self.was_open.borrow_mut() = false;
        *self.share.pending.borrow_mut() = false;
        if focus_after_close(inside) == CloseFocus::Restore
            && let Some(handle) = self.restore.borrow_mut().take()
        {
            window.focus(&handle);
        } else {
            self.restore.borrow_mut().take();
        }
    }
}
