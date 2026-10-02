//! Lightweight inspection snapshot for the Gallery.
//!
//! Components write it while rendering. It does not notify, so it cannot
//! schedule a frame. The Gallery reads it after the story has rendered.

use std::cell::RefCell;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspection {
    pub name: &'static str,
    pub focused: bool,
    pub value: String,
}

thread_local! {
    static CURRENT: RefCell<Option<Inspection>> = const { RefCell::new(None) };
}

pub fn clear_inspection() {
    CURRENT.with(|slot| *slot.borrow_mut() = None);
}

pub fn report_inspection(inspection: Inspection) {
    CURRENT.with(|slot| *slot.borrow_mut() = Some(inspection));
}

pub fn current_inspection() -> Option<Inspection> {
    CURRENT.with(|slot| slot.borrow().clone())
}
