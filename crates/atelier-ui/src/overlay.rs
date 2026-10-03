//! Pure overlay and dialog focus decisions.
//!
//! Rendering lives in the components. These functions are the rules tests
//! can pin without a window: where a floating layer anchors, which dialog
//! action is safe to focus, and whether closing puts focus back.

use gpui::Corner;

/// Preferred side of the anchor the floating layer tries first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Placement {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
}

/// Alignment of the floating layer along the anchor's crossing edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Alignment {
    #[default]
    Start,
    Center,
    End,
}

/// Axis-aligned anchor in logical pixels. The origin is the top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Where to pin a floating layer before GPUI flips it into the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchoredOrigin {
    /// Corner of the floating layer that sits on [`Self::x`], [`Self::y`].
    pub corner: Corner,
    pub x: f32,
    pub y: f32,
}

/// Anchor point for `placement` and `align`, inset by `gap` logical pixels.
///
/// Center alignment is expressed as the anchor's midpoint. The floating
/// layer's measured size is applied later as an offset, because the anchor
/// pass does not yet know that size.
pub fn anchor_origin(
    anchor: AnchorBox,
    placement: Placement,
    align: Alignment,
    gap: f32,
) -> AnchoredOrigin {
    let (x, y, corner) = match placement {
        Placement::Bottom => {
            let y = anchor.y + anchor.height + gap;
            match align {
                Alignment::Start => (anchor.x, y, Corner::TopLeft),
                Alignment::End => (anchor.x + anchor.width, y, Corner::TopRight),
                Alignment::Center => (anchor.x + anchor.width / 2.0, y, Corner::TopLeft),
            }
        }
        Placement::Top => {
            let y = anchor.y - gap;
            match align {
                Alignment::Start => (anchor.x, y, Corner::BottomLeft),
                Alignment::End => (anchor.x + anchor.width, y, Corner::BottomRight),
                Alignment::Center => (anchor.x + anchor.width / 2.0, y, Corner::BottomLeft),
            }
        }
        Placement::Right => {
            let x = anchor.x + anchor.width + gap;
            match align {
                Alignment::Start => (x, anchor.y, Corner::TopLeft),
                Alignment::End => (x, anchor.y + anchor.height, Corner::BottomLeft),
                Alignment::Center => (x, anchor.y + anchor.height / 2.0, Corner::TopLeft),
            }
        }
        Placement::Left => {
            let x = anchor.x - gap;
            match align {
                Alignment::Start => (x, anchor.y, Corner::TopRight),
                Alignment::End => (x, anchor.y + anchor.height, Corner::BottomRight),
                Alignment::Center => (x, anchor.y + anchor.height / 2.0, Corner::TopRight),
            }
        }
    };
    AnchoredOrigin { corner, x, y }
}

/// Horizontal offset that centers a layer of `layer_width` when alignment is
/// [`Alignment::Center`]. Other alignments stay put.
pub fn center_offset(
    align: Alignment,
    placement: Placement,
    layer_width: f32,
    layer_height: f32,
) -> (f32, f32) {
    if align != Alignment::Center {
        return (0.0, 0.0);
    }
    match placement {
        Placement::Top | Placement::Bottom => (-layer_width / 2.0, 0.0),
        Placement::Left | Placement::Right => (0.0, -layer_height / 2.0),
    }
}

/// What closing an overlay should do with keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CloseFocus {
    /// Focus is still inside the overlay, so return it to the trigger.
    Restore,
    /// Focus already moved somewhere else (Tab left, a field took the click).
    Keep,
}

/// Restore the trigger only while focus has not already left the overlay.
pub fn focus_after_close(focus_still_inside: bool) -> CloseFocus {
    if focus_still_inside {
        CloseFocus::Restore
    } else {
        CloseFocus::Keep
    }
}

/// How a dialog action participates in Enter, Escape, and initial focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DialogActionRole {
    /// Enter activates this action. Never combined with a destructive result.
    Default,
    /// Escape dismisses through this action.
    Cancel,
    /// Requires a deliberate choice. Enter does not activate it, and it is
    /// not the control focused when the dialog opens.
    Destructive,
    /// An extra action. Enter does not activate it.
    Normal,
}

/// Enter activates only the default action, never a destructive one.
///
/// Dialogs rely on this rule by focusing a non-destructive action, so the
/// focused button's own keyboard click is the safe one. The function stays
/// available to tests that pin the rule without a window.
#[cfg_attr(not(test), allow(dead_code))]
pub fn enter_activates(role: DialogActionRole) -> bool {
    matches!(role, DialogActionRole::Default)
}

/// Which action receives focus when a dialog opens.
///
/// A destructive action is never the initial focus. Cancel wins when one is
/// present alongside a destructive action, so the safe path is what Enter
/// and Space reach first.
pub fn initial_dialog_focus(roles: &[DialogActionRole]) -> Option<usize> {
    let destructive = roles.contains(&DialogActionRole::Destructive);
    if destructive {
        if let Some(index) = roles
            .iter()
            .position(|role| *role == DialogActionRole::Cancel)
        {
            return Some(index);
        }
        return roles
            .iter()
            .position(|role| *role != DialogActionRole::Destructive);
    }
    roles
        .iter()
        .position(|role| *role == DialogActionRole::Default)
        .or_else(|| {
            roles
                .iter()
                .position(|role| *role != DialogActionRole::Destructive)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_at() -> AnchorBox {
        AnchorBox {
            x: 10.0,
            y: 20.0,
            width: 40.0,
            height: 16.0,
        }
    }

    #[test]
    fn bottom_start_sits_just_under_the_anchor() {
        let origin = anchor_origin(box_at(), Placement::Bottom, Alignment::Start, 4.0);
        assert_eq!(origin.corner, Corner::TopLeft);
        assert_eq!(origin.x, 10.0);
        assert_eq!(origin.y, 40.0);
    }

    #[test]
    fn top_end_uses_the_bottom_right_corner() {
        let origin = anchor_origin(box_at(), Placement::Top, Alignment::End, 4.0);
        assert_eq!(origin.corner, Corner::BottomRight);
        assert_eq!(origin.x, 50.0);
        assert_eq!(origin.y, 16.0);
    }

    #[test]
    fn center_offset_shifts_by_half_the_layer() {
        assert_eq!(
            center_offset(Alignment::Center, Placement::Bottom, 80.0, 20.0),
            (-40.0, 0.0)
        );
        assert_eq!(
            center_offset(Alignment::Start, Placement::Bottom, 80.0, 20.0),
            (0.0, 0.0)
        );
        assert_eq!(
            center_offset(Alignment::Center, Placement::Right, 80.0, 20.0),
            (0.0, -10.0)
        );
    }

    #[test]
    fn close_restores_only_while_focus_remains_inside() {
        assert_eq!(focus_after_close(true), CloseFocus::Restore);
        assert_eq!(focus_after_close(false), CloseFocus::Keep);
    }

    #[test]
    fn destructive_dialog_focuses_cancel_and_ignores_enter() {
        let roles = [DialogActionRole::Cancel, DialogActionRole::Destructive];
        assert_eq!(initial_dialog_focus(&roles), Some(0));
        assert!(enter_activates(DialogActionRole::Default));
        assert!(!enter_activates(DialogActionRole::Destructive));
        assert!(!enter_activates(DialogActionRole::Cancel));
    }

    #[test]
    fn default_dialog_focuses_the_default_action() {
        let roles = [DialogActionRole::Cancel, DialogActionRole::Default];
        assert_eq!(initial_dialog_focus(&roles), Some(1));
    }
}
