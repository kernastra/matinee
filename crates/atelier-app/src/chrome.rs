//! Platform window chrome resolved from an application's intent.
//!
//! Applications say whether they want the platform default, the native
//! titlebar, or a unified titlebar. Only macOS draws content in the titlebar
//! band. That occupancy is separate from whether a client region can drag
//! the window. Windows keeps the system caption so Snap layouts stay intact.
//! Linux keeps server-side decorations.

use crate::platform::Platform;

/// What the application asked for. This is not a GPUI window option.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChromeIntent {
    /// macOS uses the unified titlebar. The system titlebar still moves the
    /// window; Atelier does not add a client drag region. Windows and Linux
    /// stay on the system frame.
    #[default]
    PlatformDefault,
    /// The operating system's own titlebar and caption, on every platform.
    Native,
    /// Content drawn in the titlebar band. Honored on macOS only.
    Unified,
}

/// Who paints the window frame. Client decorations are not requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecorationSource {
    Server,
}

/// What GPUI 0.2.2 can do with a client region, before chrome chooses whether
/// to use it. macOS drag is absent. Windows and Linux can drag a client
/// region, and the current strategy does not, because the system frame does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientRegionCapabilities {
    /// `WindowControlArea::Drag` (Windows) or `start_window_move` (X11 and Wayland).
    pub drag: bool,
    /// `Window::titlebar_double_click` performs the system titlebar action.
    pub double_click: bool,
}

/// GPUI 0.2.2 client-region support. This is not the resolved chrome.
pub fn client_region_capabilities(platform: Platform) -> ClientRegionCapabilities {
    match platform {
        // `start_window_move` is the empty trait default. The macOS hit-test
        // callback is ignored. `titlebar_double_click` reads
        // `AppleActionOnDoubleClick`.
        Platform::MacOS => ClientRegionCapabilities {
            drag: false,
            double_click: true,
        },
        // `WM_NCHITTEST` returns `HTCAPTION` for a drag area. Double-click is
        // the caption's, not a GPUI client call.
        Platform::Windows => ClientRegionCapabilities {
            drag: true,
            double_click: false,
        },
        // `start_window_move` is implemented on X11 and Wayland.
        Platform::Linux => ClientRegionCapabilities {
            drag: true,
            double_click: false,
        },
    }
}

/// The chrome `open_window` applies, plus the insets a view needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedChrome {
    pub decorations: DecorationSource,
    /// macOS full-size content view with a transparent titlebar.
    pub transparent_titlebar: bool,
    /// Top-left of the native traffic lights, in logical pixels. macOS only.
    pub traffic_light: Option<(f32, f32)>,
    /// Space at the leading edge of the titlebar band so content clears native controls.
    pub leading_inset: f32,
    /// Height of the in-client titlebar band. Zero when the OS titlebar is outside the client.
    pub band_height: f32,
    /// Content is laid out in the titlebar band. This does not make the band a drag surface.
    pub occupies_titlebar: bool,
    /// An empty client region moves the window. False when GPUI cannot drag
    /// (macOS) and false when the system frame is the drag surface.
    pub client_drag: bool,
    /// A double-click on an empty in-band region calls `Window::titlebar_double_click`.
    /// That is the system titlebar action, not a drag.
    pub forwards_titlebar_double_click: bool,
}

/// Logical size of the macOS unified band. Traffic lights are about 14pt tall,
/// so a 12pt origin centers them in a 38pt band. Three buttons plus gaps need
/// about 78pt before application content.
const MACOS_BAND_HEIGHT: f32 = 38.0;
const MACOS_LEADING_INSET: f32 = 78.0;
const MACOS_TRAFFIC_LIGHT: (f32, f32) = (12.0, 12.0);

