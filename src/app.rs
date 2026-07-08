use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cli::{Cli, Commands};
use crate::config;
use crate::docs;
use crate::doctor;
use crate::folder_search;
use crate::fzf;
use crate::github;
use crate::project_export;
use crate::session;
use crate::skill;
use crate::tmux::Tmux;
use crate::ui::DisplayStyle;
use crate::util;
use crate::zoxide;

const BUILTIN_TEMPLATE_LABEL: &str = "<builtin>";

pub fn run(cli: Cli) -> Result<()> {
    let tmux = Tmux::new();
    let config = cli.config;

    match cli.command {
        Commands::Select {
            choose_template,
            no_project_detect,
        } => {
            let loaded = config::load_optional(config.as_deref())?;
            run_select(
                &tmux,
                loaded,
                config.as_deref(),
                choose_template,
                no_project_detect,
            )
        }
        Commands::Connect {
            path,
            template,
            session_name,
        } => {
            let loaded = config::load_optional(config.as_deref())?;
            session::connect_path(
                &tmux,
                &path,
                loaded.as_ref(),
                template.as_deref(),
                session_name.as_deref(),
                session::ProjectDetection::Enabled,
            )
        }
        Commands::Switch { session } => session::switch_existing(&tmux, &session),
        Commands::Last => session::switch_last(&tmux),
        Commands::Kill { session } => {
            let killed = session::kill_target(&tmux, session.as_deref())?;
            println!("killed {killed}");
            Ok(())
        }
        Commands::Clone {
            url,
            dir,
            template,
            no_connect,
        } => {
            let loaded = config::load_optional(config.as_deref())?;
            let clone_settings = loaded
                .as_ref()
                .map(|loaded| loaded.config.settings.clone_settings.clone())
                .unwrap_or_default();
            let display_style =
                DisplayStyle::from_config(loaded.as_ref().map(|loaded| &loaded.config));

            let Some(target) =
                clone_repository(url.as_deref(), dir, &clone_settings, display_style)?
            else {
                // The repo browser was cancelled; nothing to do.
                return Ok(());
            };

            if no_connect {
                println!("{}", target.display());
                return Ok(());
            }
            session::connect_path(
                &tmux,
                &target,
                loaded.as_ref(),
                template.as_deref(),
                None,
                session::ProjectDetection::Enabled,
            )
        }
        Commands::Prune => {
            let pruned = session::prune_detached(&tmux)?;
            if pruned.is_empty() {
                eprintln!("no detached sessions to prune");
            } else {
                for session in &pruned {
                    println!("killed {session}");
                }
            }
            Ok(())
        }
        Commands::ListSessions { json } => {
            let sessions = tmux.list_sessions()?;
            if json {
                println!("{}", json_name_array(&sessions));
            } else {
                for session in sessions {
                    println!("{session}");
                }
            }

            Ok(())
        }
        Commands::Doctor { fix } => doctor::run(config.as_deref(), fix),
        Commands::SaveProject {
            name,
            session,
            path,
            stdout,
            force,
        } => {
            if let Some(path) = project_export::save_project(
                &tmux,
                name.as_deref(),
                session.as_deref(),
                path.as_deref(),
                stdout,
                force,
                config.as_deref(),
            )? {
                println!("{}", path.display());
            }
            Ok(())
        }
        Commands::ListTemplates { json } => {
            let loaded = config::load(config.as_deref())?;
            let mut names = loaded.config.templates.keys().cloned().collect::<Vec<_>>();
            names.sort();
            if json {
                println!("{}", json_name_array(&names));
            } else {
                for name in names {
                    println!("{name}");
                }
            }
            Ok(())
        }
        Commands::ListProjects { json } => {
            let loaded = config::load_workspace(config.as_deref())?;
            let mut names = loaded.projects.keys().cloned().collect::<Vec<_>>();
            names.sort();
            let mut lines = Vec::with_capacity(names.len());
            for name in names {
                let project = &loaded.projects[&name];
                // Use the graceful resolver: a project whose directory doesn't
                // exist yet is still listed (its absolute path shown) rather than
                // aborting the whole command — consistent with how `doctor` and
                // project validation treat missing paths.
                let resolved = util::expand_and_absolutize_path(Path::new(&project.path))?;
                if json {
                    lines.push(format!(
                        "{{\"name\":{},\"path\":{}}}",
                        util::json_string(&name),
                        util::json_string(&resolved.display().to_string())
                    ));
                } else {
                    println!("{name}\t{}", resolved.display());
                }
            }
            if json {
                println!("[{}]", lines.join(","));
            }
            Ok(())
        }
        Commands::Detect { path, quiet } => {
            // Fail on a missing or non-directory path (as `connect` does) so a
            // typo doesn't read as "no template matched".
            let path = util::normalize_path(&path)?;
            if !path.is_dir() {
                bail!("not a directory: {}", path.display());
            }
            let loaded = config::load_workspace(config.as_deref())?;
            let matches = session::detect_matches(&loaded.config, &path);
            if quiet {
                // Script-friendly: just the winning template name, exit 1 on
                // no match, no prose on either path.
                return match matches.first() {
                    Some(matched) => {
                        println!("{}", matched.name);
                        Ok(())
                    }
                    None => std::process::exit(1),
                };
            }
            if matches.is_empty() {
                println!("no template auto-detects {}", path.display());
                println!(
                    "smux would use the built-in fallback (or prompt if two or more templates are defined)"
                );
                return Ok(());
            }

            let width = matches.iter().map(|m| m.name.len()).max().unwrap_or(0);
            for (index, matched) in matches.iter().enumerate() {
                let mut reasons: Vec<String> = Vec::new();
                if !matched.matched_files.is_empty() {
                    reasons.push(matched.matched_files.join(", "));
                }
                for dependency in &matched.matched_dependencies {
                    reasons.push(format!("dependency \"{dependency}\""));
                }
                let arrow = if index == 0 { "→" } else { " " };
                println!(
                    "{arrow} {:<width$}  priority {}  {}",
                    matched.name,
                    matched.priority,
                    reasons.join(", "),
                );
            }
            Ok(())
        }
        Commands::Init => {
            let path = config::init(config.as_deref())?;
            println!("{}", path.display());
            Ok(())
        }
        Commands::Completions { shell, dir } => {
            if let Some(path) = docs::generate_completions(shell, dir.as_deref())? {
                println!("{}", path.display());
            }
            Ok(())
        }
        Commands::Man { dir } => {
            if let Some(paths) = docs::generate_man_pages(dir.as_deref())? {
                for path in paths {
                    println!("{}", path.display());
                }
            }
            Ok(())
        }
        Commands::Skill { dir } => {
            if let Some(path) = skill::write_skill(dir.as_deref())? {
                println!("{}", path.display());
            }
            Ok(())
        }
    }
}

