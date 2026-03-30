use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::util;

pub const STARTER_CONFIG: &str = r#"[settings]
default_template = "default"

[templates.default]
startup_window = "main"

[[templates.default.windows]]
name = "main"

[templates.rust]
startup_window = "editor"

[[templates.rust.windows]]
name = "editor"
command = "nvim"

[[templates.rust.windows]]
name = "run"
layout = "main-horizontal"

[[templates.rust.windows.panes]]
command = "cargo run"

[[templates.rust.windows.panes]]
split = "vertical"
command = "cargo test"

[projects.example]
path = "~/code/example"
template = "rust"
session_name = "example"
"#;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub templates: HashMap<String, Template>,
    #[serde(default)]
    pub projects: HashMap<String, Project>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Settings {
    pub default_template: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub path: String,
    pub template: Option<String>,
    pub session_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Template {
    pub root: Option<String>,
    pub startup_window: Option<String>,
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Window {
    pub name: String,
    pub cwd: Option<String>,
    pub command: Option<String>,
    pub layout: Option<String>,
    pub panes: Option<Vec<Pane>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pane {
    pub split: Option<SplitDirection>,
    pub size: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub struct ResolvedProject<'a> {
    pub name: &'a str,
    pub project: &'a Project,
    pub normalized_path: PathBuf,
}

pub fn default_config_path() -> Result<PathBuf> {
    let base = if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(config_home)
    } else {
        let home = std::env::var_os("HOME").context("could not resolve HOME for config path")?;
        PathBuf::from(home).join(".config")
    };

    Ok(base.join("swux").join("config.toml"))
}

pub fn load(path: Option<&Path>) -> Result<LoadedConfig> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };

    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    let config: Config = toml::from_str(&text)
        .with_context(|| format!("failed to parse config {}", path.display()))?;
    validate(&config)?;

    Ok(LoadedConfig { path, config })
}

pub fn load_optional(path: Option<&Path>) -> Result<Option<LoadedConfig>> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };

    if !path.exists() {
        return Ok(None);
    }

    load(Some(&path)).map(Some)
}

pub fn init(path: Option<&Path>) -> Result<PathBuf> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };

    if path.exists() {
        bail!("config already exists at {}", path.display());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }

    fs::write(&path, STARTER_CONFIG)
        .with_context(|| format!("failed to write starter config to {}", path.display()))?;

    Ok(path)
}

pub fn validate(config: &Config) -> Result<()> {
    for (template_name, template) in &config.templates {
        if template.windows.is_empty() {
            bail!("template \"{template_name}\" must contain at least one window");
        }

        if let Some(startup_window) = &template.startup_window
            && !template
                .windows
                .iter()
                .any(|window| window.name == *startup_window)
        {
            bail!(
                "template \"{template_name}\" references missing startup window \"{startup_window}\""
            );
        }

        for window in &template.windows {
            if window.command.is_some() && window.panes.is_some() {
                bail!(
                    "template \"{template_name}\" window \"{}\" cannot define both command and panes",
                    window.name
                );
            }

            if let Some(panes) = &window.panes
                && panes.is_empty()
            {
                bail!(
                    "template \"{template_name}\" window \"{}\" cannot define an empty panes array",
                    window.name
                );
            }
        }
    }

    if let Some(default_template) = &config.settings.default_template
        && !config.templates.contains_key(default_template)
    {
        bail!("default_template \"{default_template}\" was not found");
    }

    for (project_name, project) in &config.projects {
        util::expand_and_absolutize_path(Path::new(&project.path)).with_context(|| {
            format!(
                "project \"{project_name}\" has an invalid path {}",
                project.path
            )
        })?;

        if let Some(template_name) = &project.template
            && !config.templates.contains_key(template_name)
        {
            bail!(
                "template \"{template_name}\" referenced by project \"{project_name}\" was not found"
            );
        }
    }

    Ok(())
}

pub fn resolve_project<'a>(config: &'a Config, path: &Path) -> Result<Option<ResolvedProject<'a>>> {
    let normalized = util::expand_and_normalize_path(path)?;

    for (name, project) in &config.projects {
        let project_path = util::expand_and_absolutize_path(Path::new(&project.path))?;
        if project_path == normalized {
            return Ok(Some(ResolvedProject {
                name,
                project,
                normalized_path: project_path,
            }));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{Config, STARTER_CONFIG, load, resolve_project, validate};
    use anyhow::Result;
    use std::fs;
    use std::path::Path;

    #[test]
    fn parses_starter_config() -> Result<()> {
        let config: Config = toml::from_str(STARTER_CONFIG)?;
        validate(&config)?;
        assert!(config.templates.contains_key("default"));
        assert!(config.projects.contains_key("example"));
        Ok(())
    }

    #[test]
    fn rejects_missing_project_template() {
        let input = r#"
[projects.demo]
path = "/tmp/demo"
template = "missing"
"#;

        let config: Config = toml::from_str(input).expect("config should parse");
        let error = validate(&config).expect_err("validation should fail");
        assert!(error.to_string().contains("referenced by project"));
    }

    #[test]
    fn resolves_project_by_normalized_path() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let project_dir = tempdir.path().join("demo");
        fs::create_dir(&project_dir)?;

        let input = format!(
            r#"
[templates.default]
[[templates.default.windows]]
name = "main"

[projects.demo]
path = "{}"
template = "default"
"#,
            project_dir.display()
        );

        let config: Config = toml::from_str(&input)?;
        validate(&config)?;

        let resolved =
            resolve_project(&config, Path::new(&project_dir))?.expect("project should resolve");
        assert_eq!(resolved.name, "demo");

        Ok(())
    }

    #[test]
    fn loads_from_disk() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let path = tempdir.path().join("config.toml");
        fs::write(&path, STARTER_CONFIG)?;

        let loaded = load(Some(&path))?;
        assert_eq!(loaded.path, path);
        Ok(())
    }

    #[test]
    fn uses_xdg_config_home_when_set() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", tempdir.path());
        }

        let path = super::default_config_path()?;
        assert_eq!(path, tempdir.path().join("swux").join("config.toml"));

        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        Ok(())
    }
}
