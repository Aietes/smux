use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{LoadedConfig, Template};
use crate::templates;
use crate::tmux::Tmux;
use crate::util;

pub const BUILTIN_TEMPLATE_NAME: &str = "__builtin__";

pub fn connect_path(
    tmux: &Tmux,
    path: &Path,
    loaded: Option<&LoadedConfig>,
    override_template: Option<&str>,
    override_name: Option<&str>,
    project_detection: ProjectDetection,
) -> Result<()> {
    let normalized = util::normalize_path(path)?;
    let resolved_project = match (loaded, project_detection) {
        (_, ProjectDetection::Disabled) => None,
        (Some(loaded), _) => crate::config::resolve_project(loaded, &normalized)?,
        (None, _) => None,
    };

    let template = resolve_template(loaded, override_template, resolved_project.as_ref())?;

    let session_name = match override_name {
        Some(name) => util::validated_session_name(name)?,
        None => match resolved_project
            .as_ref()
            .and_then(|project| project.project.session_name.as_deref())
        {
            Some(name) => util::validated_session_name(name)?,
            None => util::session_name_from_path(&normalized)?,
        },
    };

    if tmux.has_session(&session_name)? {
        return tmux.switch_or_attach(&session_name);
    }

    let plan = templates::build_session_plan(&session_name, &normalized, &template)?;
    tmux.create_session_from_plan(&plan)?;
    tmux.switch_or_attach(&session_name)
}