fn run_select(
    tmux: &Tmux,
    mut loaded: Option<config::LoadedConfig>,
    config_path: Option<&Path>,
    choose_template: bool,
    no_project_detect: bool,
) -> Result<()> {
    require_interactive_terminal()?;

    let project_detection = if no_project_detect {
        session::ProjectDetection::Disabled
    } else {
        session::ProjectDetection::Enabled
    };

    let initial_show_hints = loaded
        .as_ref()
        .map(|loaded| loaded.config.settings.picker.show_hints)
        .unwrap_or(true);
    // The picker re-launches after in-loop actions; this file lets the runtime
    // hint toggle persist across those relaunches within one `smux select`.
    let hint_state = fzf::HintState::new(initial_show_hints)?;

    // Scanning zoxide and the folder-search roots is the expensive part of
    // building the entry list, and no in-picker action changes what's on
    // disk — scan once and rebuild only sessions and projects per relaunch.
    let directories = scan_directories(loaded.as_ref());

    loop {
        let config = loaded.as_ref().map(|loaded| &loaded.config);
        let display_style = DisplayStyle::from_config(config);
        let picker_bindings = config
            .map(|config| config.settings.picker.bindings.clone())
            .unwrap_or_default();
        let picker_preview = config
            .map(|config| config.settings.picker.preview.clone())
            .unwrap_or_default();
        let current_session = tmux.current_session().ok().flatten();
        let current_window = tmux.current_window_id().ok().flatten();
        let entries = select_entries(
            tmux,
            loaded.as_ref(),
            display_style,
            current_session.as_deref(),
            &directories,
        )?;

        let Some(selection) = fzf::select(entries, &picker_bindings, &picker_preview, &hint_state)?
        else {
            return Ok(());
        };

        match (selection.action, selection.entry.kind) {
            (fzf::SelectAction::Open, fzf::EntryKind::Session) => {
                return session::switch_existing(tmux, &selection.entry.value);
            }
            (fzf::SelectAction::Delete, fzf::EntryKind::Session) => {
                if current_session.as_deref() == Some(selection.entry.value.as_str()) {
                    eprintln!("cannot close the current session from the picker");
                    continue;
                }
                // Downgrade to a warning so a failed delete (like the other
                // in-loop actions) keeps the picker alive.
                if let Err(error) = session::kill_existing(tmux, &selection.entry.value) {
                    eprintln!("warning: {error:#}");
                }
            }
            (fzf::SelectAction::Rename, fzf::EntryKind::Session) => {
                if let Some(new_name) = rename_session_from_picker(tmux, &selection.entry.value)? {
                    eprintln!("renamed session to {new_name}");
                }
            }
            (fzf::SelectAction::Open, fzf::EntryKind::Window) => {
                return session::switch_to_window(tmux, &selection.entry.value);
            }
            (fzf::SelectAction::Delete, fzf::EntryKind::Window) => {
                if current_window.as_deref() == Some(selection.entry.value.as_str()) {
                    eprintln!("cannot close the current window from the picker");
                    continue;
                }
                if let Err(error) = session::kill_window(tmux, &selection.entry.value) {
                    eprintln!("warning: {error:#}");
                }
            }
            (fzf::SelectAction::Rename, fzf::EntryKind::Window) => {
                if let Some(new_name) = rename_window_from_picker(
                    tmux,
                    &selection.entry.label,
                    &selection.entry.value,
                )? {
                    eprintln!("renamed window to {new_name}");
                }
            }
            (fzf::SelectAction::Delete, fzf::EntryKind::Project)
            | (fzf::SelectAction::Delete, fzf::EntryKind::InvalidProject) => {
                match delete_project_from_picker(loaded.as_ref(), &selection.entry.value) {
                    Ok(path) => {
                        eprintln!("deleted project {}", path.display());
                        loaded = config::load_optional(config_path)?;
                    }
                    Err(error) => eprintln!("warning: {error:#}"),
                }
            }
            (fzf::SelectAction::SaveProject, fzf::EntryKind::Session) => {
                let existed = project_export::project_exists(&selection.entry.value, config_path)
                    .unwrap_or(false);
                match save_project_from_picker(tmux, &selection.entry.value, config_path) {
                    Ok(Some(path)) => {
                        let verb = if existed { "updated" } else { "saved" };
                        eprintln!("{verb} project {}", path.display());
                        loaded = config::load_optional(config_path)?;
                    }
                    Ok(None) => {}
                    Err(error) => eprintln!("warning: {error:#}"),
                }
            }
            (fzf::SelectAction::Open, fzf::EntryKind::Directory)
            | (fzf::SelectAction::ChooseTemplate, fzf::EntryKind::Directory) => {
                let path = Path::new(&selection.entry.value);
                // Offer the template picker when explicitly requested — via the
                // choose-template key on this folder or the session-wide
                // `--choose-template` flag — or when no template would resolve on
                // its own but several are available.
                let force_choice = matches!(selection.action, fzf::SelectAction::ChooseTemplate);
                let offer_choice = force_choice
                    || choose_template
                    || session::should_offer_template_choice(loaded.as_ref(), path);
                let template = if offer_choice {
                    let Some(template) = choose_template_name(config, display_style)? else {
                        return Ok(());
                    };
                    Some(template)
                } else {
                    None
                };

                return session::connect_path(
                    tmux,
                    path,
                    loaded.as_ref(),
                    template.as_deref(),
                    None,
                    project_detection,
                );
            }
            (fzf::SelectAction::Open, fzf::EntryKind::Project) => {
                let loaded = loaded
                    .as_ref()
                    .context("project selection requires config or project files")?;
                return session::connect_project(tmux, loaded, &selection.entry.value);
            }
            (fzf::SelectAction::Open, fzf::EntryKind::InvalidProject) => continue,
            (fzf::SelectAction::Edit, fzf::EntryKind::Project)
            | (fzf::SelectAction::Edit, fzf::EntryKind::InvalidProject) => {
                if let Err(error) =
                    edit_project_from_picker(loaded.as_ref(), &selection.entry.value)
                {
                    eprintln!("warning: {error:#}");
                }
                // The file may have changed (or a broken project become valid),
                // so reload before the picker relaunches.
                loaded = config::load_optional(config_path)?;
            }
            (fzf::SelectAction::Delete, _) => continue,
            (fzf::SelectAction::SaveProject, _) => continue,
            (fzf::SelectAction::Rename, _) => continue,
            (fzf::SelectAction::Edit, _) => continue,
            // Choosing a template only applies to folders; ignore it elsewhere.
            (fzf::SelectAction::ChooseTemplate, _) => continue,
        }
    }
}

