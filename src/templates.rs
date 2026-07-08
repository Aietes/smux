use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::config::{Pane, Template, Window};
use crate::util;

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
    pub zoom: bool,
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
        detect: Vec::new(),
        match_dependencies: Vec::new(),
        priority: 0,
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

    validate_window_names(template)?;

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

/// Window names are interpolated into tmux `session:window` target strings and
/// addressed by name after creation, so a name containing the tmux target
/// separators (`:` or `.`) would mis-address later commands, and duplicate
/// names would resolve ambiguously. Reject both up front with a clear error.
pub(crate) fn validate_window_names(template: &Template) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for window in &template.windows {
        if window.name.contains(':') || window.name.contains('.') {
            bail!(
                "window name \"{}\" must not contain ':' or '.'",
                window.name
            );
        }
        if !seen.insert(window.name.as_str()) {
            bail!("duplicate window name \"{}\" in template", window.name);
        }
    }
    Ok(())
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
        zoom: pane.zoom,
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

pub(crate) fn validate_pane_layout(layout: &str) -> Result<()> {
    parse_pane_layout(Some(layout)).map(|_| ())
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
    let expanded = util::expand_tilde_path(Path::new(value));
    let path = expanded.as_path();
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
            detect: Vec::new(),
            match_dependencies: Vec::new(),
            priority: 0,
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
                            zoom: false,
                        },
                        Pane {
                            layout: Some("right 30%".to_owned()),
                            cwd: Some("./server".to_owned()),
                            command: Some("cargo test".to_owned()),
                            zoom: false,
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
    fn rejects_window_name_with_target_separators() {
        for bad in ["api:v1", "build.step"] {
            let template = Template {
                detect: Vec::new(),
                match_dependencies: Vec::new(),
                priority: 0,
                root: None,
                startup_window: None,
                startup_pane: None,
                windows: vec![Window {
                    name: bad.to_owned(),
                    cwd: None,
                    pre_command: None,
                    command: None,
                    layout: None,
                    synchronize: false,
                    panes: None,
                }],
            };

            let error = build_session_plan("demo", Path::new("/tmp/demo"), &template)
                .expect_err("window name with target separators should fail");
            assert!(error.to_string().contains("must not contain"));
        }
    }

    #[test]
    fn rejects_duplicate_window_names() {
        let window = || Window {
            name: "main".to_owned(),
            cwd: None,
            pre_command: None,
            command: None,
            layout: None,
            synchronize: false,
            panes: None,
        };
        let template = Template {
            detect: Vec::new(),
            match_dependencies: Vec::new(),
            priority: 0,
            root: None,
            startup_window: None,
            startup_pane: None,
            windows: vec![window(), window()],
        };

        let error = build_session_plan("demo", Path::new("/tmp/demo"), &template)
            .expect_err("duplicate window names should fail");
        assert!(error.to_string().contains("duplicate window name"));
    }

    #[test]
    fn rejects_startup_pane_out_of_range() {
        let template = Template {
            detect: Vec::new(),
            match_dependencies: Vec::new(),
            priority: 0,
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
                    zoom: false,
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
            detect: Vec::new(),
            match_dependencies: Vec::new(),
            priority: 0,
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
                        zoom: false,
                    },
                    Pane {
                        layout: Some("diagonal 30%".to_owned()),
                        cwd: None,
                        command: None,
                        zoom: false,
                    },
                ]),
            }],
        };

        let error = build_session_plan("demo", Path::new("/tmp/demo"), &template)
            .expect_err("pane layout should be validated");
        assert!(error.to_string().contains("unknown pane layout position"));
    }

    #[test]
    fn expands_tilde_window_and_pane_paths() -> Result<()> {
        let _guard = crate::util::test_env::lock();
        unsafe {
            std::env::set_var("HOME", "/Users/dev");
        }

        let template = Template {
            detect: Vec::new(),
            match_dependencies: Vec::new(),
            priority: 0,
            root: None,
            startup_window: Some("main".to_owned()),
            startup_pane: Some(0),
            windows: vec![Window {
                name: "main".to_owned(),
                cwd: Some("~/Development/smux".to_owned()),
                pre_command: None,
                command: None,
                layout: None,
                synchronize: false,
                panes: Some(vec![
                    Pane {
                        layout: None,
                        cwd: None,
                        command: None,
                        zoom: false,
                    },
                    Pane {
                        layout: Some("right".to_owned()),
                        cwd: Some("~/Development/nixpkgs".to_owned()),
                        command: None,
                        zoom: false,
                    },
                ]),
            }],
        };

        let plan = build_session_plan("demo", Path::new("/tmp/demo"), &template)?;
        assert_eq!(
            plan.windows[0].cwd,
            Path::new("/Users/dev/Development/smux")
        );
        assert_eq!(
            plan.windows[0].panes[1].cwd,
            Path::new("/Users/dev/Development/nixpkgs")
        );

        unsafe {
            std::env::remove_var("HOME");
        }

        Ok(())
    }
}
