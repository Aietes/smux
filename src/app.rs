use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::{Cli, Commands};
use crate::config;
use crate::docs;
use crate::doctor;
use crate::fzf;
use crate::session;
use crate::tmux::Tmux;
use crate::ui::DisplayStyle;
use crate::util;
use crate::zoxide;

const BUILTIN_TEMPLATE_LABEL: &str = "<builtin>";

pub fn run(cli: Cli) -> Result<()> {
    let tmux = Tmux::new();

    match cli.command {
        Commands::Select {
            choose_template,
            no_project_detect,
            config,
        } => {
            let loaded = config::load_optional(config.as_deref())?;
            run_select(&tmux, loaded.as_ref(), choose_template, no_project_detect)
        }
        Commands::Connect {
            path,
            template,
            session_name,
            config,
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
        Commands::ListSessions => {
            for session in tmux.list_sessions()? {
                println!("{session}");
            }

            Ok(())
        }
        Commands::Doctor { config } => doctor::run(config.as_deref()),
        Commands::ListTemplates { config } => {
            let loaded = config::load(config.as_deref())?;
            let mut names = loaded.config.templates.keys().cloned().collect::<Vec<_>>();
            names.sort();
            for name in names {
                println!("{name}");
            }
            Ok(())
        }
        Commands::ListProjects { config } => {
            let loaded = config::load_workspace(config.as_deref())?;
            let mut names = loaded.projects.keys().cloned().collect::<Vec<_>>();
            names.sort();
            for name in names {
                let project = &loaded.projects[&name];
                let resolved = util::expand_and_normalize_path(Path::new(&project.path))?;
                println!("{name}\t{}", resolved.display());
            }
            Ok(())
        }
        Commands::Init { config } => {
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
    }
}

fn run_select(
    tmux: &Tmux,
    loaded: Option<&config::LoadedConfig>,
    choose_template: bool,
    no_project_detect: bool,
) -> Result<()> {
    let mut entries = Vec::new();
    let config = loaded.map(|loaded| &loaded.config);
    let display_style = DisplayStyle::from_config(config);
    let current_session = tmux.current_session().ok().flatten();
    let sessions = tmux.list_sessions()?;
    let session_count = sessions.len();

    for session in sessions {
        let entry = if current_session.as_deref() == Some(session.as_str()) {
            fzf::Entry {
                kind: fzf::EntryKind::Session,
                label: display_style.current_session_label(&session),
                value: session,
            }
        } else {
            fzf::Entry::session(display_style, session)
        };
        entries.push(entry);
    }

    if let Some(loaded) = loaded {
        let mut project_names = loaded.projects.keys().cloned().collect::<Vec<_>>();
        project_names.sort();
        for project_name in project_names {
            entries.push(fzf::Entry::project(display_style, project_name));
        }
    }

    let mut zoxide_available = true;
    let mut directory_count = 0;

    match zoxide::list_directories() {
        Ok(directories) => {
            directory_count = directories.len();
            for directory in directories {
                entries.push(fzf::Entry::directory(display_style, directory));
            }
        }
        Err(error) => {
            zoxide_available = false;
            eprintln!("warning: {error:#}");
        }
    }

    if entries.is_empty() {
        bail!(
            "{}",
            empty_select_message(session_count, directory_count, zoxide_available)
        );
    }

    let Some(selection) = fzf::select(entries)? else {
        return Ok(());
    };
    let project_detection = if no_project_detect {
        session::ProjectDetection::Disabled
    } else {
        session::ProjectDetection::Enabled
    };

    match selection.kind {
        fzf::EntryKind::Session => session::switch_existing(tmux, &selection.value),
        fzf::EntryKind::Directory => {
            let template = if choose_template {
                let Some(template) = choose_template_name(config, display_style)? else {
                    return Ok(());
                };
                Some(template)
            } else {
                None
            };

            session::connect_path(
                tmux,
                Path::new(&selection.value),
                loaded,
                template.as_deref(),
                None,
                project_detection,
            )
        }
        fzf::EntryKind::Project => {
            let loaded = loaded.context("project selection requires config or project files")?;
            session::connect_project(tmux, loaded, &selection.value)
        }
    }
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
            "nothing to select: tmux has no sessions and zoxide has no indexed directories; run `smux connect <path>` or add directories to zoxide first".to_owned()
        }
        (0, 0, false) => {
            "nothing to select: tmux has no sessions and zoxide is unavailable; run `smux connect <path>` or install zoxide".to_owned()
        }
        _ => "nothing to select".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{empty_select_message, resolve_template_choice};
    use crate::session;

    #[test]
    fn cancelling_template_choice_returns_none() {
        assert_eq!(resolve_template_choice(None), None);
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
    }

    #[test]
    fn empty_select_message_mentions_missing_zoxide() {
        assert!(empty_select_message(0, 0, false).contains("install zoxide"));
    }
}
