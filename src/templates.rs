use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::config::{Pane, Template, Window};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlan {
    pub session_name: String,
    pub windows: Vec<WindowPlan>,
    pub startup_window: String,
    pub startup_pane: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowPlan {
    pub name: String,
    pub cwd: PathBuf,
    pub pre_command: Option<String>,
    pub command: Option<String>,
    pub layout: Option<String>,
    pub synchronize: bool,
    pub panes: Vec<PanePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanePlan {
    pub layout: Option<PaneLayout>,
    pub cwd: PathBuf,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneLayout {
    pub position: PanePosition,
    pub size: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanePosition {
    Right,
    Left,
    Bottom,
    Top,
}

pub fn fallback_template() -> Template {
    Template {
        root: None,
        startup_window: Some("main".to_owned()),
        startup_pane: Some(0),
        windows: vec![Window {
            name: "main".to_owned(),
            cwd: None,
            pre_command: None,
            command: None,
            layout: None,
            synchronize: false,
            panes: None,
        }],
    }
}

pub fn build_session_plan(
    session_name: &str,
    root: &Path,
    template: &Template,
) -> Result<SessionPlan> {
    if template.windows.is_empty() {
        bail!("template must contain at least one window");
    }

    let template_root = resolve_root(root, template.root.as_deref())?;
    let mut windows = Vec::with_capacity(template.windows.len());

    for window in &template.windows {
        windows.push(build_window_plan(&template_root, root, window)?);
    }

    let startup_window = template
        .startup_window
        .clone()
        .unwrap_or_else(|| template.windows[0].name.clone());

    let startup_pane = resolve_startup_pane(template, &windows, &startup_window)?;

    Ok(SessionPlan {
        session_name: session_name.to_owned(),
        windows,
        startup_window,
        startup_pane,
    })
}

fn build_window_plan(
    template_root: &Path,
    session_root: &Path,
    window: &Window,
) -> Result<WindowPlan> {
    let cwd = resolve_root(template_root, window.cwd.as_deref())?;
    let panes = match &window.panes {
        Some(panes) => panes
            .iter()
            .map(|pane| build_pane_plan(template_root, session_root, &cwd, pane))
            .collect::<Result<Vec<_>>>()?,
        None => Vec::new(),
    };

    Ok(WindowPlan {
        name: window.name.clone(),
        cwd,
        pre_command: window.pre_command.clone(),
        command: window.command.clone(),
        layout: window.layout.clone(),
        synchronize: window.synchronize,
        panes,
    })
}

fn build_pane_plan(
    template_root: &Path,
    session_root: &Path,
    window_root: &Path,
    pane: &Pane,
) -> Result<PanePlan> {
    let cwd = if let Some(cwd) = &pane.cwd {
        resolve_relative(session_root, template_root, window_root, cwd)?
    } else {
        window_root.to_path_buf()
    };

    Ok(PanePlan {
        layout: parse_pane_layout(pane.layout.as_deref())?,
        cwd,
        command: pane.command.clone(),
    })
}

fn parse_pane_layout(layout: Option<&str>) -> Result<Option<PaneLayout>> {
    let Some(layout) = layout else {
        return Ok(None);
    };

    let mut parts = layout.split_whitespace();
    let position = match parts.next() {
        Some("right") => PanePosition::Right,
        Some("left") => PanePosition::Left,
        Some("bottom") => PanePosition::Bottom,
        Some("top") => PanePosition::Top,
        Some(other) => bail!("unknown pane layout position: {other}"),
        None => bail!("pane layout cannot be empty"),
    };

    let size = parts.next().map(ToOwned::to_owned);
    if parts.next().is_some() {
        bail!("pane layout must be in the form '<position>' or '<position> <size>'");
    }

    Ok(Some(PaneLayout { position, size }))
}

fn resolve_root(session_root: &Path, root: Option<&str>) -> Result<PathBuf> {
    match root {
        Some(root) => resolve_relative(session_root, session_root, session_root, root),
        None => Ok(session_root.to_path_buf()),
    }
}

fn resolve_startup_pane(
    template: &Template,
    windows: &[WindowPlan],
    startup_window: &str,
) -> Result<usize> {
    let startup_pane = template.startup_pane.unwrap_or(0);
    let window = windows
        .iter()
        .find(|window| window.name == startup_window)
        .ok_or_else(|| anyhow::anyhow!("startup window \"{startup_window}\" was not found"))?;

    let pane_count = if window.panes.is_empty() {
        1
    } else {
        window.panes.len()
    };

    if startup_pane >= pane_count {
        bail!(
            "startup_pane {} is out of range for window \"{}\" with {} pane(s)",
            startup_pane,
            startup_window,
            pane_count
        );
    }

    Ok(startup_pane)
}

fn resolve_relative(
    session_root: &Path,
    template_root: &Path,
    window_root: &Path,
    value: &str,
) -> Result<PathBuf> {
    let path = Path::new(value);
    let base = if path.is_absolute() {
        PathBuf::new()
    } else if value.starts_with("./") || value.starts_with("../") {
        window_root.to_path_buf()
    } else {
        template_root.to_path_buf()
    };

    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else if base.as_os_str().is_empty() {
        session_root.join(path)
    } else {
        base.join(path)
    };

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::{build_session_plan, fallback_template};
    use crate::config::{Pane, Template, Window};
    use anyhow::Result;
    use std::path::Path;

    #[test]
    fn fallback_template_has_main_window() {
        let template = fallback_template();
        assert_eq!(template.windows.len(), 1);
        assert_eq!(template.windows[0].name, "main");
    }

    #[test]
    fn builds_window_and_pane_plan() -> Result<()> {
        let template = Template {
            root: Some("workspace".to_owned()),
            startup_window: Some("editor".to_owned()),
            startup_pane: Some(0),
            windows: vec![
                Window {
                    name: "editor".to_owned(),
                    cwd: Some("app".to_owned()),
                    pre_command: Some("source .venv/bin/activate".to_owned()),
                    command: Some("nvim".to_owned()),
                    layout: None,
                    synchronize: false,
                    panes: None,
                },
                Window {
                    name: "run".to_owned(),
                    cwd: None,
                    pre_command: None,
                    command: None,
                    layout: Some("main-horizontal".to_owned()),
                    synchronize: true,
                    panes: Some(vec![
                        Pane {
                            layout: None,
                            cwd: None,
                            command: Some("cargo run".to_owned()),
                        },
                        Pane {
                            layout: Some("right 30%".to_owned()),
                            cwd: Some("./server".to_owned()),
                            command: Some("cargo test".to_owned()),
                        },
                    ]),
                },
            ],
        };

        let plan = build_session_plan("demo", Path::new("/tmp/demo"), &template)?;
        assert_eq!(plan.startup_window, "editor");
        assert_eq!(plan.startup_pane, 0);
        assert_eq!(plan.windows.len(), 2);
        assert_eq!(plan.windows[0].cwd, Path::new("/tmp/demo/workspace/app"));
        assert_eq!(
            plan.windows[0].pre_command.as_deref(),
            Some("source .venv/bin/activate")
        );
        assert!(plan.windows[1].synchronize);
        assert_eq!(
            plan.windows[1].panes[1].cwd,
            Path::new("/tmp/demo/workspace/server")
        );
        Ok(())
    }

    #[test]
    fn rejects_startup_pane_out_of_range() {
        let template = Template {
            root: None,
            startup_window: Some("main".to_owned()),
            startup_pane: Some(2),
            windows: vec![Window {
                name: "main".to_owned(),
                cwd: None,
                pre_command: None,
                command: None,
                layout: None,
                synchronize: false,
                panes: Some(vec![Pane {
                    layout: None,
                    cwd: None,
                    command: None,
                }]),
            }],
        };

        let error = build_session_plan("demo", Path::new("/tmp/demo"), &template)
            .expect_err("startup pane should be validated");
        assert!(error.to_string().contains("startup_pane"));
    }

    #[test]
    fn rejects_invalid_pane_layout_string() {
        let template = Template {
            root: None,
            startup_window: Some("main".to_owned()),
            startup_pane: Some(0),
            windows: vec![Window {
                name: "main".to_owned(),
                cwd: None,
                pre_command: None,
                command: None,
                layout: None,
                synchronize: false,
                panes: Some(vec![
                    Pane {
                        layout: None,
                        cwd: None,
                        command: None,
                    },
                    Pane {
                        layout: Some("diagonal 30%".to_owned()),
                        cwd: None,
                        command: None,
                    },
                ]),
            }],
        };

        let error = build_session_plan("demo", Path::new("/tmp/demo"), &template)
            .expect_err("pane layout should be validated");
        assert!(error.to_string().contains("unknown pane layout position"));
    }
}