/// Resolve (and if necessary create) the checkout for `smux clone`. With a
/// URL, `git clone` it; without one, open the GitHub repo browser and
/// `gh repo clone` the selection. Returns `None` when browsing is cancelled.
fn clone_repository(
    url: Option<&str>,
    dir: Option<PathBuf>,
    settings: &config::CloneSettings,
    display_style: DisplayStyle,
) -> Result<Option<PathBuf>> {
    match url {
        Some(url) => {
            let target = match dir {
                Some(dir) => dir,
                None => clone_destination(settings, &util::repo_directory_from_url(url)?),
            };
            if target.exists() {
                eprintln!("{} already exists, connecting", target.display());
                return Ok(Some(target));
            }
            // `--` keeps a URL or directory starting with `-` from being
            // parsed as a git flag (e.g. `--upload-pack=<cmd>` executes
            // arbitrary commands).
            run_clone_tool(
                "git",
                vec![
                    "clone".to_owned(),
                    "--".to_owned(),
                    url.to_owned(),
                    util::path_to_string(&target)?,
                ],
            )?;
            Ok(Some(target))
        }
        None => {
            // The browser drives fzf, which needs a terminal.
            require_interactive_terminal()?;

            let runner = crate::process::default_runner();
            let repos = github::list_repos(&runner, &settings.owners)?;
            if repos.is_empty() {
                bail!("gh returned no repositories to browse");
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or(0);
            let choices = repos
                .iter()
                .map(|repo| {
                    fzf::Choice::new(
                        "repo",
                        github::repo_label(display_style, repo, now),
                        repo.name_with_owner.clone(),
                    )
                })
                .collect();

            let Some(selection) = fzf::select_value("clone> ", choices)? else {
                return Ok(None);
            };
            let name = selection.rsplit('/').next().unwrap_or(&selection).to_owned();
            let target = match dir {
                Some(dir) => dir,
                None => clone_destination(settings, &name),
            };
            if target.exists() {
                eprintln!("{} already exists, connecting", target.display());
                return Ok(Some(target));
            }
            // gh resolved the name from GitHub's own listing, so it cannot
            // start with `-` (and `--` here would mean "flags for git").
            run_clone_tool(
                "gh",
                vec![
                    "repo".to_owned(),
                    "clone".to_owned(),
                    selection,
                    util::path_to_string(&target)?,
                ],
            )?;
            Ok(Some(target))
        }
    }
}

/// Where a clone lands when no explicit directory is given: under
/// `[settings.clone] root` if set, else the current directory.
fn clone_destination(settings: &config::CloneSettings, name: &str) -> PathBuf {
    match settings.root.as_deref() {
        Some(root) => util::expand_tilde_path(Path::new(root)).join(name),
        None => PathBuf::from(name),
    }
}

fn run_clone_tool(program: &str, args: Vec<String>) -> Result<()> {
    // Inherited stdio so progress and auth prompts reach the terminal.
    let status = crate::process::default_runner()
        .run_inherit(program, &args)
        .with_context(|| format!("failed to execute {program} clone"))?;
    if !status.success {
        bail!(
            "{program} clone failed with {}",
            util::exit_status_label(status.code)
        );
    }
    Ok(())
}

fn json_name_array(names: &[String]) -> String {
    let items = names
        .iter()
        .map(|name| util::json_string(name))
        .collect::<Vec<_>>();
    format!("[{}]", items.join(","))
}

/// The picker drives fzf, which needs a terminal to draw on; without one
/// (cron, CI, a stray script) it would block forever waiting for input.
/// Opening `/dev/tty` is the same probe fzf uses for its interactive UI.
fn require_interactive_terminal() -> Result<()> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map(|_| ())
        .map_err(|_| anyhow::anyhow!("smux select requires an interactive terminal"))
}

