use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::util;

pub const STARTER_CONFIG: &str = r#"[settings]
default_template = "default"
icons = "auto"

[settings.icon_colors]
session = 75
directory = 108
template = 179
project = 81

[templates.default]
startup_window = "main"
windows = [{ name = "main" }]

[templates.rust]
startup_window = "editor"
startup_pane = 0
windows = [
  { name = "editor", pre_command = "source .venv/bin/activate", command = "nvim" },
  { name = "run", synchronize = true, layout = "main-horizontal", panes = [
      { command = "source .venv/bin/activate" },
      { command = "cargo run" },
      { layout = "right 40%", command = "cargo test" },
    ] },
]
"#;

pub const STARTER_PROJECT: &str = r#"path = "~/code/example"
session_name = "example"
template = "rust"
"#;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub templates: HashMap<String, Template>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Settings {
    pub default_template: Option<String>,
    #[serde(default)]
    pub icons: IconMode,
    #[serde(default)]
    pub icon_colors: IconColors,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum IconMode {
    #[default]
    Auto,
    Always,
    Never,
}

impl IconMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq)]
pub struct IconColors {
    pub session: u8,
    pub directory: u8,
    pub template: u8,
    pub project: u8,
}

impl Default for IconColors {
    fn default() -> Self {
        Self {
            session: 75,
            directory: 108,
            template: 179,
            project: 81,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Project {
    pub path: String,
    pub session_name: Option<String>,
    pub template: Option<String>,
    pub root: Option<String>,
    pub startup_window: Option<String>,
    pub startup_pane: Option<usize>,
    pub windows: Option<Vec<Window>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Template {
    pub root: Option<String>,
    pub startup_window: Option<String>,
    pub startup_pane: Option<usize>,
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Window {
    pub name: String,
    pub cwd: Option<String>,
    pub pre_command: Option<String>,
    pub command: Option<String>,
    pub layout: Option<String>,
    #[serde(default)]
    pub synchronize: bool,
    pub panes: Option<Vec<Pane>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pane {
    pub layout: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub config_exists: bool,
    pub project_dir: PathBuf,
    pub config: Config,
    pub projects: HashMap<String, Project>,
}

#[derive(Debug, Clone)]
pub struct ResolvedProject<'a> {
    pub name: &'a str,
    pub project: &'a Project,
    pub normalized_path: PathBuf,
}

pub fn default_config_dir() -> Result<PathBuf> {
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        Ok(PathBuf::from(config_home).join("smux"))
    } else {
        let home = std::env::var_os("HOME").context("could not resolve HOME for config path")?;
        Ok(PathBuf::from(home).join(".config").join("smux"))
    }
}

pub fn default_config_path() -> Result<PathBuf> {
    Ok(default_config_dir()?.join("config.toml"))
}

pub fn default_projects_dir() -> Result<PathBuf> {
    Ok(default_config_dir()?.join("projects"))
}

pub fn projects_dir_for_config_path(path: &Path) -> PathBuf {
    path.parent()
        .map(|parent| parent.join("projects"))
        .unwrap_or_else(|| PathBuf::from("projects"))
}

pub fn load(path: Option<&Path>) -> Result<LoadedConfig> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };

    if !path.exists() {
        bail!("failed to read config {}", path.display());
    }

    load_workspace(Some(&path))
}

pub fn load_workspace(path: Option<&Path>) -> Result<LoadedConfig> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };
    let project_dir = projects_dir_for_config_path(&path);
    let config_exists = path.exists();

    let config = if config_exists {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config: Config = toml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        validate_config(&config)?;
        config
    } else {
        Config::default()
    };

    let projects = load_projects(&project_dir, &config)?;

    Ok(LoadedConfig {
        path,
        config_exists,
        project_dir,
        config,
        projects,
    })
}

pub fn load_optional(path: Option<&Path>) -> Result<Option<LoadedConfig>> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };
    let project_dir = projects_dir_for_config_path(&path);

    if !path.exists() && !project_dir.exists() {
        return Ok(None);
    }

    load_workspace(Some(&path)).map(Some)
}

pub fn init(path: Option<&Path>) -> Result<PathBuf> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };

    if path.exists() {
        bail!("config already exists at {}", path.display());
    }

    let config_dir = path
        .parent()
        .context("config path did not have a parent directory")?;
    let project_dir = config_dir.join("projects");

    fs::create_dir_all(config_dir)
        .with_context(|| format!("failed to create config directory {}", config_dir.display()))?;
    fs::create_dir_all(&project_dir).with_context(|| {
        format!(
            "failed to create project directory {}",
            project_dir.display()
        )
    })?;

    fs::write(&path, STARTER_CONFIG)
        .with_context(|| format!("failed to write starter config to {}", path.display()))?;

    let starter_project_path = project_dir.join("example.toml");
    fs::write(&starter_project_path, STARTER_PROJECT).with_context(|| {
        format!(
            "failed to write starter project to {}",
            starter_project_path.display()
        )
    })?;

    Ok(path)
}