pub fn resolve_chrome(platform: Platform, intent: ChromeIntent) -> ResolvedChrome {
    let occupies_titlebar = match (platform, intent) {
        (Platform::MacOS, ChromeIntent::PlatformDefault | ChromeIntent::Unified) => true,
        (Platform::MacOS, ChromeIntent::Native) => false,
        (Platform::Windows | Platform::Linux, ChromeIntent::PlatformDefault) => false,
        (Platform::Windows | Platform::Linux, ChromeIntent::Native) => false,
        (Platform::Windows | Platform::Linux, ChromeIntent::Unified) => false,
    };
    let caps = client_region_capabilities(platform);
    if occupies_titlebar {
        ResolvedChrome {
            decorations: DecorationSource::Server,
            transparent_titlebar: true,
            traffic_light: Some(MACOS_TRAFFIC_LIGHT),
            leading_inset: MACOS_LEADING_INSET,
            band_height: MACOS_BAND_HEIGHT,
            occupies_titlebar: true,
            client_drag: caps.drag,
            forwards_titlebar_double_click: caps.double_click,
        }
    } else {
        ResolvedChrome {
            decorations: DecorationSource::Server,
            transparent_titlebar: false,
            traffic_light: None,
            leading_inset: 0.0,
            band_height: 0.0,
            occupies_titlebar: false,
            client_drag: false,
            forwards_titlebar_double_click: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_default_matches_unified_and_does_not_claim_client_drag() {
        for intent in [ChromeIntent::PlatformDefault, ChromeIntent::Unified] {
            let chrome = resolve_chrome(Platform::MacOS, intent);
            assert!(chrome.transparent_titlebar);
            assert!(chrome.occupies_titlebar);
            assert_eq!(chrome.traffic_light, Some((12.0, 12.0)));
            assert!(chrome.leading_inset > 0.0);
            assert!(chrome.band_height > 0.0);
            assert!(!chrome.client_drag);
            assert!(chrome.forwards_titlebar_double_click);
            assert_eq!(chrome.decorations, DecorationSource::Server);
        }
    }

    #[test]
    fn macos_cannot_drag_a_client_region() {
        let caps = client_region_capabilities(Platform::MacOS);
        assert!(!caps.drag);
        assert!(caps.double_click);
    }

    #[test]
    fn macos_native_keeps_the_system_titlebar_outside_the_client() {
        let chrome = resolve_chrome(Platform::MacOS, ChromeIntent::Native);
        assert!(!chrome.transparent_titlebar);
        assert!(!chrome.occupies_titlebar);
        assert_eq!(chrome.leading_inset, 0.0);
        assert_eq!(chrome.band_height, 0.0);
        assert!(!chrome.client_drag);
        assert!(!chrome.forwards_titlebar_double_click);
        assert!(chrome.traffic_light.is_none());
    }

    #[test]
    fn windows_keeps_the_native_caption_for_every_intent() {
        assert!(client_region_capabilities(Platform::Windows).drag);
        for intent in [
            ChromeIntent::PlatformDefault,
            ChromeIntent::Native,
            ChromeIntent::Unified,
        ] {
            let chrome = resolve_chrome(Platform::Windows, intent);
            assert!(!chrome.transparent_titlebar, "{intent:?}");
            assert!(!chrome.occupies_titlebar);
            assert_eq!(chrome.band_height, 0.0);
            assert!(!chrome.client_drag);
            assert!(!chrome.forwards_titlebar_double_click);
            assert_eq!(chrome.decorations, DecorationSource::Server);
        }
    }

    #[test]
    fn linux_stays_on_server_decorations() {
        assert!(client_region_capabilities(Platform::Linux).drag);
        assert!(!client_region_capabilities(Platform::Linux).double_click);
        for intent in [
            ChromeIntent::PlatformDefault,
            ChromeIntent::Native,
            ChromeIntent::Unified,
        ] {
            let chrome = resolve_chrome(Platform::Linux, intent);
            assert!(!chrome.transparent_titlebar, "{intent:?}");
            assert!(!chrome.occupies_titlebar);
            assert_eq!(chrome.leading_inset, 0.0);
            assert_eq!(chrome.decorations, DecorationSource::Server);
            assert!(!chrome.client_drag);
            assert!(!chrome.forwards_titlebar_double_click);
        }
    }
}
