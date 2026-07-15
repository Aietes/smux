mod app;
mod cli;
mod config;
mod docs;
mod doctor;
mod folder_search;
mod fzf;
mod github;
mod process;
mod project_export;
mod session;
mod skill;
mod templates;
mod tmux;
mod ui;
mod util;
mod zoxide;

use clap::Parser;

/// Parse process arguments and run the smux CLI.
pub fn run() -> anyhow::Result<()> {
    app::run(cli::Cli::parse())
}
