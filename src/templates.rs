use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::config::{Pane, SplitDirection, Template, Window};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlan {
    pub session_name: String,
    pub windows: Vec<WindowPlan>,
    pub startup_window: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowPlan {
    pub name: String,
    pub cwd: PathBuf,
    pub command: Option<String>,
    pub layout: Option<String>,
    pub panes: Vec<PanePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanePlan {
    pub split: Option<SplitDirection>,
    pub size: Option<String>,
    pub cwd: PathBuf,
    pub command: Option<String>,
}

pub fn fallback_template() -> Template {
    Template {
        root: None,
        startup_window: Some("main".to_owned()),
        windows: vec![Window {
            name: "main".to_owned(),
            cwd: None,
            command: None,
            layout: None,
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

    Ok(SessionPlan {
        session_name: session_name.to_owned(),
        windows,
        startup_window,
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
        command: window.command.clone(),
        layout: window.layout.clone(),
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
        split: pane.split.clone(),
        size: pane.size.clone(),
        cwd,
        command: pane.command.clone(),
    })
}

fn resolve_root(session_root: &Path, root: Option<&str>) -> Result<PathBuf> {
    match root {
        Some(root) => resolve_relative(session_root, session_root, session_root, root),
        None => Ok(session_root.to_path_buf()),
    }
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
    use crate::config::{Pane, SplitDirection, Template, Window};
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
            windows: vec![
                Window {
                    name: "editor".to_owned(),
                    cwd: Some("app".to_owned()),
                    command: Some("nvim".to_owned()),
                    layout: None,
                    panes: None,
                },
                Window {
                    name: "run".to_owned(),
                    cwd: None,
                    command: None,
                    layout: Some("main-horizontal".to_owned()),
                    panes: Some(vec![
                        Pane {
                            split: None,
                            size: None,
                            cwd: None,
                            command: Some("cargo run".to_owned()),
                        },
                        Pane {
                            split: Some(SplitDirection::Vertical),
                            size: Some("30%".to_owned()),
                            cwd: Some("./server".to_owned()),
                            command: Some("cargo test".to_owned()),
                        },
                    ]),
                },
            ],
        };

        let plan = build_session_plan("demo", Path::new("/tmp/demo"), &template)?;
        assert_eq!(plan.startup_window, "editor");
        assert_eq!(plan.windows.len(), 2);
        assert_eq!(plan.windows[0].cwd, Path::new("/tmp/demo/workspace/app"));
        assert_eq!(
            plan.windows[1].panes[1].cwd,
            Path::new("/tmp/demo/workspace/server")
        );
        Ok(())
    }
}