pub fn validate_config(config: &Config) -> Result<()> {
    for (template_name, template) in &config.templates {
        validate_template(template_name, template)?;
    }

    if let Some(default_template) = &config.settings.default_template
        && !config.templates.contains_key(default_template)
    {
        bail!("default_template \"{default_template}\" was not found");
    }

    Ok(())
}

fn validate_template(name: &str, template: &Template) -> Result<()> {
    if template.windows.is_empty() {
        bail!("{name} must contain at least one window");
    }

    if let Some(startup_window) = &template.startup_window
        && !template
            .windows
            .iter()
            .any(|window| window.name == *startup_window)
    {
        bail!("{name} references missing startup window \"{startup_window}\"");
    }

    for window in &template.windows {
        validate_window(name, window)?;
    }

    Ok(())
}

fn validate_window(owner_name: &str, window: &Window) -> Result<()> {
    if window.command.is_some() && window.panes.is_some() {
        bail!(
            "{owner_name} window \"{}\" cannot define both command and panes",
            window.name
        );
    }

    if let Some(panes) = &window.panes
        && panes.is_empty()
    {
        bail!(
            "{owner_name} window \"{}\" cannot define an empty panes array",
            window.name
        );
    }

    Ok(())
}

fn load_projects(project_dir: &Path, config: &Config) -> Result<HashMap<String, Project>> {
    if !project_dir.exists() {
        return Ok(HashMap::new());
    }

    let mut files = fs::read_dir(project_dir)
        .with_context(|| format!("failed to read project directory {}", project_dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to read project directory {}", project_dir.display()))?;
    files.sort_by_key(|entry| entry.file_name());

    let mut projects = HashMap::new();

    for entry in files {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .context("project file name was not valid utf-8")?
            .to_owned();

        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read project {}", path.display()))?;
        let project: Project = toml::from_str(&text)
            .with_context(|| format!("failed to parse project {}", path.display()))?;
        validate_project(&name, &project, config)?;
        projects.insert(name, project);
    }

    Ok(projects)
}

fn validate_project(name: &str, project: &Project, config: &Config) -> Result<()> {
    util::expand_and_absolutize_path(Path::new(&project.path))
        .with_context(|| format!("project \"{name}\" has an invalid path {}", project.path))?;

    if let Some(template_name) = &project.template
        && !config.templates.contains_key(template_name)
    {
        bail!("template \"{template_name}\" referenced by project \"{name}\" was not found");
    }

    let has_direct_session_definition = project.root.is_some()
        || project.startup_window.is_some()
        || project.startup_pane.is_some()
        || project.windows.is_some();

    if has_direct_session_definition {
        let effective = materialize_project_template(config, project)?
            .context("project materialization unexpectedly returned no template")?;
        validate_template(&format!("project \"{name}\""), &effective)?;
    }

    Ok(())
}

pub fn materialize_project_template(
    config: &Config,
    project: &Project,
) -> Result<Option<Template>> {
    let base = match &project.template {
        Some(template_name) => Some(
            config
                .templates
                .get(template_name)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("unknown template: {template_name}"))?,
        ),
        None => None,
    };

    let has_direct_session_definition = project.root.is_some()
        || project.startup_window.is_some()
        || project.startup_pane.is_some()
        || project.windows.is_some();

    if !has_direct_session_definition {
        return Ok(base);
    }

    let mut effective = base.unwrap_or(Template {
        root: None,
        startup_window: None,
        startup_pane: None,
        windows: Vec::new(),
    });

    if let Some(root) = &project.root {
        effective.root = Some(root.clone());
    }
    if let Some(startup_window) = &project.startup_window {
        effective.startup_window = Some(startup_window.clone());
    }
    if let Some(startup_pane) = project.startup_pane {
        effective.startup_pane = Some(startup_pane);
    }
    if let Some(windows) = &project.windows {
        effective.windows = windows.clone();
    }

    Ok(Some(effective))
}

pub fn resolve_project<'a>(
    loaded: &'a LoadedConfig,
    path: &Path,
) -> Result<Option<ResolvedProject<'a>>> {
    let normalized = util::expand_and_normalize_path(path)?;

    for (name, project) in &loaded.projects {
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
    use super::{
        Config, IconColors, IconMode, STARTER_CONFIG, STARTER_PROJECT, default_projects_dir, load,
        load_optional, load_workspace, materialize_project_template, resolve_project,
        validate_config,
    };
    use anyhow::Result;
    use std::fs;
    use std::path::Path;

    #[test]
    fn parses_starter_config() -> Result<()> {
        let config: Config = toml::from_str(STARTER_CONFIG)?;
        validate_config(&config)?;
        assert!(config.templates.contains_key("default"));
        assert_eq!(config.settings.icons, IconMode::Auto);
        assert_eq!(config.settings.icon_colors, IconColors::default());
        Ok(())
    }

    #[test]
    fn parses_starter_project() -> Result<()> {
        let project: super::Project = toml::from_str(STARTER_PROJECT)?;
        assert_eq!(project.session_name.as_deref(), Some("example"));
        assert_eq!(project.template.as_deref(), Some("rust"));
        Ok(())
    }

    #[test]
    fn parses_inline_table_windows_and_panes() -> Result<()> {
        let input = r#"
[templates.default]
startup_window = "main"
windows = [
  { name = "main" },
  { name = "run", panes = [
      { command = "cargo run" },
      { layout = "right 40%", command = "cargo test" },
    ] },
]
"#;

        let config: Config = toml::from_str(input)?;
        validate_config(&config)?;
        assert_eq!(config.templates["default"].windows.len(), 2);
        assert_eq!(
            config.templates["default"].windows[1]
                .panes
                .as_ref()
                .expect("panes should exist")
                .len(),
            2
        );
        Ok(())
    }

    #[test]
    fn rejects_missing_project_template() {
        let config = Config::default();
        let project: super::Project =
            toml::from_str("path = \"/tmp/demo\"\ntemplate = \"missing\"\n")
                .expect("project should parse");
        let error =
            super::validate_project("demo", &project, &config).expect_err("validation should fail");
        assert!(error.to_string().contains("referenced by project"));
    }

    #[test]
    fn resolves_project_by_normalized_path() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let config_path = tempdir.path().join("config.toml");
        let project_dir = tempdir.path().join("projects");
        let workspace_dir = tempdir.path().join("demo");
        fs::create_dir(&workspace_dir)?;
        fs::create_dir(&project_dir)?;

        fs::write(
            &config_path,
            r#"
[templates.default]
windows = [{ name = "main" }]
"#,
        )?;
        fs::write(
            project_dir.join("demo.toml"),
            format!(
                "path = \"{}\"\ntemplate = \"default\"\n",
                workspace_dir.display()
            ),
        )?;

        let loaded = load_workspace(Some(&config_path))?;
        let resolved =
            resolve_project(&loaded, Path::new(&workspace_dir))?.expect("project should resolve");
        assert_eq!(resolved.name, "demo");

        Ok(())
    }

    #[test]
    fn materializes_project_overrides_on_template() -> Result<()> {
        let config: Config = toml::from_str(
            r#"
[templates.default]
startup_window = "main"
windows = [{ name = "main" }]
"#,
        )?;

        let project: super::Project = toml::from_str(
            r#"
path = "/tmp/demo"
template = "default"
startup_window = "editor"
windows = [{ name = "editor", command = "nvim" }]
"#,
        )?;

        let materialized = materialize_project_template(&config, &project)?
            .expect("project should materialize a template");
        assert_eq!(materialized.startup_window.as_deref(), Some("editor"));
        assert_eq!(materialized.windows[0].name, "editor");
        Ok(())
    }

    #[test]
    fn loads_from_disk_with_projects() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let path = tempdir.path().join("config.toml");
        let project_dir = tempdir.path().join("projects");
        fs::create_dir(&project_dir)?;
        fs::write(&path, STARTER_CONFIG)?;
        fs::write(project_dir.join("example.toml"), STARTER_PROJECT)?;

        let loaded = load(Some(&path))?;
        assert_eq!(loaded.path, path);
        assert!(loaded.projects.contains_key("example"));
        Ok(())
    }

    #[test]
    fn loads_projects_without_main_config() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let path = tempdir.path().join("config.toml");
        let project_dir = tempdir.path().join("projects");
        fs::create_dir(&project_dir)?;
        fs::write(
            project_dir.join("example.toml"),
            r#"
path = "/tmp/example"
session_name = "example"
windows = [{ name = "main", command = "nvim" }]
"#,
        )?;

        let loaded = load_optional(Some(&path))?.expect("workspace should load");
        assert!(!loaded.config_exists);
        assert!(loaded.projects.contains_key("example"));
        Ok(())
    }

    #[test]
    fn init_creates_project_directory_and_starter_project() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let path = tempdir.path().join("config.toml");

        let written = super::init(Some(&path))?;
        assert_eq!(written, path);
        assert!(tempdir.path().join("projects").is_dir());
        assert!(
            tempdir
                .path()
                .join("projects")
                .join("example.toml")
                .exists()
        );
        Ok(())
    }

    #[test]
    fn uses_xdg_config_home_when_set() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", tempdir.path());
        }

        let path = super::default_config_path()?;
        assert_eq!(path, tempdir.path().join("smux").join("config.toml"));
        assert_eq!(
            default_projects_dir()?,
            tempdir.path().join("smux").join("projects")
        );

        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        Ok(())
    }
}
