//! Search field.
//!
//! A [`TextField`] with a search icon, a clear button, and Escape behavior:
//! Escape clears a non-empty query, and dismisses when the query is already
//! empty. There is no product-specific search behavior here.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    Window, div,
};

use crate::components::{
    IconName, TextField,
    keybindings::{ClearOrDismiss, SEARCH_FIELD_CONTEXT, TEXT_FIELD_CONTEXT},
};

type ChangeHandler = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type DismissHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// A single-line search field.
#[derive(IntoElement)]
pub struct SearchField {
    id: ElementId,
    value: SharedString,
    placeholder: SharedString,
    disabled: bool,
    on_change: Option<ChangeHandler>,
    on_dismiss: Option<DismissHandler>,
    inspected: bool,
}

impl SearchField {
    pub fn new(id: impl Into<ElementId>, value: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            value: value.into(),
            placeholder: "Search".into(),
            disabled: false,
            on_change: None,
            on_dismiss: None,
            inspected: false,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(
        mut self,
        handler: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    /// Called when Escape is pressed and the field is already empty.
    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }

    pub fn inspected(mut self, inspected: bool) -> Self {
        self.inspected = inspected;
        self
    }
}

impl RenderOnce for SearchField {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let value = self.value.clone();
        let on_change = self.on_change.clone();
        let on_dismiss = self.on_dismiss.clone();
        let clear = on_change.clone();

        let mut field = TextField::new(self.id.clone(), value.clone())
            .placeholder(self.placeholder)
            .disabled(self.disabled)
            .leading_icon(IconName::Search)
            .inspected(self.inspected)
            .inspection_name("Search field")
            .key_context(TEXT_FIELD_CONTEXT);
        if let Some(handler) = on_change.clone() {
            field = field.on_change(move |value, window, cx| handler(value, window, cx));
        }
        if !value.is_empty() && !self.disabled {
            field = field.trailing(IconName::Close, "Clear search", move |window, cx| {
                if let Some(clear) = clear.clone() {
                    clear(SharedString::default(), window, cx);
                }
            });
        }

        div()
            .id((self.id.clone(), "wrap"))
            .key_context(SEARCH_FIELD_CONTEXT)
            .when(!self.disabled, |this| {
                this.on_action(move |_: &ClearOrDismiss, window, cx| {
                    if value.is_empty() {
                        if let Some(dismiss) = on_dismiss.clone() {
                            dismiss(window, cx);
                        }
                    } else if let Some(change) = on_change.clone() {
                        change(SharedString::default(), window, cx);
                    }
                })
            })
            .child(field)
    }
}
