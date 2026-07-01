use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::util;

const MAX_FOLDER_SEARCH_DEPTH: usize = 16;

const STARTER_CONFIG_BODY: &str = r#"[settings]
default_template = "default"
icons = "auto"

[settings.icon_colors]
session = 75
directory = 108
template = 179
project = 81

[settings.picker]
show_hints = true

[settings.picker.bindings]
reset = "ctrl-c"
sessions = "ctrl-s"
folders = "ctrl-f"
projects = "ctrl-p"
delete_session = "ctrl-x"
save_project = "alt-s"
rename_session = "ctrl-r"
edit_project = "ctrl-e"
toggle_hints = "?"

[settings.picker.preview]
# sessions = "tmux capture-pane -p -t \"$SMUX_PREVIEW_SESSION\""
# folders = "eza --tree --level=2 --color=always --icons=always \"$SMUX_PREVIEW_PATH\""
# projects = "bat --style=plain --color=always --language=toml \"$SMUX_PREVIEW_FILE\""

[settings.folder_search]
# roots = ["~"]
# max_depth = 3
# include_hidden = false
"#;

const STARTER_PROJECT_BODY: &str = r#"path = "~/code/example"
session_name = "example"
template = "rust"
"#;

const STARTER_TEMPLATE_DEFAULT_BODY: &str = r#"startup_window = "main"
windows = [{ name = "main" }]
"#;

const STARTER_TEMPLATE_RUST_BODY: &str = r#"startup_window = "editor"
startup_pane = 0
windows = [
  { name = "editor", pre_command = "source .venv/bin/activate", command = "nvim" },
  { name = "run", synchronize = true, layout = "main-horizontal", panes = [
      { command = "source .venv/bin/activate" },
      { layout = "bottom", command = "cargo run" },
      { layout = "right 40%", command = "cargo test" },
    ] },
]
"#;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    /// Templates are loaded from `templates/*.toml` files and merged in at load
    /// time. Inline `[templates.*]` tables in `config.toml` are rejected during
    /// loading; the field is kept only so the loader can populate it from files.
    #[serde(default)]
    pub templates: HashMap<String, Template>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub default_template: Option<String>,
    #[serde(default)]
    pub icons: IconMode,
    #[serde(default)]
    pub icon_colors: IconColors,
    #[serde(default)]
    pub picker: PickerSettings,
    #[serde(default)]
    pub folder_search: FolderSearchSettings,
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
#[serde(deny_unknown_fields)]
pub struct IconColors {
    #[serde(default = "default_icon_color_session")]
    pub session: u8,
    #[serde(default = "default_icon_color_directory")]
    pub directory: u8,
    #[serde(default = "default_icon_color_template")]
    pub template: u8,
    #[serde(default = "default_icon_color_project")]
    pub project: u8,
}

fn default_icon_color_session() -> u8 {
    75
}

fn default_icon_color_directory() -> u8 {
    108
}

fn default_icon_color_template() -> u8 {
    179
}

fn default_icon_color_project() -> u8 {
    81
}

