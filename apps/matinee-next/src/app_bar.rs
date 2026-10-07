//! The bar across the top of every root screen: the wordmark, the root
//! destinations, the signed-in name, Refresh, and Sign out.
//!
//! Matinee presentation over Atelier buttons. Each root screen draws it and
//! turns the choices into its own events; the shell decides what they do.
//! Only destinations that exist natively are listed ([`RootDestination::BAR`]).

use atelier_ui::prelude::*;

use crate::nav::RootDestination;

/// What the bar shows.
pub(crate) struct AppBar {
    pub active: RootDestination,
    pub name: String,
    /// Refresh is unavailable while the screen is already loading.
    pub refresh_disabled: bool,
    pub signing_out: bool,
}

/// What the person chose in the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarAction {
    Go(RootDestination),
    Refresh,
    SignOut,
}

/// Draw the bar for a root screen `V`. `act` runs on the screen.
pub(crate) fn app_bar<V: 'static>(
    theme: &Theme,
    bar: AppBar,
    cx: &mut Context<V>,
    act: fn(&mut V, BarAction, &mut Window, &mut Context<V>),
) -> impl IntoElement {
    let mut links = h_stack(Space::S1).items_center();
    for (index, destination) in RootDestination::BAR.into_iter().enumerate() {
        let active = destination == bar.active;
        links = links.child(
            Button::new(("app-bar-go", index), destination.label())
                .variant(if active {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Subtle
                })
                .size(ButtonSize::Small)
                .on_click(cx.listener(move |this, _, window, cx| {
                    act(this, BarAction::Go(destination), window, cx);
                })),
        );
    }
    h_stack(Space::S3)
        .w_full()
        .items_center()
        .child(
            Text::new("MATINEE")
                .role(TextRole::Metadata)
                .color(theme.colors.control.accent),
        )
        .child(div().w(Space::S4.px()))
        .child(links)
        .child(div().flex_1())
        .child(
            Text::new(bar.name)
                .role(TextRole::Caption)
                .tone(TextTone::Secondary),
        )
        .child(
            Button::new("app-bar-refresh", "Refresh")
                .variant(ButtonVariant::Subtle)
                .size(ButtonSize::Small)
                .disabled(bar.refresh_disabled || bar.signing_out)
                .on_click(cx.listener(move |this, _, window, cx| {
                    act(this, BarAction::Refresh, window, cx);
                })),
        )
        .child(
            Button::new(
                "app-bar-sign-out",
                if bar.signing_out {
                    "Signing out…"
                } else {
                    "Sign out"
                },
            )
            .variant(ButtonVariant::Subtle)
            .size(ButtonSize::Small)
            .loading(bar.signing_out)
            .show_label_while_loading(true)
            .on_click(cx.listener(move |this, _, window, cx| {
                act(this, BarAction::SignOut, window, cx);
            })),
        )
}
