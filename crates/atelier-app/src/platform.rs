//! The single place where the framework branches on the host platform.
//! Components and applications ask *what convention applies*, never
//! *which OS is this*.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    MacOS,
    Windows,
    Linux,
}

/// The modifier that carries application shortcuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrimaryModifier {
    Command,
    Control,
}

impl Platform {
    pub const ALL: [Platform; 3] = [Platform::MacOS, Platform::Windows, Platform::Linux];

    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOS
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Platform::MacOS => "macOS",
            Platform::Windows => "Windows",
            Platform::Linux => "Linux",
        }
    }

    pub const fn primary_modifier(self) -> PrimaryModifier {
        match self {
            Platform::MacOS => PrimaryModifier::Command,
            Platform::Windows | Platform::Linux => PrimaryModifier::Control,
        }
    }

    /// macOS apps conventionally keep running with no windows open.
    pub const fn quits_when_last_window_closes(self) -> bool {
        !matches!(self, Platform::MacOS)
    }

    /// Whether the application menu lives in a global system menu bar.
    pub const fn has_global_menu_bar(self) -> bool {
        matches!(self, Platform::MacOS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_platform_matches_target() {
        let expected = if cfg!(target_os = "macos") {
            Platform::MacOS
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        };
        assert_eq!(Platform::current(), expected);
    }

    #[test]
    fn conventions() {
        assert_eq!(Platform::MacOS.primary_modifier(), PrimaryModifier::Command);
        assert_eq!(Platform::Linux.primary_modifier(), PrimaryModifier::Control);
        assert!(!Platform::MacOS.quits_when_last_window_closes());
        assert!(Platform::Windows.quits_when_last_window_closes());
        assert!(Platform::MacOS.has_global_menu_bar());
        assert!(!Platform::Linux.has_global_menu_bar());
    }
}
