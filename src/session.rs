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

    let template = resolve_template(
        loaded,
        override_template,
        resolved_project.as_ref(),
        &normalized,
    )?;

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
    path: &Path,
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

    // No explicit, project, or default template: if the directory looks like a
    // known project type and a same-named template exists, use it.
    if let Some(loaded) = loaded
        && let Some(template_name) = detect_template_name(&loaded.config, path)
    {
        return load_template(&loaded.config, &template_name);
    }

    Ok(templates::fallback_template())
}

/// Detect a template name from marker files in `path`, returning it only when a
/// template with that conventional name actually exists in the config.
fn detect_template_name(config: &crate::config::Config, path: &Path) -> Option<String> {
    const MARKERS: &[(&str, &str)] = &[
        ("Cargo.toml", "rust"),
        ("package.json", "node"),
        ("go.mod", "go"),
        ("pyproject.toml", "python"),
        ("requirements.txt", "python"),
        ("Gemfile", "ruby"),
        ("pom.xml", "java"),
        ("build.gradle", "java"),
    ];

    MARKERS.iter().find_map(|(marker, template)| {
        if config.templates.contains_key(*template) && path.join(marker).exists() {
            Some((*template).to_owned())
        } else {
            None
        }
    })
}

/// Decide whether opening `path` from the picker should prompt for a template
/// rather than resolving one silently. We prompt only when nothing would resolve
/// automatically — no `default_template` and no marker-file match — and there are
/// at least two templates worth choosing between. This keeps the common path (a
/// configured default, or a detected project type) a single keystroke, and offers
/// a choice only when smux would otherwise fall back to the built-in template.
pub fn should_offer_template_choice(loaded: Option<&LoadedConfig>, path: &Path) -> bool {
    let Some(loaded) = loaded else {
        return false;
    };
    let config = &loaded.config;
    if config.settings.default_template.is_some() {
        return false;
    }
    if detect_template_name(config, path).is_some() {
        return false;
    }
    config.templates.len() >= 2
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

pub fn rename_existing(tmux: &Tmux, session: &str, new_name: &str) -> Result<String> {
    let session = util::validated_session_name(session)?;
    tmux.ensure_session_exists(&session)?;
    let new_name = util::validated_session_name(new_name)?;
    if new_name == session {
        return Ok(new_name);
    }
    if tmux.has_session(&new_name)? {
        anyhow::bail!("a tmux session named {new_name} already exists");
    }
    tmux.rename_session(&session, &new_name)?;
    Ok(new_name)
}

pub fn switch_last(tmux: &Tmux) -> Result<()> {
    tmux.switch_to_last()
}

/// Kill every detached session, returning the names that were killed.
pub fn prune_detached(tmux: &Tmux) -> Result<Vec<String>> {
    let detached = tmux.list_detached_sessions()?;
    for session in &detached {
        tmux.kill_session(session)?;
    }
    Ok(detached)
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
    use std::path::{Path, PathBuf};

    #[test]
    fn sanitizes_session_names() {
        assert_eq!(util::sanitize_session_name("my app"), "my_app");
        assert_eq!(util::sanitize_session_name("api:v1"), "api_v1");
        assert_eq!(util::sanitize_session_name("foo.bar"), "foo_bar");
    }

    #[test]
    fn detects_template_from_marker_file_only_when_template_exists() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        std::fs::write(tempdir.path().join("Cargo.toml"), "")?;

        let rust_template = Template {
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
        };

        let with_template = Config {
            settings: Settings::default(),
            templates: HashMap::from([("rust".to_owned(), rust_template)]),
        };
        assert_eq!(
            super::detect_template_name(&with_template, tempdir.path()).as_deref(),
            Some("rust")
        );

        // A marker with no correspondingly named template detects nothing.
        let without_template = Config {
            settings: Settings::default(),
            templates: HashMap::new(),
        };
        assert!(super::detect_template_name(&without_template, tempdir.path()).is_none());

        // No marker file detects nothing.
        let empty_dir = tempfile::tempdir()?;
        assert!(super::detect_template_name(&with_template, empty_dir.path()).is_none());
        Ok(())
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
            template_dir: PathBuf::from("/tmp/templates"),
            config,
            projects: HashMap::new(),
            project_files: HashMap::new(),
            invalid_projects: Vec::new(),
            template_files: HashMap::new(),
            invalid_templates: Vec::new(),
        };

        let error =
            super::resolve_template(Some(&loaded), Some("missing"), None, Path::new("/tmp/demo"))
                .expect_err("missing template should fail");
        assert!(error.to_string().contains("unknown template"));
    }

    fn loaded_with(default_template: Option<&str>, template_names: &[&str]) -> LoadedConfig {
        let templates = template_names
            .iter()
            .map(|name| {
                (
                    (*name).to_owned(),
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
                )
            })
            .collect();

        LoadedConfig {
            path: PathBuf::from("/tmp/config.toml"),
            config_exists: true,
            project_dir: PathBuf::from("/tmp/projects"),
            template_dir: PathBuf::from("/tmp/templates"),
            config: Config {
                settings: Settings {
                    default_template: default_template.map(|name| name.to_owned()),
                    ..Default::default()
                },
                templates,
            },
            projects: HashMap::new(),
            project_files: HashMap::new(),
            invalid_projects: Vec::new(),
            template_files: HashMap::new(),
            invalid_templates: Vec::new(),
        }
    }

    #[test]
    fn offers_template_choice_when_no_default_and_multiple_templates() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let loaded = loaded_with(None, &["default", "rust"]);
        assert!(super::should_offer_template_choice(
            Some(&loaded),
            tempdir.path()
        ));
        Ok(())
    }

    #[test]
    fn no_template_choice_when_default_template_set() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let loaded = loaded_with(Some("default"), &["default", "rust"]);
        assert!(!super::should_offer_template_choice(
            Some(&loaded),
            tempdir.path()
        ));
        Ok(())
    }

    #[test]
    fn no_template_choice_when_marker_file_resolves_a_template() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        std::fs::write(tempdir.path().join("Cargo.toml"), "")?;
        let loaded = loaded_with(None, &["rust", "node"]);
        assert!(!super::should_offer_template_choice(
            Some(&loaded),
            tempdir.path()
        ));
        Ok(())
    }

    #[test]
    fn no_template_choice_with_fewer_than_two_templates() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let loaded = loaded_with(None, &["rust"]);
        assert!(!super::should_offer_template_choice(
            Some(&loaded),
            tempdir.path()
        ));
        Ok(())
    }

    #[test]
    fn no_template_choice_without_config() {
        assert!(!super::should_offer_template_choice(
            None,
            Path::new("/tmp/demo")
        ));
    }

    #[test]
    fn falls_back_to_builtin_template_without_config() -> Result<()> {
        let template = super::resolve_template(None, None, None, Path::new("/tmp/demo"))?;
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
            template_dir: PathBuf::from("/tmp/templates"),
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
            project_files: HashMap::new(),
            invalid_projects: Vec::new(),
            template_files: HashMap::new(),
            invalid_templates: Vec::new(),
        };

        let project = ResolvedProject {
            name: "demo",
            project: loaded.projects.get("demo").expect("project exists"),
            normalized_path: PathBuf::from("/tmp/demo"),
        };

        let template = super::resolve_template(
            Some(&loaded),
            Some("explicit"),
            Some(&project),
            Path::new("/tmp/demo"),
        )?;
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
            template_dir: PathBuf::from("/tmp/templates"),
            config,
            projects: HashMap::new(),
            project_files: HashMap::new(),
            invalid_projects: Vec::new(),
            template_files: HashMap::new(),
            invalid_templates: Vec::new(),
        };

        let template = super::resolve_template(Some(&loaded), None, None, Path::new("/tmp/demo"))?;
        assert_eq!(template.windows[0].name, "default-window");
        Ok(())
    }
}