impl Default for IconColors {
    fn default() -> Self {
        Self {
            session: default_icon_color_session(),
            directory: default_icon_color_directory(),
            template: default_icon_color_template(),
            project: default_icon_color_project(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PickerSettings {
    #[serde(default)]
    pub bindings: PickerBindings,
    #[serde(default)]
    pub preview: PickerPreviewSettings,
    /// Whether the picker shows the keyboard-shortcut hint bar by default. It
    /// can always be toggled at runtime with `?`; this only sets the initial
    /// state.
    #[serde(default = "default_show_hints")]
    pub show_hints: bool,
}

fn default_show_hints() -> bool {
    true
}

impl Default for PickerSettings {
    fn default() -> Self {
        Self {
            bindings: PickerBindings::default(),
            preview: PickerPreviewSettings::default(),
            show_hints: default_show_hints(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PickerBindings {
    #[serde(default = "default_picker_reset")]
    pub reset: String,
    #[serde(default = "default_picker_sessions")]
    pub sessions: String,
    #[serde(default = "default_picker_folders")]
    pub folders: String,
    #[serde(default = "default_picker_projects")]
    pub projects: String,
    #[serde(default = "default_picker_delete_session")]
    pub delete_session: String,
    #[serde(default = "default_picker_save_project")]
    pub save_project: String,
    #[serde(default = "default_picker_rename_session")]
    pub rename_session: String,
    #[serde(default = "default_picker_edit_project")]
    pub edit_project: String,
    #[serde(default = "default_picker_toggle_hints")]
    pub toggle_hints: String,
}

#[derive(Debug, Clone, Deserialize, Default, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PickerPreviewSettings {
    pub folders: Option<String>,
    pub sessions: Option<String>,
    pub projects: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FolderSearchSettings {
    #[serde(default = "default_folder_search_roots")]
    pub roots: Vec<String>,
    #[serde(default = "default_folder_search_max_depth")]
    pub max_depth: usize,
    #[serde(default)]
    pub include_hidden: bool,
}

impl Default for FolderSearchSettings {
    fn default() -> Self {
        Self {
            roots: default_folder_search_roots(),
            max_depth: default_folder_search_max_depth(),
            include_hidden: false,
        }
    }
}

fn default_folder_search_roots() -> Vec<String> {
    vec!["~".to_owned()]
}

fn default_folder_search_max_depth() -> usize {
    3
}

impl Default for PickerBindings {
    fn default() -> Self {
        Self {
            reset: default_picker_reset(),
            sessions: default_picker_sessions(),
            folders: default_picker_folders(),
            projects: default_picker_projects(),
            delete_session: default_picker_delete_session(),
            save_project: default_picker_save_project(),
            rename_session: default_picker_rename_session(),
            edit_project: default_picker_edit_project(),
            toggle_hints: default_picker_toggle_hints(),
        }
    }
}

fn default_picker_reset() -> String {
    "ctrl-c".to_owned()
}

fn default_picker_sessions() -> String {
    "ctrl-s".to_owned()
}

fn default_picker_folders() -> String {
    "ctrl-f".to_owned()
}

fn default_picker_projects() -> String {
    "ctrl-p".to_owned()
}

fn default_picker_delete_session() -> String {
    "ctrl-x".to_owned()
}

fn default_picker_save_project() -> String {
    "alt-s".to_owned()
}

fn default_picker_rename_session() -> String {
    "ctrl-r".to_owned()
}

fn default_picker_edit_project() -> String {
    "ctrl-e".to_owned()
}

fn default_picker_toggle_hints() -> String {
    "?".to_owned()
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct Template {
    pub root: Option<String>,
    pub startup_window: Option<String>,
    pub startup_pane: Option<usize>,
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct Pane {
    pub layout: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub zoom: bool,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub config_exists: bool,
    pub project_dir: PathBuf,
    pub template_dir: PathBuf,
    pub config: Config,
    pub projects: HashMap<String, Project>,
    pub project_files: HashMap<String, PathBuf>,
    pub invalid_projects: Vec<InvalidProject>,
    pub template_files: HashMap<String, PathBuf>,
    pub invalid_templates: Vec<InvalidTemplate>,
}

#[derive(Debug, Clone)]
pub struct ResolvedProject<'a> {
    pub name: &'a str,
    pub project: &'a Project,
    pub normalized_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct InvalidProject {
    pub name: String,
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone)]
pub struct InvalidTemplate {
    pub name: String,
    pub path: PathBuf,
    pub error: String,
}

type LoadedProjects = (
    HashMap<String, Project>,
    HashMap<String, PathBuf>,
    Vec<InvalidProject>,
);

type LoadedTemplates = (
    HashMap<String, Template>,
    HashMap<String, PathBuf>,
    Vec<InvalidTemplate>,
);

pub fn starter_config() -> String {
    format!(
        "#:schema {}\n{}",
        schema_url("smux-config.schema.json"),
        STARTER_CONFIG_BODY
    )
}

pub fn starter_project() -> String {
    format!(
        "#:schema {}\n{}",
        schema_url("smux-project.schema.json"),
        STARTER_PROJECT_BODY
    )
}

pub fn starter_template(body: &str) -> String {
    format!(
        "#:schema {}\n{}",
        schema_url("smux-template.schema.json"),
        body
    )
}

pub fn schema_url(filename: &str) -> String {
    format!(
        "https://raw.githubusercontent.com/Aietes/smux/v{}/schemas/{filename}",
        env!("CARGO_PKG_VERSION")
    )
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

pub fn default_templates_dir() -> Result<PathBuf> {
    Ok(default_config_dir()?.join("templates"))
}

pub fn templates_dir_for_config_path(path: &Path) -> PathBuf {
    path.parent()
        .map(|parent| parent.join("templates"))
        .unwrap_or_else(|| PathBuf::from("templates"))
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
    let template_dir = templates_dir_for_config_path(&path);
    let config_exists = path.exists();

    let mut config = if config_exists {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config: Config = toml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        if !config.templates.is_empty() {
            bail!(
                "templates are no longer defined in config.toml; move each `[templates.<name>]` block into its own file at {}/<name>.toml",
                template_dir.display()
            );
        }
        config
    } else {
        Config::default()
    };

    let (templates, template_files, invalid_templates) = load_templates(&template_dir)?;
    config.templates = templates;

    validate_config(&config)?;

    let (projects, project_files, invalid_projects) = load_projects(&project_dir, &config)?;

    Ok(LoadedConfig {
        path,
        config_exists,
        project_dir,
        template_dir,
        config,
        projects,
        project_files,
        invalid_projects,
        template_files,
        invalid_templates,
    })
}

pub fn load_optional(path: Option<&Path>) -> Result<Option<LoadedConfig>> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };
    let project_dir = projects_dir_for_config_path(&path);
    let template_dir = templates_dir_for_config_path(&path);

    if !path.exists() && !project_dir.exists() && !template_dir.exists() {
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
    let template_dir = config_dir.join("templates");

    fs::create_dir_all(config_dir)
        .with_context(|| format!("failed to create config directory {}", config_dir.display()))?;
    fs::create_dir_all(&project_dir).with_context(|| {
        format!(
            "failed to create project directory {}",
            project_dir.display()
        )
    })?;
    fs::create_dir_all(&template_dir).with_context(|| {
        format!(
            "failed to create template directory {}",
            template_dir.display()
        )
    })?;

    fs::write(&path, starter_config())
        .with_context(|| format!("failed to write starter config to {}", path.display()))?;

    let starter_project_path = project_dir.join("example.toml");
    fs::write(&starter_project_path, starter_project()).with_context(|| {
        format!(
            "failed to write starter project to {}",
            starter_project_path.display()
        )
    })?;

    for (name, body) in [
        ("default", STARTER_TEMPLATE_DEFAULT_BODY),
        ("rust", STARTER_TEMPLATE_RUST_BODY),
    ] {
        let template_path = template_dir.join(format!("{name}.toml"));
        fs::write(&template_path, starter_template(body)).with_context(|| {
            format!(
                "failed to write starter template to {}",
                template_path.display()
            )
        })?;
    }

    Ok(path)
}

pub fn validate_config(config: &Config) -> Result<()> {
    validate_picker_bindings(&config.settings.picker.bindings)?;
    validate_folder_search(&config.settings.folder_search)?;

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

fn validate_folder_search(settings: &FolderSearchSettings) -> Result<()> {
    if settings.max_depth > MAX_FOLDER_SEARCH_DEPTH {
        bail!(
            "folder_search.max_depth must be at most {}",
            MAX_FOLDER_SEARCH_DEPTH
        );
    }

    for root in &settings.roots {
        if root.trim().is_empty() {
            bail!("folder_search.roots must not contain empty paths");
        }
    }

    Ok(())
}

fn validate_picker_bindings(bindings: &PickerBindings) -> Result<()> {
    let values = [
        ("reset", bindings.reset.trim()),
        ("sessions", bindings.sessions.trim()),
        ("folders", bindings.folders.trim()),
        ("projects", bindings.projects.trim()),
        ("delete_session", bindings.delete_session.trim()),
        ("save_project", bindings.save_project.trim()),
        ("rename_session", bindings.rename_session.trim()),
        ("edit_project", bindings.edit_project.trim()),
        ("toggle_hints", bindings.toggle_hints.trim()),
    ];

    for (name, value) in values {
        if value.is_empty() {
            bail!("picker binding \"{name}\" must not be empty");
        }
    }

    let mut seen = std::collections::HashSet::new();
    for (name, value) in values {
        if !seen.insert(value) {
            bail!("picker binding \"{name}\" duplicates another picker binding");
        }
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

    validate_startup_pane(name, template)?;

    for window in &template.windows {
        validate_window(name, window)?;
    }

    Ok(())
}

fn validate_startup_pane(owner_name: &str, template: &Template) -> Result<()> {
    let startup_pane = template.startup_pane.unwrap_or(0);
    let startup_window = template
        .startup_window
        .as_deref()
        .unwrap_or(&template.windows[0].name);
    let window = template
        .windows
        .iter()
        .find(|window| window.name == startup_window)
        .context("startup window validation ran before startup window existence validation")?;
    let pane_count = window.panes.as_ref().map(Vec::len).unwrap_or(1);

    if startup_pane >= pane_count {
        bail!(
            "{owner_name} startup_pane {} is out of range for window \"{}\" with {} pane(s)",
            startup_pane,
            startup_window,
            pane_count
        );
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

    if let Some(panes) = &window.panes {
        for (index, pane) in panes.iter().enumerate() {
            if index > 0 && pane.layout.is_none() {
                bail!(
                    "{owner_name} pane {} in window \"{}\" is missing a layout",
                    index,
                    window.name
                );
            }

            if let Some(layout) = &pane.layout {
                crate::templates::validate_pane_layout(layout).with_context(|| {
                    format!(
                        "{owner_name} pane {} in window \"{}\" has an invalid layout",
                        index, window.name
                    )
                })?;
            }
        }

        let zoomed = panes.iter().filter(|pane| pane.zoom).count();
        if zoomed > 1 {
            bail!(
                "{owner_name} window \"{}\" may define at most one zoomed pane",
                window.name
            );
        }
    }

    Ok(())
}

fn load_projects(project_dir: &Path, config: &Config) -> Result<LoadedProjects> {
    if !project_dir.exists() {
        return Ok((HashMap::new(), HashMap::new(), Vec::new()));
    }

    let mut files = fs::read_dir(project_dir)
        .with_context(|| format!("failed to read project directory {}", project_dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to read project directory {}", project_dir.display()))?;
    files.sort_by_key(|entry| entry.file_name());

    let mut projects = HashMap::new();
    let mut project_files = HashMap::new();
    let mut invalid_projects = Vec::new();

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

        match load_project_file(&path, &name, config) {
            Ok(project) => {
                project_files.insert(name.clone(), path.clone());
                projects.insert(name, project);
            }
            Err(error) => invalid_projects.push(InvalidProject {
                name,
                path: path.clone(),
                error: error.to_string(),
            }),
        }
    }

    Ok((projects, project_files, invalid_projects))
}

fn load_project_file(path: &Path, name: &str, config: &Config) -> Result<Project> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read project {}", path.display()))?;
    let project: Project = toml::from_str(&text)
        .with_context(|| format!("failed to parse project {}", path.display()))?;
    validate_project(name, &project, config)?;
    Ok(project)
}

fn load_templates(template_dir: &Path) -> Result<LoadedTemplates> {
    if !template_dir.exists() {
        return Ok((HashMap::new(), HashMap::new(), Vec::new()));
    }

    let mut files = fs::read_dir(template_dir)
        .with_context(|| {
            format!(
                "failed to read template directory {}",
                template_dir.display()
            )
        })?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| {
            format!(
                "failed to read template directory {}",
                template_dir.display()
            )
        })?;
    files.sort_by_key(|entry| entry.file_name());

    let mut templates = HashMap::new();
    let mut template_files = HashMap::new();
    let mut invalid_templates = Vec::new();

    for entry in files {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .context("template file name was not valid utf-8")?
            .to_owned();

        match load_template_file(&path, &name) {
            Ok(template) => {
                template_files.insert(name.clone(), path.clone());
                templates.insert(name, template);
            }
            Err(error) => invalid_templates.push(InvalidTemplate {
                name,
                path: path.clone(),
                error: error.to_string(),
            }),
        }
    }

    Ok((templates, template_files, invalid_templates))
}

fn load_template_file(path: &Path, name: &str) -> Result<Template> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read template {}", path.display()))?;
    let template: Template = toml::from_str(&text)
        .with_context(|| format!("failed to parse template {}", path.display()))?;
    validate_template(name, &template)?;
    Ok(template)
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
    // Normalize the query path the same way as each project path so the
    // comparison is symmetric (both canonicalize when the directory exists and
    // fall back to a lexical absolute path otherwise); using `normalize_path`
    // here would canonicalize only one side and error on not-yet-created dirs.
    let normalized = util::expand_and_absolutize_path(path)?;

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

/// Resolve the on-disk path of a project file by name, covering both valid and
/// invalid (broken) projects, and verifying the file lives inside the configured
/// project directory.
pub fn project_file_path(loaded: &LoadedConfig, project_name: &str) -> Result<PathBuf> {
    let project_name = util::validated_project_name(project_name)?;
    let path = loaded
        .project_files
        .get(&project_name)
        .cloned()
        .or_else(|| {
            loaded
                .invalid_projects
                .iter()
                .find(|project| project.name == project_name)
                .map(|project| project.path.clone())
        })
        .with_context(|| format!("project file not found for {project_name}"))?;
    ensure_project_file_is_in_project_dir(&loaded.project_dir, &path)?;
    Ok(path)
}

pub fn delete_project_file(loaded: &LoadedConfig, project_name: &str) -> Result<PathBuf> {
    let path = project_file_path(loaded, project_name)?;
    fs::remove_file(&path)
        .with_context(|| format!("failed to delete project file {}", path.display()))?;
    Ok(path)
}

fn ensure_project_file_is_in_project_dir(project_dir: &Path, path: &Path) -> Result<()> {
    let project_dir = project_dir.canonicalize().with_context(|| {
        format!(
            "failed to resolve project directory {}",
            project_dir.display()
        )
    })?;
    let parent = path
        .parent()
        .with_context(|| format!("project file {} did not have a parent", path.display()))?
        .canonicalize()
        .with_context(|| format!("failed to resolve project file parent {}", path.display()))?;

    if parent != project_dir {
        bail!(
            "refusing to delete project file outside project directory: {}",
            path.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Config, IconColors, IconMode, PickerBindings, STARTER_TEMPLATE_DEFAULT_BODY,
        STARTER_TEMPLATE_RUST_BODY, Template, default_projects_dir, load, load_optional,
        load_workspace, materialize_project_template, resolve_project, schema_url, starter_config,
        starter_project, starter_template, validate_config, validate_template,
    };
    use anyhow::Result;
    use std::fs;
    use std::path::Path;

    fn strip_schema_directive(text: &str) -> String {
        text.lines().skip(1).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn parses_starter_config() -> Result<()> {
        let starter = starter_config();
        assert!(starter.starts_with("#:schema "));
        let config: Config = toml::from_str(&strip_schema_directive(&starter))?;
        // Templates now live in their own files, so the starter config carries none.
        assert!(config.templates.is_empty());
        assert_eq!(config.settings.default_template.as_deref(), Some("default"));
        assert_eq!(config.settings.icons, IconMode::Auto);
        assert_eq!(config.settings.icon_colors, IconColors::default());
        assert_eq!(config.settings.picker.bindings, PickerBindings::default());
        assert_eq!(
            config.settings.folder_search,
            super::FolderSearchSettings::default()
        );
        Ok(())
    }

    #[test]
    fn parses_starter_project() -> Result<()> {
        let starter = starter_project();
        assert!(starter.starts_with("#:schema "));
        let project: super::Project = toml::from_str(&strip_schema_directive(&starter))?;
        assert_eq!(project.session_name.as_deref(), Some("example"));
        assert_eq!(project.template.as_deref(), Some("rust"));
        Ok(())
    }

    #[test]
    fn parses_starter_templates() -> Result<()> {
        for (name, body, windows) in [
            ("default", STARTER_TEMPLATE_DEFAULT_BODY, 1),
            ("rust", STARTER_TEMPLATE_RUST_BODY, 2),
        ] {
            let starter = starter_template(body);
            assert!(starter.starts_with("#:schema "));
            let template: Template = toml::from_str(&strip_schema_directive(&starter))?;
            validate_template(name, &template)?;
            assert_eq!(template.windows.len(), windows);
        }
        Ok(())
    }

    #[test]
    fn schema_urls_are_versioned() {
        let version = env!("CARGO_PKG_VERSION");
        assert!(schema_url("smux-config.schema.json").contains(&format!("/v{version}/")));
        assert!(schema_url("smux-project.schema.json").contains(&format!("/v{version}/")));
        assert!(schema_url("smux-template.schema.json").contains(&format!("/v{version}/")));
    }

    #[test]
    fn parses_custom_picker_bindings() -> Result<()> {
        let input = r#"
[settings.picker.bindings]
reset = "alt-a"
sessions = "alt-s"
folders = "alt-f"
projects = "alt-p"
delete_session = "alt-x"
save_project = "alt-y"
"#;

        let config: Config = toml::from_str(input)?;
        validate_config(&config)?;
        assert_eq!(config.settings.picker.bindings.reset, "alt-a");
        assert_eq!(config.settings.picker.bindings.delete_session, "alt-x");
        assert_eq!(config.settings.picker.bindings.save_project, "alt-y");
        Ok(())
    }

    #[test]
    fn picker_hint_settings_default_when_absent() -> Result<()> {
        let config: Config = toml::from_str("[settings]\n")?;
        assert!(config.settings.picker.show_hints);
        assert_eq!(config.settings.picker.bindings.toggle_hints, "?");
        Ok(())
    }

    #[test]
    fn parses_custom_picker_hint_settings() -> Result<()> {
        let input = r#"
[settings.picker]
show_hints = false

[settings.picker.bindings]
toggle_hints = "f1"
"#;

        let config: Config = toml::from_str(input)?;
        validate_config(&config)?;
        assert!(!config.settings.picker.show_hints);
        assert_eq!(config.settings.picker.bindings.toggle_hints, "f1");
        Ok(())
    }

    #[test]
    fn rejects_toggle_hints_colliding_with_another_binding() {
        let input = r#"
[settings.picker.bindings]
toggle_hints = "ctrl-s"
"#;

        let config: Config = toml::from_str(input).expect("config should parse");
        let error = validate_config(&config).expect_err("colliding bindings should fail");
        assert!(
            error
                .to_string()
                .contains("duplicates another picker binding")
        );
    }

    #[test]
    fn parses_custom_picker_preview_commands() -> Result<()> {
        let input = r#"
[settings.picker.preview]
sessions = "tmux capture-pane -p -t \"$SMUX_PREVIEW_SESSION\""
folders = "eza --tree \"$SMUX_PREVIEW_PATH\""
projects = "bat --style=plain \"$SMUX_PREVIEW_FILE\""
"#;

        let config: Config = toml::from_str(input)?;
        assert_eq!(
            config.settings.picker.preview.sessions.as_deref(),
            Some("tmux capture-pane -p -t \"$SMUX_PREVIEW_SESSION\"")
        );
        assert_eq!(
            config.settings.picker.preview.folders.as_deref(),
            Some("eza --tree \"$SMUX_PREVIEW_PATH\"")
        );
        assert_eq!(
            config.settings.picker.preview.projects.as_deref(),
            Some("bat --style=plain \"$SMUX_PREVIEW_FILE\"")
        );
        Ok(())
    }

    #[test]
    fn rejects_duplicate_picker_bindings() {
        let input = r#"
[settings.picker.bindings]
reset = "ctrl-c"
sessions = "ctrl-s"
folders = "ctrl-f"
projects = "ctrl-s"
delete_session = "ctrl-x"
save_project = "alt-s"
"#;

        let config: Config = toml::from_str(input).expect("config should parse");
        let error = validate_config(&config).expect_err("duplicate picker bindings should fail");
        assert!(
            error
                .to_string()
                .contains("duplicates another picker binding")
        );
    }

    #[test]
    fn defaults_folder_search_to_home_root() -> Result<()> {
        let config: Config = toml::from_str("[settings]\n")?;
        assert_eq!(config.settings.folder_search.roots, vec!["~"]);
        assert_eq!(config.settings.folder_search.max_depth, 3);
        assert!(!config.settings.folder_search.include_hidden);
        Ok(())
    }

    #[test]
    fn parses_custom_folder_search_settings() -> Result<()> {
        let input = r#"
[settings.folder_search]
roots = ["~/Development", "~/code"]
max_depth = 5
include_hidden = true
"#;

        let config: Config = toml::from_str(input)?;
        validate_config(&config)?;
        assert_eq!(
            config.settings.folder_search.roots,
            vec!["~/Development", "~/code"]
        );
        assert_eq!(config.settings.folder_search.max_depth, 5);
        assert!(config.settings.folder_search.include_hidden);
        Ok(())
    }

    #[test]
    fn rejects_empty_folder_search_roots() {
        let input = r#"
[settings.folder_search]
roots = [""]
"#;

        let config: Config = toml::from_str(input).expect("config should parse");
        let error = validate_config(&config).expect_err("validation should fail");
        assert!(error.to_string().contains("must not contain empty paths"));
    }

    #[test]
    fn rejects_unbounded_folder_search_depth() {
        let input = r#"
[settings.folder_search]
max_depth = 17
"#;

        let config: Config = toml::from_str(input).expect("config should parse");
        let error = validate_config(&config).expect_err("validation should fail");
        assert!(error.to_string().contains("max_depth"));
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
    fn rejects_unknown_project_fields() {
        let error = toml::from_str::<super::Project>(
            "path = \"/tmp/demo\"\nwindows = [{ name = \"main\", panes = [{ cmd = \"nvim\" }] }]\n",
        )
        .expect_err("unknown fields should fail");

        assert!(error.to_string().contains("unknown field"));
        assert!(error.to_string().contains("cmd"));
    }

    #[test]
    fn rejects_multiple_zoomed_panes_in_window() {
        let config: Config = toml::from_str(
            r#"
[templates.default]
windows = [
  { name = "main", panes = [
      { command = "nvim", zoom = true },
      { layout = "right", command = "cargo test", zoom = true },
    ] },
]
"#,
        )
        .expect("config should parse");

        let error = validate_config(&config).expect_err("validation should fail");
        assert!(error.to_string().contains("zoomed pane"));
    }

    #[test]
    fn rejects_startup_pane_out_of_range_during_config_validation() {
        let config: Config = toml::from_str(
            r#"
[templates.default]
startup_window = "main"
startup_pane = 2
windows = [
  { name = "main", panes = [
      { command = "nvim" },
      { layout = "right", command = "cargo test" },
    ] },
]
"#,
        )
        .expect("config should parse");

        let error = validate_config(&config).expect_err("validation should fail");
        assert!(error.to_string().contains("startup_pane"));
        assert!(error.to_string().contains("out of range"));
    }

    #[test]
    fn rejects_invalid_pane_layout_during_config_validation() {
        let config: Config = toml::from_str(
            r#"
[templates.default]
windows = [
  { name = "main", panes = [
      { command = "nvim" },
      { layout = "diagonal 40%", command = "cargo test" },
    ] },
]
"#,
        )
        .expect("config should parse");

        let error = validate_config(&config).expect_err("validation should fail");
        assert!(error.to_string().contains("invalid layout"));
    }

    #[test]
    fn rejects_missing_layout_for_additional_panes() {
        let config: Config = toml::from_str(
            r#"
[templates.default]
windows = [
  { name = "main", panes = [
      { command = "nvim" },
      { command = "cargo test" },
    ] },
]
"#,
        )
        .expect("config should parse");

        let error = validate_config(&config).expect_err("validation should fail");
        assert!(error.to_string().contains("missing a layout"));
    }

    #[test]
    fn resolves_project_by_normalized_path() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let config_path = tempdir.path().join("config.toml");
        let project_dir = tempdir.path().join("projects");
        let workspace_dir = tempdir.path().join("demo");
        fs::create_dir(&workspace_dir)?;
        fs::create_dir(&project_dir)?;

        let template_dir = tempdir.path().join("templates");
        fs::create_dir(&template_dir)?;
        fs::write(
            template_dir.join("default.toml"),
            "windows = [{ name = \"main\" }]\n",
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
    fn deletes_project_file_by_name() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let config_path = tempdir.path().join("config.toml");
        let project_dir = tempdir.path().join("projects");
        fs::create_dir(&project_dir)?;
        let project_path = project_dir.join("demo.toml");
        fs::write(&project_path, "path = \"/tmp/demo\"\n")?;

        let loaded = load_workspace(Some(&config_path))?;
        let deleted = super::delete_project_file(&loaded, "demo")?;

        assert_eq!(deleted, project_path);
        assert!(!deleted.exists());
        Ok(())
    }

    #[test]
    fn deletes_invalid_project_file_by_name() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let config_path = tempdir.path().join("config.toml");
        let project_dir = tempdir.path().join("projects");
        fs::create_dir(&project_dir)?;
        let project_path = project_dir.join("broken.toml");
        fs::write(&project_path, "not = [valid\n")?;

        let loaded = load_workspace(Some(&config_path))?;
        assert_eq!(loaded.invalid_projects.len(), 1);
        let deleted = super::delete_project_file(&loaded, "broken")?;

        assert_eq!(deleted, project_path);
        assert!(!deleted.exists());
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
        let template_dir = tempdir.path().join("templates");
        fs::create_dir(&project_dir)?;
        fs::create_dir(&template_dir)?;
        fs::write(&path, starter_config())?;
        fs::write(
            template_dir.join("default.toml"),
            starter_template(STARTER_TEMPLATE_DEFAULT_BODY),
        )?;
        fs::write(
            template_dir.join("rust.toml"),
            starter_template(STARTER_TEMPLATE_RUST_BODY),
        )?;
        fs::write(project_dir.join("example.toml"), starter_project())?;

        let loaded = load(Some(&path))?;
        assert_eq!(loaded.path, path);
        assert!(loaded.projects.contains_key("example"));
        assert!(loaded.config.templates.contains_key("default"));
        assert!(loaded.config.templates.contains_key("rust"));
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
