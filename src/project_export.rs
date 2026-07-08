use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config;
use crate::tmux::{PaneSnapshot, SessionSnapshot, Tmux, WindowSnapshot};
use crate::util;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ExportedProject {
    pub path: String,
    pub session_name: String,
    pub startup_window: String,
    pub startup_pane: usize,
    pub windows: Vec<ExportedWindow>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ExportedWindow {
    pub name: String,
    pub cwd: Option<String>,
    pub layout: Option<String>,
    pub synchronize: bool,
    pub panes: Option<Vec<ExportedPane>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ExportedPane {
    pub layout: Option<String>,
    pub cwd: Option<String>,
}

pub fn capture_project(
    tmux: &Tmux,
    session: &str,
    path_override: Option<&Path>,
) -> Result<ExportedProject> {
    let snapshot = tmux.capture_session(session)?;
    ExportedProject::from_snapshot(snapshot, path_override)
}

pub fn save_project(
    tmux: &Tmux,
    name: Option<&str>,
    session: Option<&str>,
    path_override: Option<&Path>,
    stdout: bool,
    force: bool,
    config_path: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let session = resolve_source_session(tmux, session)?;
    // Default the project name to the source session's name so a bare
    // `smux save-project` captures the session you are in.
    let project_name = util::validated_project_name(name.unwrap_or(session.as_str()))?;
    let project = capture_project(tmux, &session, path_override)?;
    let toml = project.to_toml();

    if stdout {
        print!("{toml}");
        return Ok(None);
    }

    let destination = project_destination(&project_name, config_path)?;
    if destination.exists() && !force {
        bail!(
            "project already exists at {}; pass --force to overwrite",
            destination.display()
        );
    }

    if let Some(project_dir) = destination.parent() {
        fs::create_dir_all(project_dir).with_context(|| {
            format!(
                "failed to create project directory {}",
                project_dir.display()
            )
        })?;
    }

    fs::write(&destination, toml)
        .with_context(|| format!("failed to write project {}", destination.display()))?;

    Ok(Some(destination))
}

/// Resolve where a project file with the given (already validated) name lives.
fn project_destination(project_name: &str, config_path: Option<&Path>) -> Result<PathBuf> {
    let config_path = match config_path {
        Some(path) => path.to_path_buf(),
        None => config::default_config_path()?,
    };
    let project_dir = config::projects_dir_for_config_path(&config_path);
    Ok(project_dir.join(format!("{project_name}.toml")))
}

/// Whether a project file already exists for the given name.
pub fn project_exists(name: &str, config_path: Option<&Path>) -> Result<bool> {
    let project_name = util::validated_project_name(name)?;
    Ok(project_destination(&project_name, config_path)?.exists())
}

fn resolve_source_session(tmux: &Tmux, session: Option<&str>) -> Result<String> {
    match session {
        Some(session) => {
            // The source session already exists in tmux, so its name is used
            // verbatim — sanitizing would make sessions created outside smux
            // (e.g. names with spaces) unreachable.
            if session.trim().is_empty() {
                bail!("session name must not be empty");
            }
            tmux.ensure_session_exists(session)?;
            Ok(session.to_owned())
        }
        None if util::inside_tmux() => tmux
            .current_session()?
            .context("could not determine current tmux session"),
        None => bail!("--session is required outside tmux"),
    }
}

impl ExportedProject {
    fn from_snapshot(snapshot: SessionSnapshot, path_override: Option<&Path>) -> Result<Self> {
        let path = match path_override {
            Some(path) => util::path_to_config_string(&util::expand_and_absolutize_path(path)?)?,
            None => util::path_to_config_string(&snapshot.active_path)?,
        };

        // tmux permits window names that smux's own validation rejects (`:`
        // and `.` break tmux target addressing; duplicates resolve
        // ambiguously), so captured names are rewritten before export — the
        // saved project must connect cleanly. The startup window is remapped
        // to the rewritten name of the same window.
        let window_names = sanitized_window_names(&snapshot.windows);
        let startup_window = snapshot
            .windows
            .iter()
            .position(|window| window.active)
            .and_then(|index| window_names.get(index))
            .or_else(|| window_names.first())
            .cloned()
            .unwrap_or_else(|| snapshot.active_window.clone());

        let windows = snapshot
            .windows
            .into_iter()
            .zip(window_names)
            .map(|(window, name)| ExportedWindow::from_snapshot(window, name))
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            path,
            session_name: snapshot.session_name,
            startup_window,
            startup_pane: snapshot.active_pane,
            windows,
        })
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "#:schema {}\n",
            config::schema_url("smux-project.schema.json")
        ));
        out.push_str(&format!("path = {}\n", toml_string(&self.path)));
        out.push_str(&format!(
            "session_name = {}\n",
            toml_string(&self.session_name)
        ));
        out.push_str(&format!(
            "startup_window = {}\n",
            toml_string(&self.startup_window)
        ));
        out.push_str(&format!("startup_pane = {}\n", self.startup_pane));
        out.push_str("windows = [\n");
        for window in &self.windows {
            out.push_str("  ");
            out.push_str(&window.to_inline_toml(2));
            out.push_str(",\n");
        }
        out.push_str("]\n");
        out
    }
}

