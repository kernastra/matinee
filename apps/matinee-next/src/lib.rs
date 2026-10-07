//! Native Matinee application.
//!
//! The GPUI window talks to one service runtime. That runtime owns Jellyfin
//! HTTP, playback reporting, artwork fetches, and vault work. Login, Home,
//! Library, Details, and the Player are the screens in this crate. Search,
//! Calendar, Settings, and Poster Studio are not yet.
//!
//! # Artwork
//!
//! Screens ask [`artwork::ArtworkLoader`] for a
//! [`matinee_jellyfin::ArtworkRequest`]. The loader fetches it on the
//! service runtime with [`matinee_jellyfin::Session::authorization_header`]
//! and decodes it off the UI thread. Do not put `api_key` back on artwork
//! URLs.

mod app_bar;
mod artwork;
mod details;
mod home;
mod library;
mod media_grid;
mod model;
mod nav;
mod player;
mod runtime;
mod search;
mod session;
mod store;
#[cfg(test)]
mod test_support;
mod tiles;
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
    let size = if let Some(scene) = review {
        review_size(scene.size())
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
        // Closing the last window on Linux and Windows quits after the window,
        // and its Player, are already gone. This drain covers the stop report
        // that window close started. The deadline is shared with the window's
        // own quit handler, so exit waits at most `FINAL_WORK_BOUND` in total.
        let runtime = services.runtime();
        cx.on_app_quit(move |_| {
            runtime.drain_final();
            async {}
        })
        .detach();
    });
    // Linux and Windows return here; macOS exits inside the quit handlers.
    // Window tasks have been aborted with their views. Dropping the last
    // handle drains final work (already done by an orderly exit), then shuts
    // the service runtime down and cancels anything still in flight.
    drop(runtime);
}

fn review_scene() -> Option<ReviewScene> {
    match std::env::var("MATINEE_PREVIEW").ok().as_deref() {
        Some("login") => Some(ReviewScene::Login),
        Some("focus") => Some(ReviewScene::Focus),
        Some("warning") => Some(ReviewScene::Warning),
        Some("error") => Some(ReviewScene::Error),
        Some("loading") => Some(ReviewScene::Loading),
        Some("home") => Some(ReviewScene::Home),
        Some("home-continue-watching") => Some(ReviewScene::HomeContinueWatching),
        Some("home-empty") => Some(ReviewScene::HomeEmpty),
        Some("home-partial-error") => Some(ReviewScene::HomePartialError),
        Some("home-loading") => Some(ReviewScene::HomeLoading),
        Some("home-small-window") => Some(ReviewScene::HomeSmallWindow),
        Some("home-large-window") => Some(ReviewScene::HomeLargeWindow),
        Some("home-hero-resume") => Some(ReviewScene::HomeHeroResume),
        Some("home-hero-fresh") => Some(ReviewScene::HomeHeroFresh),
        Some("player-playing") => Some(ReviewScene::PlayerPlaying),
        Some("player-paused") => Some(ReviewScene::PlayerPaused),
        Some("player-controls") => Some(ReviewScene::PlayerControls),
        Some("player-error") => Some(ReviewScene::PlayerError),
        Some("player-audio") => Some(ReviewScene::PlayerAudio),
        Some("player-subtitles") => Some(ReviewScene::PlayerSubtitles),
        Some("details-movie") => Some(ReviewScene::DetailsMovie),
        Some("details-movie-resume") => Some(ReviewScene::DetailsMovieResume),
        Some("details-series") => Some(ReviewScene::DetailsSeries),
        Some("details-season") => Some(ReviewScene::DetailsSeason),
        Some("details-loading") => Some(ReviewScene::DetailsLoading),
        Some("details-error") => Some(ReviewScene::DetailsError),
        Some("library") => Some(ReviewScene::Library),
        Some("library-movies") => Some(ReviewScene::LibraryMovies),
        Some("library-series") => Some(ReviewScene::LibrarySeries),
        Some("library-loading") => Some(ReviewScene::LibraryLoading),
        Some("library-empty") => Some(ReviewScene::LibraryEmpty),
        Some("library-filtered-empty") => Some(ReviewScene::LibraryFilteredEmpty),
        Some("library-partial-page") => Some(ReviewScene::LibraryPartialPage),
        Some("library-error") => Some(ReviewScene::LibraryError),
        Some("library-small-window") => Some(ReviewScene::LibrarySmallWindow),
        Some("library-large-window") => Some(ReviewScene::LibraryLargeWindow),
        Some("library-many-items") => Some(ReviewScene::LibraryManyItems),
        _ => None,
    }
}

/// `MATINEE_PREVIEW_SIZE=WxH`, or the scene's own size.
fn review_size(default: (f32, f32)) -> (f32, f32) {
    let Ok(raw) = std::env::var("MATINEE_PREVIEW_SIZE") else {
        return default;
    };
    let Some((width, height)) = raw.split_once('x') else {
        return default;
    };
    (
        width.parse().unwrap_or(default.0),
        height.parse().unwrap_or(default.1),
    )
}
