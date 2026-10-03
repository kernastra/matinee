//! Window geometry in logical pixels.
//!
//! Placement, off-screen correction, and the on-disk frame are pure data so
//! they can be tested without opening a window. GPUI has no maximum-size
//! field. `placement_max` clamps the frame used for placement, restoration,
//! and the saved normal frame. A live resize past that size is kept on
//! screen and is not snapped back; the oversized frame is left unsaved.

use std::path::{Path, PathBuf};

/// A rectangle in logical pixels. Origin is the top-left of the global desktop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Normal frame plus whether the window should open maximized.
///
/// `width` and `height` are the restored windowed size, including while
/// `maximized` is set. They are never the fullscreen size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

/// How much of the top edge must sit on a display before a frame is considered reachable.
pub const TITLEBAR_GRAB: f32 = 48.0;

/// Inputs for choosing the frame a window opens with.
#[derive(Clone, Debug)]
pub struct PlacementRequest {
    pub saved: Option<WindowGeometry>,
    pub default_size: (f32, f32),
    pub min_size: (f32, f32),
    /// Placement and restoration limit. Not a live window maximum.
    pub placement_max: Option<(f32, f32)>,
    pub displays: Vec<LogicalRect>,
    /// When false, a saved origin is ignored and the window is centered.
    /// Wayland does not reliably restore positions.
    pub restore_origin: bool,
}

/// The frame to open, and whether that open should be maximized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub frame: LogicalRect,
    pub maximized: bool,
}

pub fn clamp_size(width: f32, height: f32, min: (f32, f32), max: Option<(f32, f32)>) -> (f32, f32) {
    let min_w = min.0.max(1.0);
    let min_h = min.1.max(1.0);
    let (max_w, max_h) = match max {
        Some((w, h)) => (w.max(min_w), h.max(min_h)),
        None => (f32::MAX, f32::MAX),
    };
    (width.clamp(min_w, max_w), height.clamp(min_h, max_h))
}

/// Largest intersection wins. Equal areas keep the earlier display.
pub fn select_display(window: LogicalRect, displays: &[LogicalRect]) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, display) in displays.iter().enumerate() {
        let area = intersection_area(window, *display);
        if area <= 0.0 {
            continue;
        }
        match best {
            Some((_, best_area)) if area <= best_area => {}
            _ => best = Some((index, area)),
        }
    }
    best.map(|(index, _)| index)
}

pub fn center_on(display: LogicalRect, width: f32, height: f32) -> LogicalRect {
    let width = width.min(display.width).max(1.0);
    let height = height.min(display.height).max(1.0);
    LogicalRect {
        x: display.x + (display.width - width) / 2.0,
        y: display.y + (display.height - height) / 2.0,
        width,
        height,
    }
}

/// Moves a frame fully onto the nearest display when its top edge is not reachable.
pub fn correct_offscreen(window: LogicalRect, displays: &[LogicalRect]) -> LogicalRect {
    if displays.is_empty() {
        return window;
    }
    if displays
        .iter()
        .any(|display| top_is_reachable(window, *display))
    {
        return window;
    }
    let target = select_display(window, displays)
        .map(|index| displays[index])
        .unwrap_or_else(|| nearest_display(window, displays));
    place_inside(window, target)
}

pub fn place(request: &PlacementRequest) -> Placement {
    let (raw_w, raw_h, saved_origin, maximized) = match request.saved {
        Some(saved) => (
            saved.width,
            saved.height,
            Some((saved.x, saved.y)),
            saved.maximized,
        ),
        None => (request.default_size.0, request.default_size.1, None, false),
    };
    let (width, height) = clamp_size(raw_w, raw_h, request.min_size, request.placement_max);
    let frame = match (
        saved_origin,
        request.restore_origin,
        request.displays.first(),
    ) {
        (Some((x, y)), true, _) => correct_offscreen(
            LogicalRect {
                x,
                y,
                width,
                height,
            },
            &request.displays,
        ),
        (_, _, Some(display)) => center_on(*display, width, height),
        _ => LogicalRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        },
    };
    Placement { frame, maximized }
}

/// Fullscreen must not replace the normal frame. A maximized window keeps the
/// last windowed size and only flips the flag.
pub fn next_saved_geometry(
    previous: WindowGeometry,
    observed: LogicalRect,
    maximized: bool,
    fullscreen: bool,
    min_size: (f32, f32),
    placement_max: Option<(f32, f32)>,
    displays: &[LogicalRect],
) -> WindowGeometry {
    if fullscreen || maximized {
        return WindowGeometry {
            maximized: maximized || (previous.maximized && fullscreen),
            ..previous
        };
    }
    // A fullscreen transition can report the display size before
    // `is_fullscreen` flips. A frame past `placement_max` is the same kind of
    // transient. Neither replaces the normal frame, and neither resizes the
    // live window.
    if frame_is_transient(observed, placement_max, displays) {
        return previous;
    }
    let (width, height) = clamp_size(observed.width, observed.height, min_size, placement_max);
    WindowGeometry {
        x: observed.x,
        y: observed.y,
        width,
        height,
        maximized: false,
    }
}