/// Rewrite captured tmux window names into names that pass smux's template
/// validation: `:` and `.` become `_`, empty names fall back to "window", and
/// duplicates get a numeric suffix (`dev`, `dev-2`, ...).
fn sanitized_window_names(windows: &[WindowSnapshot]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::with_capacity(windows.len());
    for window in windows {
        let base = window.name.replace([':', '.'], "_");
        let base = if base.is_empty() {
            "window".to_owned()
        } else {
            base
        };
        let mut candidate = base.clone();
        let mut counter = 2;
        while !seen.insert(candidate.clone()) {
            candidate = format!("{base}-{counter}");
            counter += 1;
        }
        names.push(candidate);
    }
    names
}

impl ExportedWindow {
    fn from_snapshot(window: WindowSnapshot, name: String) -> Result<Self> {
        let all_same_cwd = all_panes_share_cwd(&window.panes);
        let cwd = if all_same_cwd {
            Some(util::path_to_config_string(&window.panes[0].cwd)?)
        } else {
            None
        };

        let panes = if window.panes.len() > 1 || !all_same_cwd {
            Some(
                window
                    .panes
                    .into_iter()
                    .enumerate()
                    .map(|(index, pane)| ExportedPane::from_snapshot(index, pane, cwd.as_deref()))
                    .collect::<Result<Vec<_>>>()?,
            )
        } else {
            None
        };

        Ok(Self {
            name,
            cwd,
            layout: None,
            synchronize: window.synchronize,
            panes,
        })
    }

    fn to_inline_toml(&self, indent: usize) -> String {
        let mut fields = vec![format!("name = {}", toml_string(&self.name))];
        if let Some(cwd) = &self.cwd {
            fields.push(format!("cwd = {}", toml_string(cwd)));
        }
        if let Some(layout) = &self.layout {
            fields.push(format!("layout = {}", toml_string(layout)));
        }
        if self.synchronize {
            fields.push("synchronize = true".to_owned());
        }
        if let Some(panes) = &self.panes {
            let inner_indent = " ".repeat(indent + 2);
            let outer_indent = " ".repeat(indent);
            let rendered = panes
                .iter()
                .map(|pane| format!("{inner_indent}{},", pane.to_inline_toml()))
                .collect::<Vec<_>>()
                .join("\n");
            fields.push(format!("panes = [\n{rendered}\n{outer_indent}]"));
        }
        format!("{{ {} }}", fields.join(", "))
    }
}

impl ExportedPane {
    fn from_snapshot(index: usize, pane: PaneSnapshot, window_cwd: Option<&str>) -> Result<Self> {
        let cwd = util::path_to_config_string(&pane.cwd)?;
        Ok(Self {
            layout: if index == 0 {
                None
            } else {
                pane.layout.map(render_pane_layout)
            },
            cwd: if window_cwd == Some(cwd.as_str()) {
                None
            } else {
                Some(cwd)
            },
        })
    }

    fn to_inline_toml(&self) -> String {
        let mut fields = Vec::new();
        if let Some(layout) = &self.layout {
            fields.push(format!("layout = {}", toml_string(layout)));
        }
        if let Some(cwd) = &self.cwd {
            fields.push(format!("cwd = {}", toml_string(cwd)));
        }
        format!("{{ {} }}", fields.join(", "))
    }
}

fn all_panes_share_cwd(panes: &[PaneSnapshot]) -> bool {
    panes
        .first()
        .map(|first| panes.iter().all(|pane| pane.cwd == first.cwd))
        .unwrap_or(false)
}

