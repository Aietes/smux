use std::env;

use crate::config::{Config, IconMode};

const SESSION_ICON: &str = "";
const DIRECTORY_ICON: &str = "󰉋";
const TEMPLATE_ICON: &str = "󰙅";
const ANSI_RESET: &str = "\x1b[0m";
const SESSION_COLOR: &str = "\x1b[38;5;75m";
const DIRECTORY_COLOR: &str = "\x1b[38;5;108m";
const TEMPLATE_COLOR: &str = "\x1b[38;5;179m";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayStyle {
    icons_enabled: bool,
    icon_mode: IconMode,
}

impl DisplayStyle {
    pub fn from_config(config: Option<&Config>) -> Self {
        let icon_mode = config.map_or(IconMode::Auto, |config| config.settings.icons);
        Self::from_icon_mode(icon_mode)
    }

    pub fn from_icon_mode(icon_mode: IconMode) -> Self {
        let icons_enabled = match icon_mode {
            IconMode::Always => true,
            IconMode::Never => false,
            IconMode::Auto => terminal_supports_icons(),
        };

        Self {
            icons_enabled,
            icon_mode,
        }
    }

    pub fn icons_enabled(self) -> bool {
        self.icons_enabled
    }

    pub fn icon_mode(self) -> IconMode {
        self.icon_mode
    }

    pub fn session_label(self, value: &str) -> String {
        self.label(SESSION_ICON, SESSION_COLOR, "session", value)
    }

    pub fn directory_label(self, value: &str) -> String {
        self.label(DIRECTORY_ICON, DIRECTORY_COLOR, "dir", value)
    }

    pub fn template_label(self, value: &str) -> String {
        self.label(TEMPLATE_ICON, TEMPLATE_COLOR, "template", value)
    }

    fn label(self, icon: &str, color: &str, text: &str, value: &str) -> String {
        if self.icons_enabled {
            format!("{color}{icon}{ANSI_RESET}  {value}")
        } else {
            format!("{text:<8} {value}")
        }
    }
}

pub fn terminal_supports_icons() -> bool {
    if matches!(env::var("TERM"), Ok(term) if term == "dumb") {
        return false;
    }

    match locale_value() {
        Some(locale) => {
            let locale = locale.to_string_lossy().to_ascii_lowercase();
            locale.contains("utf-8") || locale.contains("utf8")
        }
        None => true,
    }
}

fn locale_value() -> Option<std::ffi::OsString> {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(env::var_os)
}

#[cfg(test)]
mod tests {
    use super::DisplayStyle;
    use crate::config::IconMode;

    #[test]
    fn always_mode_enables_icons() {
        let style = DisplayStyle::from_icon_mode(IconMode::Always);
        assert!(style.icons_enabled());
        assert!(
            style
                .session_label("demo")
                .starts_with("\u{1b}[38;5;75m\u{1b}[0m")
        );
    }

    #[test]
    fn never_mode_uses_text_labels() {
        let style = DisplayStyle::from_icon_mode(IconMode::Never);
        assert!(!style.icons_enabled());
        assert_eq!(style.directory_label("/tmp/demo"), "dir      /tmp/demo");
        assert_eq!(style.template_label("rust"), "template rust");
    }
}