fn frame_is_transient(
    observed: LogicalRect,
    placement_max: Option<(f32, f32)>,
    displays: &[LogicalRect],
) -> bool {
    if let Some((max_width, max_height)) = placement_max
        && (observed.width > max_width + 1.0 || observed.height > max_height + 1.0)
    {
        return true;
    }
    displays.iter().any(|display| {
        (observed.width - display.width).abs() < 2.0
            && (observed.height - display.height).abs() < 2.0
    })
}

pub fn encode_geometry(geometry: &WindowGeometry) -> String {
    format!(
        "1\n{:.4} {:.4} {:.4} {:.4} {}\n",
        geometry.x,
        geometry.y,
        geometry.width,
        geometry.height,
        u8::from(geometry.maximized)
    )
}

pub fn decode_geometry(text: &str) -> Option<WindowGeometry> {
    let mut lines = text.lines();
    if lines.next()?.trim() != "1" {
        return None;
    }
    let mut parts = lines.next()?.split_whitespace();
    let x = parts.next()?.parse::<f32>().ok()?;
    let y = parts.next()?.parse::<f32>().ok()?;
    let width = parts.next()?.parse::<f32>().ok()?;
    let height = parts.next()?.parse::<f32>().ok()?;
    let maximized = match parts.next()? {
        "1" => true,
        "0" => false,
        _ => return None,
    };
    if parts.next().is_some()
        || !width.is_finite()
        || !height.is_finite()
        || width < 1.0
        || height < 1.0
    {
        return None;
    }
    Some(WindowGeometry {
        x,
        y,
        width,
        height,
        maximized,
    })
}

/// `app_id` and `key` become a single file name. Path separators and `..` are rejected.
pub fn restoration_file_name(app_id: &str, key: &str) -> Option<String> {
    fn token(value: &str) -> Option<&str> {
        if value.is_empty()
            || value.len() > 80
            || value == "."
            || value == ".."
            || !value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-')
        {
            return None;
        }
        Some(value)
    }
    Some(format!("{}.{}.window", token(app_id)?, token(key)?))
}

pub fn geometry_path(root: &Path, app_id: &str, key: &str) -> Option<PathBuf> {
    Some(root.join(restoration_file_name(app_id, key)?))
}

fn intersection_area(a: LogicalRect, b: LogicalRect) -> f32 {
    let width = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let height = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    width.max(0.0) * height.max(0.0)
}

fn top_is_reachable(window: LogicalRect, display: LogicalRect) -> bool {
    let left = window.x.max(display.x);
    let right = (window.x + window.width).min(display.x + display.width);
    let top = window.y.max(display.y);
    let bottom = (window.y + TITLEBAR_GRAB.min(window.height)).min(display.y + display.height);
    let width = right - left;
    let needed = TITLEBAR_GRAB.min(window.width).max(1.0);
    width >= needed * 0.5 && bottom - top > 1.0
}

