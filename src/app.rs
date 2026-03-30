use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::{Cli, Commands};
use crate::doctor;
use crate::fzf;
use crate::session;
use crate::tmux::Tmux;
use crate::zoxide;

pub fn run(cli: Cli) -> Result<()> {
    let tmux = Tmux::new();

    match cli.command {
        Commands::Popup {
            choose_template: _,
            no_project_detect: _,
            config: _,
        } => run_popup(&tmux),
        Commands::Connect {
            path,
            template: _,
            session_name,
            config: _,
        } => session::connect_path(&tmux, &path, session_name.as_deref()),
        Commands::Switch { session } => session::switch_existing(&tmux, &session),
        Commands::ListSessions => {
            for session in tmux.list_sessions()? {
                println!("{session}");
            }

            Ok(())
        }
        Commands::Doctor => doctor::run(),
        Commands::ListTemplates => bail!("list-templates is not implemented yet"),
        Commands::ListProjects => bail!("list-projects is not implemented yet"),
        Commands::Init => bail!("init is not implemented yet"),
    }
}

fn run_popup(tmux: &Tmux) -> Result<()> {
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
        fzf::EntryKind::Directory => session::connect_path(tmux, Path::new(&selection.value), None),
    }
}