fn rename_window_from_picker(
    tmux: &Tmux,
    label: &str,
    window_id: &str,
) -> Result<Option<String>> {
    // Prompt with the human-readable "session: window" label, not the @id.
    let Some(input) = prompt_line(&format!("rename window \"{label}\" to: "))? else {
        return Ok(None);
    };
    match session::rename_window(tmux, window_id, &input) {
        Ok(new_name) => Ok(Some(new_name)),
        Err(error) => {
            eprintln!("warning: {error:#}");
            Ok(None)
        }
    }
}

fn rename_session_from_picker(tmux: &Tmux, session_name: &str) -> Result<Option<String>> {
    let Some(input) = prompt_line(&format!("rename \"{session_name}\" to: "))? else {
        return Ok(None);
    };
    match session::rename_existing(tmux, session_name, &input) {
        Ok(new_name) => Ok(Some(new_name)),
        Err(error) => {
            eprintln!("warning: {error:#}");
            Ok(None)
        }
    }
}

/// Prompt on the controlling terminal and read a single trimmed line. Returns
/// `None` when the user submits an empty line or input reaches EOF.
fn prompt_line(prompt: &str) -> Result<Option<String>> {
    use std::io::{Write, stderr, stdin};

    let mut err = stderr();
    write!(err, "{prompt}").ok();
    err.flush().ok();

    let mut line = String::new();
    if stdin().read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_owned()))
    }
}

