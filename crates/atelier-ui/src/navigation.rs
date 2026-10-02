//! Linear keyboard movement shared by lists, sidebars, and menus.
//!
//! The three controls are not one widget. They share only the arithmetic of
//! moving a cursor through a disabled mask: next, previous, first, and last,
//! without wrapping. Selection, activation, and dismissal stay with the
//! control that owns them.

/// A step along a linear collection of rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavStep {
    Previous,
    Next,
    First,
    Last,
}

/// Move from `current` to the next enabled index. `None` starts from the
/// corresponding end (the item before the first, or after the last).
/// Disabled entries are skipped. Movement does not wrap.
pub fn move_enabled(disabled: &[bool], current: Option<usize>, step: NavStep) -> Option<usize> {
    if disabled.is_empty() {
        return None;
    }
    match step {
        NavStep::First => first_enabled(disabled),
        NavStep::Last => last_enabled(disabled),
        NavStep::Next => step_from(disabled, current, 1),
        NavStep::Previous => step_from(disabled, current, -1),
    }
}

fn first_enabled(disabled: &[bool]) -> Option<usize> {
    disabled.iter().position(|disabled| !disabled)
}

fn last_enabled(disabled: &[bool]) -> Option<usize> {
    disabled.iter().rposition(|disabled| !disabled)
}

fn step_from(disabled: &[bool], current: Option<usize>, dir: i32) -> Option<usize> {
    let mut index = match current {
        Some(index) => index as i32,
        None => {
            if dir > 0 {
                -1
            } else {
                disabled.len() as i32
            }
        }
    };
    loop {
        index += dir;
        if index < 0 || index >= disabled.len() as i32 {
            return current.filter(|index| *index < disabled.len());
        }
        if !disabled[index as usize] {
            return Some(index as usize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_skips_disabled_and_does_not_wrap() {
        let disabled = [false, true, false, true];
        assert_eq!(move_enabled(&disabled, None, NavStep::First), Some(0));
        assert_eq!(move_enabled(&disabled, None, NavStep::Last), Some(2));
        assert_eq!(move_enabled(&disabled, Some(0), NavStep::Next), Some(2));
        assert_eq!(move_enabled(&disabled, Some(2), NavStep::Next), Some(2));
        assert_eq!(move_enabled(&disabled, Some(2), NavStep::Previous), Some(0));
        assert_eq!(move_enabled(&disabled, Some(0), NavStep::Previous), Some(0));
    }

    #[test]
    fn empty_and_all_disabled_have_nowhere_to_go() {
        assert_eq!(move_enabled(&[], None, NavStep::Next), None);
        assert_eq!(move_enabled(&[true, true], None, NavStep::First), None);
        assert_eq!(move_enabled(&[true, true], Some(0), NavStep::Next), Some(0));
    }
}
