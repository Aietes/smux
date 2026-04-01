use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{self, IconMode};
use crate::tmux::Tmux;
use crate::ui::DisplayStyle;
use crate::util;
use crate::zoxide;

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD: &str = "\x1b[1m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_RED: &str = "\x1b[31m";

pub fn run(config_path: Option<&Path>, fix: bool) -> Result<()> {
    let tmux = util::command_available("tmux");
    let fzf = util::command_available("fzf");
    let zoxide_available = util::command_available("zoxide");
    let mut has_error = false;
    let config_path = path_for_missing_config(config_path);
    let project_dir = config::projects_dir_for_config_path(&config_path);

    let schema_fix_summary = if fix {
        Some(apply_schema_fixes(&config_path, &project_dir)?)
    } else {
        None
    };

    print_status_line("tmux", availability_state(tmux), None::<&str>);
    print_status_line("fzf", availability_state(fzf), None::<&str>);
    print_status_line("zoxide", availability_state(zoxide_available), None::<&str>);

    if tmux {
        match Tmux::new().list_sessions() {
            Ok(sessions) => print_value_line("tmux_sessions", &sessions.len().to_string()),
            Err(error) => {
                print_status_line("tmux_sessions", Status::Error, None::<&str>);
                print_value_line("tmux_sessions_error", &format!("{error:#}"));
            }
        }
    } else {
        print_status_line("tmux_sessions", Status::Unavailable, None::<&str>);
    }

    if zoxide_available {
        match zoxide::list_directories() {
            Ok(directories) => {
                print_value_line("zoxide_directories", &directories.len().to_string())
            }
            Err(error) => print_value_line("zoxide_directories", &format!("error ({error:#})")),
        }
    } else {
        print_status_line("zoxide_directories", Status::Unavailable, None::<&str>);
    }

    if !tmux || !fzf {
        has_error = true;
    }

    match config::load_optional(Some(&config_path)) {
        Ok(Some(loaded)) => {
            if loaded.config_exists {
                print_status_line("config", Status::Ok, Some(loaded.path.display()));
            } else {
                print_status_line("config", Status::Missing, None::<&str>);
            }
            print_value_line(
                "projects",
                &format!(
                    "{} ({})",
                    loaded.projects.len(),
                    loaded.project_dir.display()
                ),
            );
            print_value_line(
                "invalid_projects",
                &loaded.invalid_projects.len().to_string(),
            );
            print_schema_status(&loaded.path, &loaded.project_dir);
            if let Some(summary) = schema_fix_summary {
                print_schema_fix_summary(summary);
            }
            print_icon_status(
                loaded.config.settings.icons,
                loaded.config.settings.icon_colors,
            );
        }
        Ok(None) => {
            print_status_line("config", Status::Missing, None::<&str>);
            if project_dir.exists() || config_path.exists() {
                print_value_line("projects", &format!("0 ({})", project_dir.display()));
                print_schema_status(&config_path, &project_dir);
            } else {
                print_value_line("projects", "0");
            }
            print_value_line("invalid_projects", "0");
            if let Some(summary) = schema_fix_summary {
                print_schema_fix_summary(summary);
            }
            print_icon_status(IconMode::Auto, Default::default());
        }
        Err(error) => {
            has_error = true;
            print_status_line("config", Status::Error, None::<&str>);
            print_value_line("config_error", &format!("{error:#}"));
            print_schema_status(&config_path, &project_dir);
            if let Some(summary) = schema_fix_summary {
                print_schema_fix_summary(summary);
            }
            print_value_line("icons", "unknown (config error)");
        }
    }

    if has_error {
        print_status_line("doctor", Status::Error, None::<&str>);
        bail!("doctor checks failed");
    }

    print_status_line("doctor", Status::Ok, None::<&str>);

    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SchemaFixSummary {
    updated: usize,
    inserted: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    Ok,
    Missing,
    Error,
    Unavailable,
    Drift,
}

impl Status {
    fn text(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Missing => "missing",
            Self::Error => "error",
            Self::Unavailable => "unavailable",
            Self::Drift => "drift",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Self::Ok => ANSI_GREEN,
            Self::Missing | Self::Unavailable | Self::Drift => ANSI_YELLOW,
            Self::Error => ANSI_RED,
        }
    }
}

fn availability_state(available: bool) -> Status {
    if available {
        Status::Ok
    } else {
        Status::Missing
    }
}

fn path_for_missing_config(config_path: Option<&Path>) -> PathBuf {
    match config_path {
        Some(path) => path.to_path_buf(),
        None => config::default_config_path().unwrap_or_else(|_| PathBuf::from("config.toml")),
    }
}

fn print_status_line(label: &str, status: Status, detail: Option<impl std::fmt::Display>) {
    let colored = format!("{}{}{}", status.color(), status.text(), ANSI_RESET);
    match detail {
        Some(detail) => println!("{ANSI_BOLD}{label:<16}{ANSI_RESET} {colored}  {detail}"),
        None => println!("{ANSI_BOLD}{label:<16}{ANSI_RESET} {colored}"),
    }
}

fn print_value_line(label: &str, value: &str) {
    println!("{ANSI_BOLD}{label:<16}{ANSI_RESET} {value}");
}

fn print_icon_status(icon_mode: IconMode, icon_colors: crate::config::IconColors) {
    let style = DisplayStyle::new(icon_mode, icon_colors);
    let state = if style.icons_enabled() {
        "enabled"
    } else {
        "disabled"
    };

    print_value_line(
        "icons",
        &format!(
            "{state} (mode: {}; colors: session={}, directory={}, template={}, project={}; Nerd Font support is not auto-detectable)",
            style.icon_mode().as_str(),
            style.icon_colors().session,
            style.icon_colors().directory,
            style.icon_colors().template,
            style.icon_colors().project,
        ),
    );
}

fn print_schema_fix_summary(summary: SchemaFixSummary) {
    print_status_line(
        "schema_fix",
        Status::Ok,
        Some(format!(
            "updated={} inserted={}",
            summary.updated, summary.inserted
        )),
    );
}

fn print_schema_status(config_path: &Path, project_dir: &Path) {
    let config_expected = config::schema_url("smux-config.schema.json");
    let project_expected = config::schema_url("smux-project.schema.json");

    match schema_state(config_path, &config_expected) {
        SchemaState::Ok => print_status_line("schema_config", Status::Ok, None::<&str>),
        SchemaState::Missing => print_status_line("schema_config", Status::Missing, None::<&str>),
        SchemaState::Drift => print_status_line(
            "schema_config",
            Status::Drift,
            Some(format!("expected {config_expected}")),
        ),
    }

    let mut ok = 0;
    let mut missing = 0;
    let mut drift = 0;

    if let Ok(entries) = fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }

            match schema_state(&path, &project_expected) {
                SchemaState::Ok => ok += 1,
                SchemaState::Missing => missing += 1,
                SchemaState::Drift => drift += 1,
            }
        }
    }

    let state = if drift > 0 {
        Status::Drift
    } else if missing > 0 {
        Status::Missing
    } else {
        Status::Ok
    };
    print_status_line(
        "schema_projects",
        state,
        Some(format!("ok={ok} drift={drift} missing={missing}")),
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchemaState {
    Ok,
    Missing,
    Drift,
}

fn schema_state(path: &Path, expected: &str) -> SchemaState {
    let Ok(text) = fs::read_to_string(path) else {
        return SchemaState::Missing;
    };

    let directive = text
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("#:schema "))
        .map(str::trim);

    match directive {
        Some(found) if found == expected => SchemaState::Ok,
        Some(_) => SchemaState::Drift,
        None => SchemaState::Missing,
    }
}