fn delete_project_from_picker(
    loaded: Option<&config::LoadedConfig>,
    project_name: &str,
) -> Result<PathBuf> {
    let loaded = loaded.context("project deletion requires config or project files")?;
    config::delete_project_file(loaded, project_name)
}

fn edit_project_from_picker(
    loaded: Option<&config::LoadedConfig>,
    project_name: &str,
) -> Result<()> {
    let loaded = loaded.context("editing a project requires config or project files")?;
    let path = config::project_file_path(loaded, project_name)?;
    let Some(command) = editor_command() else {
        anyhow::bail!("no editor set: export $EDITOR (or $VISUAL) to edit project files");
    };
    let (program, args) = command
        .split_first()
        .expect("editor_command never returns an empty command");
    let status = std::process::Command::new(program)
        .args(args)
        .arg(&path)
        .status()
        .with_context(|| format!("failed to launch editor `{program}`"))?;
    if !status.success() {
        anyhow::bail!("editor `{program}` exited with a non-zero status");
    }
    Ok(())
}

/// Resolve the editor command from `$VISUAL`, then `$EDITOR`, split into a
/// program and its leading arguments (so values like `code --wait` work). Returns
/// `None` when neither variable is set to a non-empty value.
fn editor_command() -> Option<Vec<String>> {
    for var in ["VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(var) {
            let parts = split_editor_command(&value);
            if !parts.is_empty() {
                return Some(parts);
            }
        }
    }
    None
}

fn split_editor_command(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(|part| part.to_owned())
        .collect()
}

fn save_project_from_picker(
    tmux: &Tmux,
    session_name: &str,
    config_path: Option<&Path>,
) -> Result<Option<PathBuf>> {
    project_export::save_project(
        tmux,
        Some(session_name),
        Some(session_name),
        None,
        false,
        // force = true: pressing "save" on a session whose project already
        // exists updates it in place rather than failing.
        true,
        config_path,
    )
}

