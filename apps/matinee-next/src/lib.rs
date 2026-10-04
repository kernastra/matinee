//! Native Matinee application.
//!
//! The GPUI window talks to one service runtime. That runtime owns Jellyfin
//! HTTP and vault work. Login is the first screen. Library screens are not
//! in this crate yet.
//!
//! # Artwork
//!
//! Login does not load Jellyfin artwork. The next screen that does must build
//! an [`matinee_jellyfin::ArtworkRequest`] and send
//! [`matinee_jellyfin::Session::authorization_header`] from the service
//! runtime. Do not put `api_key` back on artwork URLs.

mod model;
mod runtime;
mod session;
mod store;
mod view;
mod warning;

use std::sync::Arc;

use atelier_app::{AppInfo, AtelierApp};
use matinee_ui::matinee_theme;

use crate::model::ReviewScene;
use crate::view::{Services, open_matinee};

pub fn run() {
    let review = review_scene();
    let services = if review.is_some() {
        Services::memory().expect("service runtime")
    } else {
        Services::production().expect("service runtime")
    };
    let runtime = Arc::clone(&services.runtime());
    let size = if review.is_some() {
        review_size()
    } else {
        (1200.0, 760.0)
    };
    AtelierApp::new(AppInfo {
        name: "Matinee",
        app_id: "dev.sean.matinee.next",
    })
    .theme(matinee_theme())
    .run(move |cx| {
        if let Err(error) = matinee_ui::load_bundled_fonts(cx) {
            eprintln!("failed to load Matinee fonts: {error}");
        }
        open_matinee(cx, services.clone(), review, size);
    });
    // Window tasks have been aborted with their views. Dropping the last
    // handle shuts the service runtime down and cancels anything still in flight.
    drop(runtime);
}

fn review_scene() -> Option<ReviewScene> {
    match std::env::var("MATINEE_PREVIEW").ok().as_deref() {
        Some("login") => Some(ReviewScene::Login),
        Some("focus") => Some(ReviewScene::Focus),
        Some("warning") => Some(ReviewScene::Warning),
        Some("error") => Some(ReviewScene::Error),
        Some("loading") => Some(ReviewScene::Loading),
        Some("shell") => Some(ReviewScene::Shell),
        _ => None,
    }
}

fn review_size() -> (f32, f32) {
    let Ok(raw) = std::env::var("MATINEE_PREVIEW_SIZE") else {
        return (1200.0, 760.0);
    };
    let Some((width, height)) = raw.split_once('x') else {
        return (1200.0, 760.0);
    };
    (
        width.parse().unwrap_or(1200.0),
        height.parse().unwrap_or(760.0),
    )
}
