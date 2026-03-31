use std::env;

use crate::config::{Config, IconColors, IconMode};

const SESSION_ICON: &str = "";
const DIRECTORY_ICON: &str = "󰉋";
const TEMPLATE_ICON: &str = "󰙅";
const PROJECT_ICON: &str = "󰏖";
const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD: &str = "\x1b[1m";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayStyle {
    icons_enabled: bool,
    icon_mode: IconMode,
    icon_colors: IconColors,
}

impl DisplayStyle {
    pub fn from_config(config: Option<&Config>) -> Self {
        let (icon_mode, icon_colors) = config.map_or_else(
            || (IconMode::Auto, IconColors::default()),
            |config| (config.settings.icons, config.settings.icon_colors),
        );
        Self::new(icon_mode, icon_colors)
    }

    pub fn from_icon_mode(icon_mode: IconMode) -> Self {
        Self::new(icon_mode, IconColors::default())
    }

    pub fn new(icon_mode: IconMode, icon_colors: IconColors) -> Self {
        let icons_enabled = match icon_mode {
            IconMode::Always => true,
            IconMode::Never => false,
            IconMode::Auto => terminal_supports_icons(),
        };

        Self {
            icons_enabled,
            icon_mode,
            icon_colors,
        }
    }

    pub fn icons_enabled(self) -> bool {
        self.icons_enabled
    }

    pub fn icon_mode(self) -> IconMode {
        self.icon_mode
    }

    pub fn icon_colors(self) -> IconColors {
        self.icon_colors
    }

    pub fn session_label(self, value: &str) -> String {
        self.label(SESSION_ICON, self.icon_colors.session, "session", value)
    }

    pub fn current_session_label(self, value: &str) -> String {
        if self.icons_enabled {
            format!(
                "{ANSI_BOLD}\x1b[38;5;{color}m{icon}{ANSI_RESET}  {ANSI_BOLD}\x1b[38;5;{color}m{value}{ANSI_RESET}",
                color = self.icon_colors.session,
                icon = SESSION_ICON,
            )
        } else {
            format!("current  {value}")
        }
    }

    pub fn directory_label(self, value: &str) -> String {
        self.label(DIRECTORY_ICON, self.icon_colors.directory, "dir", value)
    }

    pub fn template_label(self, value: &str) -> String {
        self.label(TEMPLATE_ICON, self.icon_colors.template, "template", value)
    }

    pub fn project_label(self, value: &str) -> String {
        self.label(PROJECT_ICON, self.icon_colors.project, "project", value)
    }

    fn label(self, icon: &str, color: u8, text: &str, value: &str) -> String {
        if self.icons_enabled {
            format!("\x1b[38;5;{color}m{icon}{ANSI_RESET}  {value}")
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
    use crate::config::{IconColors, IconMode};

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
        assert_eq!(style.project_label("demo"), "project  demo");
        assert_eq!(style.current_session_label("demo"), "current  demo");
    }

    #[test]
    fn custom_palette_changes_icon_colors() {
        let style = DisplayStyle::new(
            IconMode::Always,
            IconColors {
                session: 33,
                directory: 44,
                template: 55,
                project: 66,
            },
        );

        assert!(style.session_label("demo").starts_with("\u{1b}[38;5;33m"));
        assert!(
            style
                .directory_label("/tmp/demo")
                .starts_with("\u{1b}[38;5;44m")
        );
        assert!(style.template_label("rust").starts_with("\u{1b}[38;5;55m"));
        assert!(style.project_label("demo").starts_with("\u{1b}[38;5;66m"));
    }

    #[test]
    fn current_session_label_uses_bold_style() {
        let style = DisplayStyle::from_icon_mode(IconMode::Always);
        let label = style.current_session_label("demo");
        assert!(label.starts_with("\u{1b}[1m\u{1b}[38;5;75m\u{1b}[0m"));
        assert!(label.ends_with("  \u{1b}[1m\u{1b}[38;5;75mdemo\u{1b}[0m"));
    }
}
