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
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_RED: &str = "\x1b[31m";

pub fn run(config_path: Option<&Path>, fix: bool) -> Result<()> {
    let tmux = util::command_available("tmux");
    let fzf = util::command_available("fzf");
    let zoxide_available = util::command_available("zoxide");
    let config_path = path_for_missing_config(config_path);
    let project_dir = config::projects_dir_for_config_path(&config_path);

    let schema_fix_summary = if fix {
        Some(apply_schema_fixes(&config_path, &project_dir)?)
    } else {
        None
    };

    let mut sections = vec![
        Section {
            title: "Dependencies",
            checks: vec![
                dependency_check("tmux", tmux, true),
                dependency_check("fzf", fzf, true),
                dependency_check("zoxide", zoxide_available, false),
            ],
        },
        Section {
            title: "Sources",
            checks: vec![
                tmux_sessions_check(tmux),
                zoxide_directories_check(zoxide_available),
            ],
        },
    ];

    let mut config_checks = Vec::new();
    let mut schema_checks = Vec::new();
    let mut display_checks = Vec::new();
    let mut folder_checks = Vec::new();

    match config::load_optional(Some(&config_path)) {
        Ok(Some(loaded)) => {
            if loaded.config_exists {
                config_checks.push(Check::new(
                    Status::Ok,
                    "config",
                    Some(loaded.path.display().to_string()),
                ));
            } else {
                config_checks.push(Check::new(
                    Status::Missing,
                    "config",
                    Some("not found (using defaults)".to_owned()),
                ));
            }
            config_checks.push(Check::new(
                Status::Ok,
                "projects",
                Some(format!(
                    "{} in {}",
                    loaded.projects.len(),
                    loaded.project_dir.display()
                )),
            ));
            config_checks.push(invalid_projects_check(loaded.invalid_projects.len()));
            schema_checks.extend(schema_checks_for(&loaded.path, &loaded.project_dir));
            display_checks.push(icon_check(
                loaded.config.settings.icons,
                loaded.config.settings.icon_colors,
            ));
            folder_checks.push(folder_search_check(&loaded.config.settings.folder_search));
        }
        Ok(None) => {
            config_checks.push(Check::new(
                Status::Missing,
                "config",
                Some("not found (using defaults)".to_owned()),
            ));
            if project_dir.exists() || config_path.exists() {
                config_checks.push(Check::new(
                    Status::Ok,
                    "projects",
                    Some(format!("0 in {}", project_dir.display())),
                ));
                schema_checks.extend(schema_checks_for(&config_path, &project_dir));
            } else {
                config_checks.push(Check::new(Status::Ok, "projects", Some("0".to_owned())));
            }
            config_checks.push(invalid_projects_check(0));
            display_checks.push(icon_check(IconMode::Auto, Default::default()));
            folder_checks.push(folder_search_check(&Default::default()));
        }
        Err(error) => {
            config_checks.push(Check::new(
                Status::Error,
                "config",
                Some(format!("invalid: {error:#}")),
            ));
            schema_checks.extend(schema_checks_for(&config_path, &project_dir));
            display_checks.push(Check::new(
                Status::Unavailable,
                "icons",
                Some("unknown (config error)".to_owned()),
            ));
            folder_checks.push(Check::new(
                Status::Unavailable,
                "folder search",
                Some("unknown (config error)".to_owned()),
            ));
        }
    }

    if let Some(summary) = schema_fix_summary {
        schema_checks.push(Check::new(
            Status::Ok,
            "schema fixes",
            Some(format!(
                "updated {} · inserted {}",
                summary.updated, summary.inserted
            )),
        ));
    }

    sections.push(Section {
        title: "Configuration",
        checks: config_checks,
    });
    sections.push(Section {
        title: "Schemas",
        checks: schema_checks,
    });
    sections.push(Section {
        title: "Display",
        checks: display_checks,
    });
    sections.push(Section {
        title: "Folder search",
        checks: folder_checks,
    });

    let errors = sections
        .iter()
        .flat_map(|section| &section.checks)
        .filter(|check| check.status.is_error())
        .count();
    let warnings = sections
        .iter()
        .flat_map(|section| &section.checks)
        .filter(|check| check.status.is_warning())
        .count();

    render_report(&sections);
    render_footer(errors, warnings);

    if errors > 0 {
        bail!("doctor checks failed");
    }

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
    fn symbol(self) -> &'static str {
        match self {
            Self::Ok => "✓",
            Self::Missing | Self::Drift => "⚠",
            Self::Error => "✗",
            Self::Unavailable => "·",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Self::Ok => ANSI_GREEN,
            Self::Missing | Self::Drift => ANSI_YELLOW,
            Self::Error => ANSI_RED,
            Self::Unavailable => ANSI_DIM,
        }
    }

    fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }

    fn is_warning(self) -> bool {
        matches!(self, Self::Missing | Self::Drift)
    }
}

