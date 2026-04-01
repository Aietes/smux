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
    name: &str,
    session: Option<&str>,
    path_override: Option<&Path>,
    stdout: bool,
    force: bool,
    config_path: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let project_name = util::validated_project_name(name)?;
    let session = resolve_source_session(tmux, session)?;
    let project = capture_project(tmux, &session, path_override)?;
    let toml = project.to_toml();

    if stdout {
        print!("{toml}");
        return Ok(None);
    }

    let config_path = match config_path {
        Some(path) => path.to_path_buf(),
        None => config::default_config_path()?,
    };
    let project_dir = config::projects_dir_for_config_path(&config_path);
    fs::create_dir_all(&project_dir).with_context(|| {
        format!(
            "failed to create project directory {}",
            project_dir.display()
        )
    })?;

    let destination = project_dir.join(format!("{project_name}.toml"));
    if destination.exists() && !force {
        bail!(
            "project already exists at {}; pass --force to overwrite",
            destination.display()
        );
    }

    fs::write(&destination, toml)
        .with_context(|| format!("failed to write project {}", destination.display()))?;

    Ok(Some(destination))
}

fn resolve_source_session(tmux: &Tmux, session: Option<&str>) -> Result<String> {
    match session {
        Some(session) => {
            let session = util::validated_session_name(session)?;
            tmux.ensure_session_exists(&session)?;
            Ok(session)
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

        let windows = snapshot
            .windows
            .into_iter()
            .map(ExportedWindow::from_snapshot)
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            path,
            session_name: snapshot.session_name,
            startup_window: snapshot.active_window,
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

impl ExportedWindow {
    fn from_snapshot(window: WindowSnapshot) -> Result<Self> {
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
            name: window.name,
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
    use std::sync::Mutex;

    use super::{ExportedProject, ExportedWindow, capture_project};
    use crate::process::{CommandOutput, CommandStatus, FakeCommandRunner};
    use crate::tmux::{PaneSnapshot, Tmux, WindowSnapshot};
    use std::sync::Arc;

    static HOME_ENV_LOCK: Mutex<()> = Mutex::new(());

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
        let _guard = HOME_ENV_LOCK.lock().expect("home env lock should work");
        unsafe {
            std::env::set_var("HOME", "/Users/stefan");
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
            stdout: b"0\t/Users/stefan/code/demo\t1\t0\t0\t100\t40\n1\t/Users/stefan/code/demo/server\t0\t50\t0\t50\t40\n".to_vec(),
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
            Some("right")
        );

        unsafe {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn exported_window_omits_duplicate_pane_cwds() {
        let _guard = HOME_ENV_LOCK.lock().expect("home env lock should work");
        unsafe {
            std::env::set_var("HOME", "/Users/stefan");
        }

        let window = ExportedWindow::from_snapshot(WindowSnapshot {
            name: "editor".to_owned(),
            synchronize: false,
            active: true,
            panes: vec![PaneSnapshot {
                cwd: PathBuf::from("/Users/stefan/code/demo"),
                active: true,
                layout: None,
            }],
        })
        .expect("window export should succeed");

        assert_eq!(window.cwd.as_deref(), Some("~/code/demo"));
        assert!(window.panes.is_none());

        unsafe {
            std::env::remove_var("HOME");
        }
    }
}
