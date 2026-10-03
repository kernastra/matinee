//! Platform window chrome resolved from an application's intent.
//!
//! Applications say whether they want the platform default, the native
//! titlebar, or a unified titlebar. Only macOS actually draws content under
//! the titlebar. Windows keeps the system caption so Snap layouts stay
//! intact. Linux keeps server-side decorations.

use crate::platform::Platform;

/// What the application asked for. This is not a GPUI window option.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChromeIntent {
    /// macOS uses a unified titlebar. Windows and Linux stay native.
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
    /// Empty regions in the band should drag and honor the platform double-click.
    pub content_drag: bool,
}

/// Logical size of the macOS unified band. Traffic lights are about 14pt tall,
/// so a 12pt origin centers them in a 38pt band. Three buttons plus gaps need
/// about 78pt before application content.
const MACOS_BAND_HEIGHT: f32 = 38.0;
const MACOS_LEADING_INSET: f32 = 78.0;
const MACOS_TRAFFIC_LIGHT: (f32, f32) = (12.0, 12.0);

pub fn resolve_chrome(platform: Platform, intent: ChromeIntent) -> ResolvedChrome {
    let unified = match (platform, intent) {
        (Platform::MacOS, ChromeIntent::PlatformDefault | ChromeIntent::Unified) => true,
        (Platform::MacOS, ChromeIntent::Native) => false,
        (Platform::Windows | Platform::Linux, ChromeIntent::PlatformDefault) => false,
        (Platform::Windows | Platform::Linux, ChromeIntent::Native) => false,
        (Platform::Windows | Platform::Linux, ChromeIntent::Unified) => false,
    };
    if unified {
        ResolvedChrome {
            decorations: DecorationSource::Server,
            transparent_titlebar: true,
            traffic_light: Some(MACOS_TRAFFIC_LIGHT),
            leading_inset: MACOS_LEADING_INSET,
            band_height: MACOS_BAND_HEIGHT,
            content_drag: true,
        }
    } else {
        ResolvedChrome {
            decorations: DecorationSource::Server,
            transparent_titlebar: false,
            traffic_light: None,
            leading_inset: 0.0,
            band_height: 0.0,
            content_drag: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_default_and_unified_use_native_traffic_lights() {
        for intent in [ChromeIntent::PlatformDefault, ChromeIntent::Unified] {
            let chrome = resolve_chrome(Platform::MacOS, intent);
            assert!(chrome.transparent_titlebar);
            assert_eq!(chrome.traffic_light, Some((12.0, 12.0)));
            assert!(chrome.leading_inset > 0.0);
            assert!(chrome.band_height > 0.0);
            assert!(chrome.content_drag);
            assert_eq!(chrome.decorations, DecorationSource::Server);
        }
    }

    #[test]
    fn macos_native_keeps_the_system_titlebar_outside_the_client() {
        let chrome = resolve_chrome(Platform::MacOS, ChromeIntent::Native);
        assert!(!chrome.transparent_titlebar);
        assert_eq!(chrome.leading_inset, 0.0);
        assert_eq!(chrome.band_height, 0.0);
        assert!(!chrome.content_drag);
        assert!(chrome.traffic_light.is_none());
    }

    #[test]
    fn windows_keeps_the_native_caption_for_every_intent() {
        for intent in [
            ChromeIntent::PlatformDefault,
            ChromeIntent::Native,
            ChromeIntent::Unified,
        ] {
            let chrome = resolve_chrome(Platform::Windows, intent);
            assert!(!chrome.transparent_titlebar, "{intent:?}");
            assert_eq!(chrome.band_height, 0.0);
            assert!(!chrome.content_drag);
            assert_eq!(chrome.decorations, DecorationSource::Server);
        }
    }

    #[test]
    fn linux_stays_on_server_decorations() {
        for intent in [
            ChromeIntent::PlatformDefault,
            ChromeIntent::Native,
            ChromeIntent::Unified,
        ] {
            let chrome = resolve_chrome(Platform::Linux, intent);
            assert!(!chrome.transparent_titlebar, "{intent:?}");
            assert_eq!(chrome.leading_inset, 0.0);
            assert_eq!(chrome.decorations, DecorationSource::Server);
            assert!(!chrome.content_drag);
        }
    }
}
