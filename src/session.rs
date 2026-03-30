use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::Config;
use crate::tmux::Tmux;
use crate::util;

pub fn connect_path(
    tmux: &Tmux,
    path: &Path,
    config: Option<&Config>,
    override_template: Option<&str>,
    override_name: Option<&str>,
) -> Result<()> {
    let normalized = util::normalize_path(path)?;
    let resolved_project = match config {
        Some(config) => crate::config::resolve_project(config, &normalized)?,
        None => None,
    };

    let _template = resolve_template_name(config, override_template, resolved_project.as_ref())?;

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

    tmux.create_session(&session_name, &normalized)?;
    tmux.switch_or_attach(&session_name)
}

fn resolve_template_name(
    config: Option<&Config>,
    override_template: Option<&str>,
    project: Option<&crate::config::ResolvedProject<'_>>,
) -> Result<Option<String>> {
    if let Some(template_name) = override_template {
        let config = config.context("explicit --template requires a config file with templates")?;
        ensure_template_exists(config, template_name)?;
        return Ok(Some(template_name.to_owned()));
    }

    if let Some(project) = project
        && let Some(template_name) = &project.project.template
    {
        return Ok(Some(template_name.clone()));
    }

    if let Some(config) = config
        && let Some(template_name) = &config.settings.default_template
    {
        return Ok(Some(template_name.clone()));
    }

    Ok(None)
}

fn ensure_template_exists(config: &Config, template_name: &str) -> Result<()> {
    if config.templates.contains_key(template_name) {
        Ok(())
    } else {
        bail!("unknown template: {template_name}")
    }
}

pub fn switch_existing(tmux: &Tmux, session: &str) -> Result<()> {
    let session = util::validated_session_name(session)?;
    tmux.ensure_session_exists(&session)?;
    tmux.switch_or_attach(&session)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::config::{Config, Project, ResolvedProject, Settings, Template, Window};
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
        let config = Config {
            settings: Settings::default(),
            templates: HashMap::new(),
            projects: HashMap::from([(
                "demo".to_owned(),
                Project {
                    path: "/tmp/demo".to_owned(),
                    template: None,
                    session_name: Some("demo-session".to_owned()),
                },
            )]),
        };

        let project = ResolvedProject {
            name: "demo",
            project: config.projects.get("demo").expect("project exists"),
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
                    windows: vec![Window {
                        name: "main".to_owned(),
                        cwd: None,
                        command: None,
                        layout: None,
                        panes: None,
                    }],
                },
            )]),
            projects: HashMap::new(),
        };

        let error = super::resolve_template_name(Some(&config), Some("missing"), None)
            .expect_err("missing template should fail");
        assert!(error.to_string().contains("unknown template"));
    }
}
