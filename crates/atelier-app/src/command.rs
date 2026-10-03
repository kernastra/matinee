//! Application commands and their platform-adapted shortcuts and labels.
//!
//! Apps and menus refer to a [`Command`]; the keystroke that triggers it is
//! decided here, once, per platform (e.g. Settings is ⌘, on macOS and
//! Ctrl+, elsewhere).

use gpui::{Action, actions};

use crate::platform::{Platform, PrimaryModifier};

actions!(
    atelier,
    [
        Quit,
        CloseWindow,
        Minimize,
        ToggleFullScreen,
        OpenSettings,
        Hide,
        HideOthers,
        Undo,
        Redo,
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    Quit,
    CloseWindow,
    Minimize,
    ToggleFullScreen,
    OpenSettings,
    Hide,
    HideOthers,
}

/// A platform-neutral key chord. `primary` resolves to ⌘ on macOS and Ctrl
/// elsewhere; `control` is a literal Control key (used for ⌃⌘F on macOS).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub primary: bool,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    /// Key name in GPUI keystroke syntax (`"q"`, `","`, `"f11"`).
    pub key: &'static str,
}

impl Shortcut {
    const fn primary(key: &'static str) -> Self {
        Self {
            primary: true,
            control: false,
            alt: false,
            shift: false,
            key,
        }
    }

    const fn bare(key: &'static str) -> Self {
        Self {
            primary: false,
            control: false,
            alt: false,
            shift: false,
            key,
        }
    }

    const fn with_control(mut self) -> Self {
        self.control = true;
        self
    }

    const fn with_alt(mut self) -> Self {
        self.alt = true;
        self
    }

    fn uses_ctrl(self, platform: Platform) -> bool {
        self.control || (self.primary && platform.primary_modifier() == PrimaryModifier::Control)
    }

    fn uses_cmd(self, platform: Platform) -> bool {
        self.primary && platform.primary_modifier() == PrimaryModifier::Command
    }

    /// The keystroke string GPUI's keymap understands, e.g. `"cmd-,"`.
    pub fn keystroke(self, platform: Platform) -> String {
        let mut parts = Vec::new();
        if self.uses_ctrl(platform) {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        if self.uses_cmd(platform) {
            parts.push("cmd");
        }
        parts.push(self.key);
        parts.join("-")
    }

    /// Human-readable label following platform conventions:
    /// `⌃⌘F` on macOS, `Ctrl+Shift+F` elsewhere.
    pub fn label(self, platform: Platform) -> String {
        let key = self.key.to_uppercase();
        if platform == Platform::MacOS {
            let mut s = String::new();
            if self.uses_ctrl(platform) {
                s.push('⌃');
            }
            if self.alt {
                s.push('⌥');
            }
            if self.shift {
                s.push('⇧');
            }
            if self.uses_cmd(platform) {
                s.push('⌘');
            }
            s.push_str(&key);
            s
        } else {
            let mut parts: Vec<String> = Vec::new();
            if self.uses_ctrl(platform) {
                parts.push("Ctrl".into());
            }
            if self.alt {
                parts.push("Alt".into());
            }
            if self.shift {
                parts.push("Shift".into());
            }
            parts.push(key);
            parts.join("+")
        }
    }
}

impl Command {
    pub const ALL: [Command; 7] = [
        Command::Quit,
        Command::CloseWindow,
        Command::Minimize,
        Command::ToggleFullScreen,
        Command::OpenSettings,
        Command::Hide,
        Command::HideOthers,
    ];

    /// The shortcut for this command, or `None` where the platform (window
    /// manager / OS) owns the gesture and the app should not bind it.
    pub const fn shortcut(self, platform: Platform) -> Option<Shortcut> {
        match (self, platform) {
            // Windows apps conventionally quit via Alt+F4 (OS-provided).
            (Command::Quit, Platform::Windows) => None,
            (Command::Quit, _) => Some(Shortcut::primary("q")),
            (Command::CloseWindow, _) => Some(Shortcut::primary("w")),
            (Command::Minimize, Platform::MacOS) => Some(Shortcut::primary("m")),
            (Command::Minimize, _) => None,
            (Command::ToggleFullScreen, Platform::MacOS) => {
                Some(Shortcut::primary("f").with_control())
            }
            (Command::ToggleFullScreen, _) => Some(Shortcut::bare("f11")),
            (Command::OpenSettings, _) => Some(Shortcut::primary(",")),
            (Command::Hide, Platform::MacOS) => Some(Shortcut::primary("h")),
            (Command::Hide, Platform::Windows | Platform::Linux) => None,
            (Command::HideOthers, Platform::MacOS) => Some(Shortcut::primary("h").with_alt()),
            (Command::HideOthers, Platform::Windows | Platform::Linux) => None,
        }
    }

