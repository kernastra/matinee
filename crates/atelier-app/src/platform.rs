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

    /// GPUI modifier name for the primary shortcut key.
    pub const fn primary_key(self) -> &'static str {
        match self.primary_modifier() {
            PrimaryModifier::Command => "cmd",
            PrimaryModifier::Control => "ctrl",
        }
    }

    /// GPUI modifier name for word movement. Option on macOS, Control elsewhere.
    pub const fn word_key(self) -> &'static str {
        match self {
            Platform::MacOS => "alt",
            Platform::Windows | Platform::Linux => "ctrl",
        }
    }

    /// macOS text fields also accept Ctrl-A/E and ⌘←/⌘→ as line boundaries.
    pub const fn uses_emacs_line_editing(self) -> bool {
        matches!(self, Platform::MacOS)
    }

    /// macOS character palette (Ctrl-⌘-Space).
    pub const fn has_character_palette(self) -> bool {
        matches!(self, Platform::MacOS)
    }

    /// OS reduced-motion signal.
    ///
    /// `Some(true)` means the OS asked for reduced motion, `Some(false)`
    /// means it asked for full motion, and `None` means this host has no
    /// supported signal. Callers must not treat `None` as "full motion".
    ///
    /// Linux reads GNOME `enable-animations` and, if that is missing, KDE's
    /// `AnimationDurationFactor` (0 means reduced). Xfce, Sway, and other
    /// desktops are unsupported. macOS reads `com.apple.universalaccess`
    /// `reduceMotion`; a missing key is full motion. Windows currently asks
    /// `SystemParametersInfo(SPI_GETCLIENTAREAANIMATION)` by spawning
    /// PowerShell. That shell-out is temporary platform technical debt: the
    /// replacement should call the Win32 API directly, without a process.
    /// GPUI 0.2.2 has no reduced-motion API, and these probes are not live
    /// subscriptions.
    pub fn detect_reduced_motion(self) -> Option<bool> {
        match self {
            Platform::MacOS => detect_macos(),
            Platform::Windows => detect_windows(),
            Platform::Linux => detect_linux(),
        }
    }
}

/// `enable-animations` true means full motion.
pub fn reduced_from_animations_enabled(enabled: bool) -> bool {
    !enabled
}

/// Parses `gsettings get org.gnome.desktop.interface enable-animations`.
pub fn parse_gsettings_bool(output: &str) -> Option<bool> {
    match unquote(output) {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// KDE `AnimationDurationFactor`. Zero means reduced motion.
pub fn parse_kde_animation_factor(output: &str) -> Option<bool> {
    let value: f32 = unquote(output).parse().ok()?;
    Some(value == 0.0)
}

/// `defaults read` output. A missing key is full motion (`Some(false)` reduced).
pub fn parse_defaults_reduce_motion(output: &str) -> Option<bool> {
    let text = output.trim();
    if text.contains("does not exist") {
        return Some(false);
    }
    match unquote(text) {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// `SystemParametersInfo` printed `1` when client-area animation is enabled.
pub fn parse_client_area_animation(output: &str) -> Option<bool> {
    match unquote(output) {
        "1" => Some(false),
        "0" => Some(true),
        _ => None,
    }
}

fn unquote(value: &str) -> &str {
    value.trim().trim_matches('\'').trim_matches('"')
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        return Some(stdout.into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.trim().is_empty() {
        None
    } else {
        Some(stderr.into_owned())
    }
}

fn detect_linux() -> Option<bool> {
    if let Some(text) = command_output(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "enable-animations"],
    ) && let Some(enabled) = parse_gsettings_bool(&text)
    {
        return Some(reduced_from_animations_enabled(enabled));
    }
    for program in ["kreadconfig6", "kreadconfig5"] {
        if let Some(text) = command_output(
            program,
            &[
                "--file",
                "kdeglobals",
                "--group",
                "KDE",
                "--key",
                "AnimationDurationFactor",
            ],
        ) && let Some(reduced) = parse_kde_animation_factor(&text)
        {
            return Some(reduced);
        }
    }
    None
}

fn detect_macos() -> Option<bool> {
    let text = command_output(
        "defaults",
        &["read", "com.apple.universalaccess", "reduceMotion"],
    )?;
    parse_defaults_reduce_motion(&text)
}

/// Temporary. Spawning PowerShell is platform technical debt. Replace this
/// with a direct `SystemParametersInfo(SPI_GETCLIENTAREAANIMATION)` call
/// when that can be done without `unsafe` or a new platform crate.
fn detect_windows() -> Option<bool> {
    let script = r#"
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class AtelierSpi { [DllImport("user32.dll")] public static extern bool SystemParametersInfo(uint a, uint b, ref bool v, uint f); }'
$enabled = $true
[void][AtelierSpi]::SystemParametersInfo(0x1042, 0, [ref]$enabled, 0)
if ($enabled) { '1' } else { '0' }
"#;
    let text = command_output(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    )?;
    parse_client_area_animation(&text)
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
        assert_eq!(Platform::MacOS.word_key(), "alt");
        assert_eq!(Platform::Linux.word_key(), "ctrl");
        assert!(Platform::MacOS.uses_emacs_line_editing());
        assert!(!Platform::Windows.uses_emacs_line_editing());
    }

    #[test]
    fn reduced_motion_parsers() {
        assert_eq!(parse_gsettings_bool("true\n"), Some(true));
        assert_eq!(parse_gsettings_bool("'false'"), Some(false));
        assert_eq!(parse_gsettings_bool("no"), None);
        assert!(reduced_from_animations_enabled(false));
        assert!(!reduced_from_animations_enabled(true));
        assert_eq!(parse_kde_animation_factor("0"), Some(true));
        assert_eq!(parse_kde_animation_factor("0.0"), Some(true));
        assert_eq!(parse_kde_animation_factor("1"), Some(false));
        assert_eq!(parse_kde_animation_factor(""), None);
        assert_eq!(
            parse_defaults_reduce_motion(
                "The domain/default pair of (com.apple.universalaccess, reduceMotion) does not exist"
            ),
            Some(false)
        );
        assert_eq!(parse_defaults_reduce_motion("1"), Some(true));
        assert_eq!(parse_defaults_reduce_motion("0"), Some(false));
        assert_eq!(parse_client_area_animation("1"), Some(false));
        assert_eq!(parse_client_area_animation("0"), Some(true));
    }
}