struct Check {
    status: Status,
    label: String,
    detail: Option<String>,
}

impl Check {
    fn new(status: Status, label: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            status,
            label: label.into(),
            detail,
        }
    }
}

struct Section {
    title: &'static str,
    checks: Vec<Check>,
}

fn render_report(sections: &[Section]) {
    let width = sections
        .iter()
        .flat_map(|section| &section.checks)
        .map(|check| check.label.chars().count())
        .max()
        .unwrap_or(0);

    println!("{ANSI_BOLD}smux doctor{ANSI_RESET}");

    for section in sections {
        if section.checks.is_empty() {
            continue;
        }
        println!();
        println!("{ANSI_BOLD}{}{ANSI_RESET}", section.title);
        for check in &section.checks {
            let symbol = format!(
                "{}{}{ANSI_RESET}",
                check.status.color(),
                check.status.symbol()
            );
            match &check.detail {
                Some(detail) => {
                    println!(
                        "  {symbol} {:<width$}  {ANSI_DIM}{detail}{ANSI_RESET}",
                        check.label
                    )
                }
                None => println!("  {symbol} {}", check.label),
            }
        }
    }
}

fn render_footer(errors: usize, warnings: usize) {
    println!();
    let summary = summary_text(errors, warnings);
    if errors > 0 {
        println!("{ANSI_RED}✗ {summary}{ANSI_RESET}");
    } else if warnings > 0 {
        println!("{ANSI_YELLOW}⚠ {summary}{ANSI_RESET}");
    } else {
        println!("{ANSI_GREEN}✓ {summary}{ANSI_RESET}");
    }
}

fn summary_text(errors: usize, warnings: usize) -> String {
    if errors == 0 && warnings == 0 {
        return "all checks passed".to_owned();
    }
    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(count_phrase(errors, "error"));
    }
    if warnings > 0 {
        parts.push(count_phrase(warnings, "warning"));
    }
    parts.join(", ")
}