/// Directory candidates for the picker, gathered once per `smux select` run.
struct ScannedDirectories {
    /// Deduplicated directories, zoxide results first.
    directories: Vec<String>,
    zoxide_available: bool,
}

fn scan_directories(loaded: Option<&config::LoadedConfig>) -> ScannedDirectories {
    let mut directories = Vec::new();
    let mut directory_keys = HashSet::new();
    let mut zoxide_available = true;

    match zoxide::list_directories() {
        Ok(zoxide_directories) => {
            for directory in zoxide_directories {
                if insert_directory_key(&mut directory_keys, &directory) {
                    directories.push(directory);
                }
            }
        }
        Err(error) => {
            zoxide_available = false;
            eprintln!("warning: {error:#}");
        }
    }

    let folder_search_settings = loaded
        .map(|loaded| loaded.config.settings.folder_search.clone())
        .unwrap_or_default();
    let result = folder_search::list_directories(&folder_search_settings);
    for warning in result.warnings {
        eprintln!(
            "warning: folder search {}: {}",
            warning.root, warning.message
        );
    }
    for directory in result.directories {
        if insert_directory_key(&mut directory_keys, &directory) {
            directories.push(directory);
        }
    }

    ScannedDirectories {
        directories,
        zoxide_available,
    }
}

fn select_entries(
    tmux: &Tmux,
    loaded: Option<&config::LoadedConfig>,
    display_style: DisplayStyle,
    current_session: Option<&str>,
    directories: &ScannedDirectories,
) -> Result<Vec<fzf::Entry>> {
    let mut entries = Vec::new();
    let sessions = tmux.list_sessions()?;
    let session_count = sessions.len();

    for session in sessions {
        let entry = if current_session == Some(session.as_str()) {
            fzf::Entry {
                kind: fzf::EntryKind::Session,
                label: display_style.current_session_label(&session),
                value: session,
                preview: None,
            }
        } else {
            fzf::Entry::session(display_style, session)
        };
        entries.push(entry);
    }

    // Windows across all sessions; shown only under the window filter key.
    match tmux.list_all_windows() {
        Ok(windows) => {
            for window in windows {
                entries.push(fzf::Entry::window(
                    display_style,
                    &window.session,
                    &window.name,
                    window.id,
                ));
            }
        }
        Err(error) => eprintln!("warning: {error:#}"),
    }

    if let Some(loaded) = loaded {
        let mut project_names = loaded.projects.keys().cloned().collect::<Vec<_>>();
        // Most recently saved/edited projects first (by file mtime), falling
        // back to name order when timestamps are equal or unavailable.
        project_names.sort_by(|left, right| {
            project_mtime(loaded, right)
                .cmp(&project_mtime(loaded, left))
                .then_with(|| left.cmp(right))
        });
        for project_name in project_names {
            let preview = loaded
                .project_files
                .get(&project_name)
                .map(|path| path.display().to_string());
            let project = &loaded.projects[&project_name];
            let label_value = project
                .session_name
                .as_deref()
                .unwrap_or(&project_name)
                .to_string();
            entries.push(fzf::Entry::project(
                display_style,
                project_name,
                label_value,
                preview,
            ));
        }
        let mut invalid_projects = loaded.invalid_projects.clone();
        invalid_projects.sort_by(|left, right| left.name.cmp(&right.name));
        for project in invalid_projects {
            entries.push(fzf::Entry::invalid_project(
                display_style,
                project.name,
                &project.error,
                Some(project.path.display().to_string()),
            ));
        }
    }

    for directory in &directories.directories {
        entries.push(fzf::Entry::directory(display_style, directory.clone()));
    }

    if entries.is_empty() {
        bail!(
            "{}",
            empty_select_message(
                session_count,
                directories.directories.len(),
                directories.zoxide_available
            )
        );
    }

    Ok(entries)
}

/// Last-modified time of a project's file, used to order projects by recency.
/// Missing files or unreadable metadata sort oldest.
fn project_mtime(
    loaded: &config::LoadedConfig,
    project_name: &str,
) -> Option<std::time::SystemTime> {
    let path = loaded.project_files.get(project_name)?;
    std::fs::metadata(path).ok()?.modified().ok()
}

