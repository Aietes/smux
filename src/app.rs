use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::{Cli, Commands};
use crate::config;
use crate::doctor;
use crate::fzf;
use crate::session;
use crate::tmux::Tmux;
use crate::util;
use crate::zoxide;

pub fn run(cli: Cli) -> Result<()> {
    let tmux = Tmux::new();

    match cli.command {
        Commands::Select {
            choose_template: _,
            no_project_detect: _,
            config,
        } => {
            let loaded = config::load_optional(config.as_deref())?;
            run_popup(&tmux, loaded.as_ref().map(|loaded| &loaded.config))
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
    }
}

fn run_popup(tmux: &Tmux, config: Option<&config::Config>) -> Result<()> {
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

    match selection.kind {
        fzf::EntryKind::Session => session::switch_existing(tmux, &selection.value),
        fzf::EntryKind::Directory => {
            session::connect_path(tmux, Path::new(&selection.value), config, None, None)
        }
    }
}