fn count_phrase(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn dependency_check(name: &'static str, available: bool, required: bool) -> Check {
    if available {
        Check::new(Status::Ok, name, Some("found".to_owned()))
    } else if required {
        Check::new(Status::Error, name, Some("not found (required)".to_owned()))
    } else {
        Check::new(
            Status::Missing,
            name,
            Some("not found (optional)".to_owned()),
        )
    }
}

fn tmux_sessions_check(tmux: bool) -> Check {
    if !tmux {
        return Check::new(
            Status::Unavailable,
            "tmux sessions",
            Some("tmux not found".to_owned()),
        );
    }
    match Tmux::new().list_sessions() {
        Ok(sessions) => Check::new(
            Status::Ok,
            "tmux sessions",
            Some(format!("{} running", sessions.len())),
        ),
        Err(error) => Check::new(
            Status::Missing,
            "tmux sessions",
            Some(format!("failed to list: {error:#}")),
        ),
    }
}

fn zoxide_directories_check(zoxide_available: bool) -> Check {
    if !zoxide_available {
        return Check::new(
            Status::Unavailable,
            "zoxide directories",
            Some("zoxide not found".to_owned()),
        );
    }
    match zoxide::list_directories() {
        Ok(directories) => Check::new(
            Status::Ok,
            "zoxide directories",
            Some(format!("{} indexed", directories.len())),
        ),
        Err(error) => Check::new(
            Status::Missing,
            "zoxide directories",
            Some(format!("failed to list: {error:#}")),
        ),
    }
}

fn invalid_projects_check(count: usize) -> Check {
    if count == 0 {
        Check::new(Status::Ok, "invalid projects", Some("none".to_owned()))
    } else {
        Check::new(
            Status::Missing,
            "invalid projects",
            Some(format!("{count} broken — fix or remove")),
        )
    }
}

fn icon_check(icon_mode: IconMode, icon_colors: crate::config::IconColors) -> Check {
    let style = DisplayStyle::new(icon_mode, icon_colors);
    let state = if style.icons_enabled() {
        "enabled"
    } else {
        "disabled"
    };
    let colors = style.icon_colors();

    Check::new(
        Status::Ok,
        "icons",
        Some(format!(
            "{state} · mode {} · colors {}/{}/{}/{} · Nerd Font not auto-detected",
            style.icon_mode().as_str(),
            colors.session,
            colors.directory,
            colors.template,
            colors.project,
        )),
    )
}

fn folder_search_check(settings: &config::FolderSearchSettings) -> Check {
    let missing = settings
        .roots
        .iter()
        .filter(|root| {
            let expanded = util::expand_tilde_path(Path::new(root));
            !expanded.exists()
        })
        .count();

    let roots_word = if settings.roots.len() == 1 {
        "root"
    } else {
        "roots"
    };
    let hidden = if settings.include_hidden {
        "included"
    } else {
        "excluded"
    };
    let detail = if missing == 0 {
        format!(
            "{} {roots_word} · max depth {} · hidden {hidden}",
            settings.roots.len(),
            settings.max_depth,
        )
    } else {
        format!(
            "{} {roots_word} · {missing} missing · max depth {} · hidden {hidden}",
            settings.roots.len(),
            settings.max_depth,
        )
    };
    let status = if missing == 0 {
        Status::Ok
    } else {
        Status::Missing
    };

    Check::new(status, "folder search", Some(detail))
}

fn path_for_missing_config(config_path: Option<&Path>) -> PathBuf {
    match config_path {
        Some(path) => path.to_path_buf(),
        None => config::default_config_path().unwrap_or_else(|_| PathBuf::from("config.toml")),
    }
}

fn schema_checks_for(config_path: &Path, project_dir: &Path) -> Vec<Check> {
    let config_expected = config::schema_url("smux-config.schema.json");
    let project_expected = config::schema_url("smux-project.schema.json");

    let config_check = match schema_state(config_path, &config_expected) {
        SchemaState::Ok => Check::new(Status::Ok, "config schema", Some("up to date".to_owned())),
        SchemaState::Missing => Check::new(
            Status::Missing,
            "config schema",
            Some("no #:schema directive".to_owned()),
        ),
        SchemaState::Drift => Check::new(
            Status::Drift,
            "config schema",
            Some("out of date — run `smux doctor --fix`".to_owned()),
        ),
    };

    let (ok, missing, drift) = count_project_schema_states(project_dir, &project_expected);
    let status = if drift > 0 {
        Status::Drift
    } else if missing > 0 {
        Status::Missing
    } else {
        Status::Ok
    };
    let project_check = Check::new(
        status,
        "project schemas",
        Some(format!("{ok} ok · {drift} drift · {missing} missing")),
    );

    vec![config_check, project_check]
}

fn count_project_schema_states(project_dir: &Path, expected: &str) -> (usize, usize, usize) {
    let mut ok = 0;
    let mut missing = 0;
    let mut drift = 0;

    if let Ok(entries) = fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }

            match schema_state(&path, expected) {
                SchemaState::Ok => ok += 1,
                SchemaState::Missing => missing += 1,
                SchemaState::Drift => drift += 1,
            }
        }
    }

    (ok, missing, drift)
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

#[cfg(test)]
mod tests {
    use super::{Status, count_phrase, dependency_check, invalid_projects_check, summary_text};

    #[test]
    fn summary_text_describes_combinations() {
        assert_eq!(summary_text(0, 0), "all checks passed");
        assert_eq!(summary_text(0, 1), "1 warning");
        assert_eq!(summary_text(0, 2), "2 warnings");
        assert_eq!(summary_text(1, 0), "1 error");
        assert_eq!(summary_text(1, 1), "1 error, 1 warning");
        assert_eq!(summary_text(2, 3), "2 errors, 3 warnings");
    }

    #[test]
    fn count_phrase_pluralizes() {
        assert_eq!(count_phrase(1, "error"), "1 error");
        assert_eq!(count_phrase(0, "error"), "0 errors");
        assert_eq!(count_phrase(3, "warning"), "3 warnings");
    }

    #[test]
    fn status_severity_classification() {
        assert!(Status::Error.is_error());
        assert!(!Status::Error.is_warning());
        assert!(Status::Missing.is_warning());
        assert!(Status::Drift.is_warning());
        assert!(!Status::Ok.is_warning() && !Status::Ok.is_error());
        assert!(!Status::Unavailable.is_warning() && !Status::Unavailable.is_error());
    }

    #[test]
    fn required_dependency_missing_is_an_error() {
        assert_eq!(dependency_check("tmux", false, true).status, Status::Error);
        assert_eq!(
            dependency_check("zoxide", false, false).status,
            Status::Missing
        );
        assert_eq!(dependency_check("tmux", true, true).status, Status::Ok);
    }

    #[test]
    fn invalid_projects_check_warns_only_when_present() {
        assert_eq!(invalid_projects_check(0).status, Status::Ok);
        assert_eq!(invalid_projects_check(2).status, Status::Missing);
    }
}