fn nearest_display(window: LogicalRect, displays: &[LogicalRect]) -> LogicalRect {
    let cx = window.x + window.width / 2.0;
    let cy = window.y + window.height / 2.0;
    displays
        .iter()
        .copied()
        .min_by(|a, b| {
            let da = center_distance(*a, cx, cy);
            let db = center_distance(*b, cx, cy);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(window)
}

fn center_distance(display: LogicalRect, x: f32, y: f32) -> f32 {
    let dx = (display.x + display.width / 2.0) - x;
    let dy = (display.y + display.height / 2.0) - y;
    dx * dx + dy * dy
}

fn place_inside(window: LogicalRect, display: LogicalRect) -> LogicalRect {
    let width = window.width.min(display.width).max(1.0);
    let height = window.height.min(display.height).max(1.0);
    let max_x = display.x + display.width - width;
    let max_y = display.y + display.height - height;
    LogicalRect {
        x: window.x.clamp(display.x, max_x),
        y: window.y.clamp(display.y, max_y),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop() -> Vec<LogicalRect> {
        vec![LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        }]
    }

    fn request(saved: Option<WindowGeometry>, restore_origin: bool) -> PlacementRequest {
        PlacementRequest {
            saved,
            default_size: (800.0, 600.0),
            min_size: (400.0, 300.0),
            placement_max: Some((1000.0, 800.0)),
            displays: desktop(),
            restore_origin,
        }
    }

    #[test]
    fn clamp_size_honors_min_and_max() {
        assert_eq!(
            clamp_size(100.0, 100.0, (400.0, 300.0), Some((1000.0, 800.0))),
            (400.0, 300.0)
        );
        assert_eq!(
            clamp_size(2000.0, 2000.0, (400.0, 300.0), Some((1000.0, 800.0))),
            (1000.0, 800.0)
        );
        assert_eq!(
            clamp_size(500.0, 500.0, (400.0, 300.0), None),
            (500.0, 500.0)
        );
    }

    #[test]
    fn minimum_wins_when_maximum_is_smaller() {
        assert_eq!(
            clamp_size(10.0, 10.0, (400.0, 300.0), Some((100.0, 50.0))),
            (400.0, 300.0)
        );
    }

    #[test]
    fn visible_window_keeps_its_origin() {
        let placed = place(&request(
            Some(WindowGeometry {
                x: 40.0,
                y: 50.0,
                width: 800.0,
                height: 600.0,
                maximized: false,
            }),
            true,
        ));
        assert_eq!(placed.frame.x, 40.0);
        assert_eq!(placed.frame.y, 50.0);
        assert!(!placed.maximized);
    }

    #[test]
    fn offscreen_window_is_moved_onto_a_display() {
        let placed = place(&request(
            Some(WindowGeometry {
                x: -4000.0,
                y: -2000.0,
                width: 800.0,
                height: 600.0,
                maximized: false,
            }),
            true,
        ));
        assert!(placed.frame.x >= 0.0);
        assert!(placed.frame.y >= 0.0);
        assert!(placed.frame.x + placed.frame.width <= 1920.0);
        assert!(placed.frame.y + placed.frame.height <= 1080.0);
    }

    #[test]
    fn display_selection_prefers_the_largest_intersection() {
        let displays = [
            LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            LogicalRect {
                x: 1920.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
        ];
        let on_second = LogicalRect {
            x: 2000.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        };
        assert_eq!(select_display(on_second, &displays), Some(1));
        assert!(top_is_reachable(on_second, displays[1]));
        let corrected = correct_offscreen(on_second, &displays);
        assert_eq!(corrected.x, 2000.0);
    }

    #[test]
    fn ignored_origin_recenters_and_keeps_size() {
        let placed = place(&request(
            Some(WindowGeometry {
                x: 20.0,
                y: 30.0,
                width: 900.0,
                height: 500.0,
                maximized: true,
            }),
            false,
        ));
        assert_eq!(placed.frame.width, 900.0);
        assert_eq!(placed.frame.height, 500.0);
        assert!((placed.frame.x - (1920.0 - 900.0) / 2.0).abs() < 0.1);
        assert!(placed.maximized);
    }

    #[test]
    fn fullscreen_does_not_replace_the_normal_frame() {
        let previous = WindowGeometry {
            x: 10.0,
            y: 20.0,
            width: 800.0,
            height: 600.0,
            maximized: false,
        };
        let next = next_saved_geometry(
            previous,
            LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            false,
            true,
            (400.0, 300.0),
            None,
            &[],
        );
        assert_eq!(next, previous);
    }

    #[test]
    fn maximize_keeps_the_windowed_size() {
        let previous = WindowGeometry {
            x: 10.0,
            y: 20.0,
            width: 800.0,
            height: 600.0,
            maximized: false,
        };
        let next = next_saved_geometry(
            previous,
            LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            true,
            false,
            (400.0, 300.0),
            None,
            &[],
        );
        assert!(next.maximized);
        assert_eq!(next.width, 800.0);
        assert_eq!(next.x, 10.0);
    }

    #[test]
    fn display_sized_frame_does_not_replace_the_normal_frame() {
        let previous = WindowGeometry {
            x: 40.0,
            y: 60.0,
            width: 720.0,
            height: 480.0,
            maximized: false,
        };
        let display = LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1200.0,
        };
        let next = next_saved_geometry(
            previous,
            LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1200.0,
            },
            false,
            false,
            (720.0, 480.0),
            Some((1600.0, 1000.0)),
            &[display],
        );
        assert_eq!(next, previous);
        let over_max = next_saved_geometry(
            previous,
            LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 1700.0,
                height: 1100.0,
            },
            false,
            false,
            (720.0, 480.0),
            Some((1600.0, 1000.0)),
            &[display],
        );
        assert_eq!(over_max, previous);
        let resized = next_saved_geometry(
            previous,
            LogicalRect {
                x: 80.0,
                y: 90.0,
                width: 1000.0,
                height: 700.0,
            },
            false,
            false,
            (720.0, 480.0),
            Some((1600.0, 1000.0)),
            &[display],
        );
        assert_eq!(resized.width, 1000.0);
        assert_eq!(resized.height, 700.0);
        assert_eq!(resized.x, 80.0);
    }

    #[test]
    fn geometry_codec_roundtrips() {
        let geometry = WindowGeometry {
            x: 12.5,
            y: 8.0,
            width: 800.0,
            height: 600.0,
            maximized: true,
        };
        let decoded = decode_geometry(&encode_geometry(&geometry)).unwrap();
        assert_eq!(decoded.x, 12.5);
        assert_eq!(decoded.width, 800.0);
        assert!(decoded.maximized);
        assert!(decode_geometry("nope").is_none());
        assert!(decode_geometry("1\n1 2 0 10 0\n").is_none());
    }

    #[test]
    fn restoration_names_reject_paths() {
        assert_eq!(
            restoration_file_name("dev.example.App", "main-window"),
            Some("dev.example.App.main-window.window".into())
        );
        assert!(restoration_file_name("../etc", "main").is_none());
        assert!(restoration_file_name("app", "a/b").is_none());
        assert!(restoration_file_name("app", "..").is_none());
        assert!(geometry_path(Path::new("/tmp/state"), "app", "main").is_some());
    }
}