    /// Menu title following platform vocabulary.
    pub fn title(self, platform: Platform, app_name: &str) -> String {
        match self {
            Command::Quit => match platform {
                Platform::MacOS => format!("Quit {app_name}"),
                Platform::Windows => "Exit".into(),
                Platform::Linux => "Quit".into(),
            },
            Command::CloseWindow => "Close Window".into(),
            Command::Minimize => "Minimize".into(),
            Command::ToggleFullScreen => match platform {
                Platform::MacOS => "Enter Full Screen".into(),
                _ => "Full Screen".into(),
            },
            Command::OpenSettings => match platform {
                Platform::MacOS => "Settings…".into(),
                Platform::Windows => "Settings".into(),
                Platform::Linux => "Preferences".into(),
            },
            Command::Hide => "Hide".into(),
            Command::HideOthers => "Hide Others".into(),
        }
    }

    pub fn action(self) -> Box<dyn Action> {
        match self {
            Command::Quit => Box::new(Quit),
            Command::CloseWindow => Box::new(CloseWindow),
            Command::Minimize => Box::new(Minimize),
            Command::ToggleFullScreen => Box::new(ToggleFullScreen),
            Command::OpenSettings => Box::new(OpenSettings),
            Command::Hide => Box::new(Hide),
            Command::HideOthers => Box::new(HideOthers),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_uses_platform_primary_modifier() {
        let s = Command::OpenSettings;
        let mac = s.shortcut(Platform::MacOS).unwrap();
        let linux = s.shortcut(Platform::Linux).unwrap();
        let windows = s.shortcut(Platform::Windows).unwrap();
        assert_eq!(mac.keystroke(Platform::MacOS), "cmd-,");
        assert_eq!(linux.keystroke(Platform::Linux), "ctrl-,");
        assert_eq!(windows.keystroke(Platform::Windows), "ctrl-,");
        assert_eq!(mac.label(Platform::MacOS), "⌘,");
        assert_eq!(linux.label(Platform::Linux), "Ctrl+,");
    }

    #[test]
    fn full_screen_follows_platform_convention() {
        let fs = Command::ToggleFullScreen;
        assert_eq!(
            fs.shortcut(Platform::MacOS)
                .unwrap()
                .keystroke(Platform::MacOS),
            "ctrl-cmd-f"
        );
        assert_eq!(
            fs.shortcut(Platform::MacOS).unwrap().label(Platform::MacOS),
            "⌃⌘F"
        );
        assert_eq!(
            fs.shortcut(Platform::Linux)
                .unwrap()
                .keystroke(Platform::Linux),
            "f11"
        );
        assert_eq!(
            fs.shortcut(Platform::Windows)
                .unwrap()
                .label(Platform::Windows),
            "F11"
        );
    }

    #[test]
    fn os_owned_gestures_are_not_bound() {
        assert!(Command::Quit.shortcut(Platform::Windows).is_none());
        assert!(Command::Minimize.shortcut(Platform::Linux).is_none());
        assert!(Command::Minimize.shortcut(Platform::MacOS).is_some());
    }

    #[test]
    fn no_two_commands_share_a_keystroke_on_any_platform() {
        for platform in Platform::ALL {
            let mut seen = std::collections::HashSet::new();
            for command in Command::ALL {
                if let Some(s) = command.shortcut(platform) {
                    assert!(
                        seen.insert(s.keystroke(platform)),
                        "{command:?} collides on {platform:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn titles_use_platform_vocabulary() {
        assert_eq!(Command::Quit.title(Platform::MacOS, "Demo"), "Quit Demo");
        assert_eq!(Command::Quit.title(Platform::Windows, "Demo"), "Exit");
        assert_eq!(
            Command::OpenSettings.title(Platform::Linux, "Demo"),
            "Preferences"
        );
    }
}