pub fn connect_project(tmux: &Tmux, loaded: &LoadedConfig, project_name: &str) -> Result<()> {
    let project = loaded
        .projects
        .get(project_name)
        .with_context(|| format!("unknown project: {project_name}"))?;
    connect_path(
        tmux,
        Path::new(&project.path),
        Some(loaded),
        None,
        project.session_name.as_deref(),
        ProjectDetection::Enabled,
    )
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProjectDetection {
    Enabled,
    Disabled,
}

fn resolve_template(
    loaded: Option<&LoadedConfig>,
    override_template: Option<&str>,
    project: Option<&crate::config::ResolvedProject<'_>>,
) -> Result<Template> {
    if let Some(template_name) = override_template {
        if template_name == BUILTIN_TEMPLATE_NAME {
            return Ok(templates::fallback_template());
        }

        let loaded = loaded.context("explicit --template requires a config file with templates")?;
        return load_template(&loaded.config, template_name);
    }

    if let Some(project) = project {
        let loaded = loaded.context("project template resolution requires config")?;
        if let Some(template) =
            crate::config::materialize_project_template(&loaded.config, project.project)?
        {
            return Ok(template);
        }
    }

    if let Some(loaded) = loaded
        && let Some(template_name) = &loaded.config.settings.default_template
    {
        return load_template(&loaded.config, template_name);
    }

    Ok(templates::fallback_template())
}

fn load_template(config: &crate::config::Config, template_name: &str) -> Result<Template> {
    config
        .templates
        .get(template_name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("unknown template: {template_name}"))
}

pub fn switch_existing(tmux: &Tmux, session: &str) -> Result<()> {
    let session = util::validated_session_name(session)?;
    tmux.ensure_session_exists(&session)?;
    tmux.switch_or_attach(&session)
}

pub fn kill_existing(tmux: &Tmux, session: &str) -> Result<()> {
    let session = util::validated_session_name(session)?;
    tmux.ensure_session_exists(&session)?;
    tmux.kill_session(&session)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::config::{
        Config, LoadedConfig, Project, ResolvedProject, Settings, Template, Window,
    };
    use crate::templates;
    use crate::util;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn sanitizes_session_names() {
        assert_eq!(util::sanitize_session_name("my app"), "my_app");
        assert_eq!(util::sanitize_session_name("api:v1"), "api_v1");
        assert_eq!(util::sanitize_session_name("foo.bar"), "foo_bar");
    }

    #[test]
    fn derives_session_name_from_path() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let directory = tempdir.path().join("my-project");
        std::fs::create_dir(&directory)?;

        let session = util::session_name_from_path(&directory)?;
        assert_eq!(session, "my-project");

        Ok(())
    }

    #[test]
    fn prefers_project_session_name_when_available() -> Result<()> {
        let projects = HashMap::from([(
            "demo".to_owned(),
            Project {
                path: "/tmp/demo".to_owned(),
                template: None,
                session_name: Some("demo-session".to_owned()),
                root: None,
                startup_window: None,
                startup_pane: None,
                windows: None,
            },
        )]);

        let project = ResolvedProject {
            name: "demo",
            project: projects.get("demo").expect("project exists"),
            normalized_path: PathBuf::from("/tmp/demo"),
        };

        let name = match project.project.session_name.as_deref() {
            Some(name) => util::validated_session_name(name)?,
            None => unreachable!(),
        };

        assert_eq!(name, "demo-session");
        Ok(())
    }

    #[test]
    fn explicit_template_must_exist() {
        let config = Config {
            settings: Settings::default(),
            templates: HashMap::from([(
                "default".to_owned(),
                Template {
                    root: None,
                    startup_window: None,
                    startup_pane: None,
                    windows: vec![Window {
                        name: "main".to_owned(),
                        cwd: None,
                        pre_command: None,
                        command: None,
                        layout: None,
                        synchronize: false,
                        panes: None,
                    }],
                },
            )]),
        };

        let loaded = LoadedConfig {
            path: PathBuf::from("/tmp/config.toml"),
            config_exists: true,
            project_dir: PathBuf::from("/tmp/projects"),
            config,
            projects: HashMap::new(),
        };

        let error = super::resolve_template(Some(&loaded), Some("missing"), None)
            .expect_err("missing template should fail");
        assert!(error.to_string().contains("unknown template"));
    }

    #[test]
    fn falls_back_to_builtin_template_without_config() -> Result<()> {
        let template = super::resolve_template(None, None, None)?;
        assert_eq!(template.windows.len(), 1);
        assert_eq!(
            template.windows[0].name,
            templates::fallback_template().windows[0].name
        );
        Ok(())
    }

    #[test]
    fn project_detection_can_be_disabled() {
        let disabled = super::ProjectDetection::Disabled;
        assert_eq!(disabled, super::ProjectDetection::Disabled);
    }

    #[test]
    fn explicit_template_overrides_project_and_default() -> Result<()> {
        let loaded = LoadedConfig {
            path: PathBuf::from("/tmp/config.toml"),
            config_exists: true,
            project_dir: PathBuf::from("/tmp/projects"),
            config: Config {
                settings: Settings {
                    default_template: Some("default".to_owned()),
                    ..Default::default()
                },
                templates: HashMap::from([
                    (
                        "default".to_owned(),
                        Template {
                            root: None,
                            startup_window: None,
                            startup_pane: None,
                            windows: vec![Window {
                                name: "default-window".to_owned(),
                                cwd: None,
                                pre_command: None,
                                command: None,
                                layout: None,
                                synchronize: false,
                                panes: None,
                            }],
                        },
                    ),
                    (
                        "project".to_owned(),
                        Template {
                            root: None,
                            startup_window: None,
                            startup_pane: None,
                            windows: vec![Window {
                                name: "project-window".to_owned(),
                                cwd: None,
                                pre_command: None,
                                command: None,
                                layout: None,
                                synchronize: false,
                                panes: None,
                            }],
                        },
                    ),
                    (
                        "explicit".to_owned(),
                        Template {
                            root: None,
                            startup_window: None,
                            startup_pane: None,
                            windows: vec![Window {
                                name: "explicit-window".to_owned(),
                                cwd: None,
                                pre_command: None,
                                command: None,
                                layout: None,
                                synchronize: false,
                                panes: None,
                            }],
                        },
                    ),
                ]),
            },
            projects: HashMap::from([(
                "demo".to_owned(),
                Project {
                    path: "/tmp/demo".to_owned(),
                    template: Some("project".to_owned()),
                    session_name: None,
                    root: None,
                    startup_window: None,
                    startup_pane: None,
                    windows: None,
                },
            )]),
        };

        let project = ResolvedProject {
            name: "demo",
            project: loaded.projects.get("demo").expect("project exists"),
            normalized_path: PathBuf::from("/tmp/demo"),
        };

        let template = super::resolve_template(Some(&loaded), Some("explicit"), Some(&project))?;
        assert_eq!(template.windows[0].name, "explicit-window");
        Ok(())
    }

    #[test]
    fn default_template_applies_without_project_or_override() -> Result<()> {
        let config = Config {
            settings: Settings {
                default_template: Some("default".to_owned()),
                ..Default::default()
            },
            templates: HashMap::from([(
                "default".to_owned(),
                Template {
                    root: None,
                    startup_window: None,
                    startup_pane: None,
                    windows: vec![Window {
                        name: "default-window".to_owned(),
                        cwd: None,
                        pre_command: None,
                        command: None,
                        layout: None,
                        synchronize: false,
                        panes: None,
                    }],
                },
            )]),
        };

        let loaded = LoadedConfig {
            path: PathBuf::from("/tmp/config.toml"),
            config_exists: true,
            project_dir: PathBuf::from("/tmp/projects"),
            config,
            projects: HashMap::new(),
        };

        let template = super::resolve_template(Some(&loaded), None, None)?;
        assert_eq!(template.windows[0].name, "default-window");
        Ok(())
    }
}
