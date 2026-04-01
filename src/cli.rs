use std::path::PathBuf;

use clap::CommandFactory;
use clap::{Parser, Subcommand, ValueHint};
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(name = "smux")]
#[command(version)]
#[command(arg_required_else_help = true)]
#[command(about = "Small Rust CLI for tmux session selection and creation")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    pub fn command() -> clap::Command {
        <Self as CommandFactory>::command()
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Open the unified tmux-session and zoxide-directory selector.
    Select {
        #[arg(long)]
        choose_template: bool,
        #[arg(long)]
        no_project_detect: bool,
        #[arg(long)]
        #[arg(value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Create or reuse a tmux session for a directory.
    Connect {
        #[arg(value_hint = ValueHint::DirPath)]
        path: PathBuf,
        #[arg(long)]
        template: Option<String>,
        #[arg(long)]
        session_name: Option<String>,
        #[arg(long)]
        #[arg(value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Switch to or attach an existing tmux session.
    Switch { session: String },
    /// Print current tmux session names.
    ListSessions,
    /// Print configured template names.
    ListTemplates {
        #[arg(long)]
        #[arg(value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Print configured project entries.
    ListProjects {
        #[arg(long)]
        #[arg(value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Validate runtime dependencies and basic environment state.
    Doctor {
        #[arg(long)]
        fix: bool,
        #[arg(long)]
        #[arg(value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Capture a tmux session as a project file.
    SaveProject {
        name: String,
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        #[arg(value_hint = ValueHint::DirPath)]
        path: Option<PathBuf>,
        #[arg(long)]
        stdout: bool,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        #[arg(value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Write an initial configuration file.
    Init {
        #[arg(long)]
        #[arg(value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Generate shell completion scripts.
    Completions {
        shell: Shell,
        #[arg(long)]
        #[arg(value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
    },
    /// Generate man pages.
    Man {
        #[arg(long)]
        #[arg(value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
    },
}