fn apply_schema_fixes(config_path: &Path, project_dir: &Path) -> Result<SchemaFixSummary> {
    let mut summary = SchemaFixSummary::default();
    let config_expected = config::schema_url("smux-config.schema.json");
    let project_expected = config::schema_url("smux-project.schema.json");

    if config_path.exists() {
        match rewrite_schema_line(config_path, &config_expected)? {
            SchemaRewrite::Updated => summary.updated += 1,
            SchemaRewrite::Inserted => summary.inserted += 1,
            SchemaRewrite::Unchanged => {}
        }
    }

    if project_dir.exists() {
        for entry in fs::read_dir(project_dir).with_context(|| {
            format!("failed to read project directory {}", project_dir.display())
        })? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }

            match rewrite_schema_line(&path, &project_expected)? {
                SchemaRewrite::Updated => summary.updated += 1,
                SchemaRewrite::Inserted => summary.inserted += 1,
                SchemaRewrite::Unchanged => {}
            }
        }
    }

    Ok(summary)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchemaRewrite {
    Unchanged,
    Updated,
    Inserted,
}

fn rewrite_schema_line(path: &Path, expected: &str) -> Result<SchemaRewrite> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let expected_line = format!("#:schema {expected}");
    let mut lines = text.lines().collect::<Vec<_>>();

    if let Some(index) = lines
        .iter()
        .position(|line| line.trim_start().starts_with("#:schema "))
    {
        if lines[index].trim() == expected_line {
            return Ok(SchemaRewrite::Unchanged);
        }
        lines[index] = expected_line.as_str();
        let mut updated = lines.join("\n");
        if text.ends_with('\n') {
            updated.push('\n');
        }
        fs::write(path, updated).with_context(|| format!("failed to write {}", path.display()))?;
        return Ok(SchemaRewrite::Updated);
    }

    let updated = if text.is_empty() {
        format!("{expected_line}\n")
    } else {
        format!("{expected_line}\n{text}")
    };
    fs::write(path, updated).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(SchemaRewrite::Inserted)
}
