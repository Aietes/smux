use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::{Cli, Commands};
use crate::config;
use crate::docs;
use crate::doctor;
use crate::fzf;
use crate::session;
use crate::tmux::Tmux;
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
            run_select(
                &tmux,
                loaded.as_ref().map(|loaded| &loaded.config),
                choose_template,
                no_project_detect,
            )
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
                loaded.as_ref().map(|loaded| &loaded.config),
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
            let loaded = config::load(config.as_deref())?;
            let mut names = loaded.config.projects.keys().cloned().collect::<Vec<_>>();
            names.sort();
            for name in names {
                let project = &loaded.config.projects[&name];
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
    config: Option<&config::Config>,
    choose_template: bool,
    no_project_detect: bool,
) -> Result<()> {
    let mut entries = Vec::new();

    for session in tmux.list_sessions()? {
        entries.push(fzf::Entry::session(session));
    }

    match zoxide::list_directories() {
        Ok(directories) => {
            for directory in directories {
                entries.push(fzf::Entry::directory(directory));
            }
        }
        Err(error) => eprintln!("warning: {error:#}"),
    }

    if entries.is_empty() {
        bail!("no tmux sessions or zoxide directories available");
    }

    let selection = fzf::select(entries)?.context("fzf returned no selection")?;
    let project_detection = if no_project_detect {
        session::ProjectDetection::Disabled
    } else {
        session::ProjectDetection::Enabled
    };

    match selection.kind {
        fzf::EntryKind::Session => session::switch_existing(tmux, &selection.value),
        fzf::EntryKind::Directory => {
            let template = if choose_template {
                choose_template_name(config)?
            } else {
                None
            };

            session::connect_path(
                tmux,
                Path::new(&selection.value),
                config,
                template.as_deref(),
                None,
                project_detection,
            )
        }
    }
}

fn choose_template_name(config: Option<&config::Config>) -> Result<Option<String>> {
    let mut template_names = config
        .map(|config| config.templates.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    template_names.sort();
    template_names.insert(0, BUILTIN_TEMPLATE_LABEL.to_owned());

    let choice = fzf::select_value("template> ", template_names)?
        .context("template selection was cancelled")?;

    if choice == BUILTIN_TEMPLATE_LABEL {
        Ok(Some(session::BUILTIN_TEMPLATE_NAME.to_owned()))
    } else {
        Ok(Some(choice))
    }
}