fn render_pane_layout(layout: crate::templates::PaneLayout) -> String {
    let position = match layout.position {
        crate::templates::PanePosition::Right => "right",
        crate::templates::PanePosition::Left => "left",
        crate::templates::PanePosition::Bottom => "bottom",
        crate::templates::PanePosition::Top => "top",
    };

    match layout.size {
        Some(size) => format!("{position} {size}"),
        None => position.to_owned(),
    }
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ExportedProject, ExportedWindow, capture_project};
    use crate::process::{CommandOutput, CommandStatus, FakeCommandRunner};
    use crate::tmux::{PaneSnapshot, Tmux, WindowSnapshot};
    use std::sync::Arc;

    #[test]
    fn exported_project_renders_inline_toml() {
        let project = ExportedProject {
            path: "~/code/demo".to_owned(),
            session_name: "demo".to_owned(),
            startup_window: "editor".to_owned(),
            startup_pane: 0,
            windows: vec![ExportedWindow {
                name: "editor".to_owned(),
                cwd: Some("~/code/demo".to_owned()),
                layout: None,
                synchronize: false,
                panes: Some(vec![
                    super::ExportedPane {
                        layout: None,
                        cwd: None,
                    },
                    super::ExportedPane {
                        layout: Some("right".to_owned()),
                        cwd: Some("~/code/demo/server".to_owned()),
                    },
                ]),
            }],
        };

        let toml = project.to_toml();
        assert!(toml.starts_with("#:schema "));
        assert!(toml.contains("path = \"~/code/demo\""));
        assert!(toml.contains("session_name = \"demo\""));
        assert!(toml.contains("windows = ["));
        assert!(toml.contains("{ name = \"editor\""));
        assert!(toml.contains("{ layout = \"right\", cwd = \"~/code/demo/server\" }"));
    }

    #[test]
    fn capture_project_uses_active_pane_path_by_default() {
        let _guard = crate::util::test_env::lock();
        unsafe {
            std::env::set_var("HOME", "/Users/dev");
        }

        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: Vec::new(),
            stderr: Vec::new(),
        }));
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"@1\teditor\t1\n".to_vec(),
            stderr: Vec::new(),
        }));
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"off\n".to_vec(),
            stderr: Vec::new(),
        }));
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"0\t/Users/dev/code/demo\t1\t0\t0\t100\t40\n1\t/Users/dev/code/demo/server\t0\t50\t0\t50\t40\n".to_vec(),
            stderr: Vec::new(),
        }));

        let tmux = Tmux::with_runner(runner);
        let project = capture_project(&tmux, "demo", None).expect("capture should succeed");
        assert_eq!(project.path, "~/code/demo");
        assert_eq!(project.startup_window, "editor");
        assert_eq!(project.startup_pane, 0);
        assert_eq!(
            project.windows[0]
                .panes
                .as_ref()
                .expect("panes should exist")[1]
                .layout
                .as_deref(),
            Some("right 50")
        );

        unsafe {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn sanitizes_and_dedupes_captured_window_names() {
        let _guard = crate::util::test_env::lock();
        let pane = PaneSnapshot {
            cwd: PathBuf::from("/tmp/demo"),
            active: true,
            layout: None,
        };
        let window = |name: &str, active: bool| WindowSnapshot {
            name: name.to_owned(),
            synchronize: false,
            active,
            panes: vec![pane.clone()],
        };

        let snapshot = crate::tmux::SessionSnapshot {
            session_name: "demo".to_owned(),
            active_window: "vim.main".to_owned(),
            active_pane: 0,
            active_path: PathBuf::from("/tmp/demo"),
            windows: vec![window("vim.main", false), window("vim:main", true)],
        };

        let project =
            ExportedProject::from_snapshot(snapshot, None).expect("export should succeed");
        assert_eq!(project.windows[0].name, "vim_main");
        assert_eq!(project.windows[1].name, "vim_main-2");
        // The startup window follows the rewritten name of the active window.
        assert_eq!(project.startup_window, "vim_main-2");
    }

    #[test]
    fn exported_window_omits_duplicate_pane_cwds() {
        let _guard = crate::util::test_env::lock();
        unsafe {
            std::env::set_var("HOME", "/Users/dev");
        }

        let window = ExportedWindow::from_snapshot(
            WindowSnapshot {
                name: "editor".to_owned(),
                synchronize: false,
                active: true,
                panes: vec![PaneSnapshot {
                    cwd: PathBuf::from("/Users/dev/code/demo"),
                    active: true,
                    layout: None,
                }],
            },
            "editor".to_owned(),
        )
        .expect("window export should succeed");

        assert_eq!(window.cwd.as_deref(), Some("~/code/demo"));
        assert!(window.panes.is_none());

        unsafe {
            std::env::remove_var("HOME");
        }
    }
}