fn insert_directory_key(seen: &mut HashSet<PathBuf>, directory: &str) -> bool {
    let key =
        util::normalize_path(Path::new(directory)).unwrap_or_else(|_| PathBuf::from(directory));
    seen.insert(key)
}

fn choose_template_name(
    config: Option<&config::Config>,
    display_style: DisplayStyle,
) -> Result<Option<String>> {
    let mut template_names = config
        .map(|config| config.templates.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    template_names.sort();
    template_names.insert(0, BUILTIN_TEMPLATE_LABEL.to_owned());

    let choices = template_names
        .into_iter()
        .map(|name| fzf::Choice::new("template", display_style.template_label(&name), name))
        .collect();

    Ok(resolve_template_choice(fzf::select_value(
        "template> ",
        choices,
    )?))
}

fn resolve_template_choice(choice: Option<String>) -> Option<String> {
    match choice.as_deref() {
        None => None,
        Some(BUILTIN_TEMPLATE_LABEL) => Some(session::BUILTIN_TEMPLATE_NAME.to_owned()),
        Some(choice) => Some(choice.to_owned()),
    }
}

fn empty_select_message(
    session_count: usize,
    directory_count: usize,
    zoxide_available: bool,
) -> String {
    match (session_count, directory_count, zoxide_available) {
        (0, 0, true) => {
            "nothing to select: tmux has no sessions, zoxide has no indexed directories, and folder search found no directories; run `smux connect <path>` or adjust `[settings.folder_search]`".to_owned()
        }
        (0, 0, false) => {
            "nothing to select: tmux has no sessions, zoxide is unavailable, and folder search found no directories; run `smux connect <path>` or adjust `[settings.folder_search]`".to_owned()
        }
        _ => "nothing to select".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{empty_select_message, resolve_template_choice, split_editor_command};
    use crate::session;

    #[test]
    fn cancelling_template_choice_returns_none() {
        assert_eq!(resolve_template_choice(None), None);
    }

    #[test]
    fn split_editor_command_handles_program_and_args() {
        assert_eq!(split_editor_command("nvim"), vec!["nvim".to_owned()]);
        assert_eq!(
            split_editor_command("code --wait"),
            vec!["code".to_owned(), "--wait".to_owned()]
        );
        assert!(split_editor_command("   ").is_empty());
        assert!(split_editor_command("").is_empty());
    }

    #[test]
    fn builtin_template_choice_maps_to_builtin_template_name() {
        assert_eq!(
            resolve_template_choice(Some("<builtin>".to_owned())).as_deref(),
            Some(session::BUILTIN_TEMPLATE_NAME)
        );
    }

    #[test]
    fn named_template_choice_is_preserved() {
        assert_eq!(
            resolve_template_choice(Some("rust".to_owned())).as_deref(),
            Some("rust")
        );
    }

    #[test]
    fn empty_select_message_is_actionable_with_empty_sources() {
        assert!(empty_select_message(0, 0, true).contains("smux connect <path>"));
        assert!(empty_select_message(0, 0, true).contains("zoxide"));
        assert!(empty_select_message(0, 0, true).contains("folder search"));
    }

    #[test]
    fn empty_select_message_mentions_missing_zoxide() {
        assert!(empty_select_message(0, 0, false).contains("zoxide is unavailable"));
    }
    use super::clone_destination;
    use crate::config::CloneSettings;
    use std::path::Path;

    #[test]
    fn clone_destination_prefers_the_configured_root() {
        let _guard = crate::util::test_env::lock();
        unsafe {
            std::env::set_var("HOME", "/Users/dev");
        }

        let with_root = CloneSettings {
            root: Some("~/Development".to_owned()),
            owners: Vec::new(),
        };
        assert_eq!(
            clone_destination(&with_root, "demo"),
            Path::new("/Users/dev/Development/demo")
        );

        let without_root = CloneSettings::default();
        assert_eq!(clone_destination(&without_root, "demo"), Path::new("demo"));

        unsafe {
            std::env::remove_var("HOME");
        }
    }
}
