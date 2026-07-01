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
    /// Open the unified tmux session, project, and directory selector.
    #[command(
        long_about = "Open the unified picker for tmux sessions, saved projects, zoxide directories, and configured folder-search results.\n\nEnter opens the selected item. Ctrl-X deletes the selected non-current session or project file. Alt-S saves the selected tmux session as a project. Ctrl-R renames the selected session. Press ? to toggle the keyboard-shortcut hints."
    )]
    Select {
        /// Always prompt for a template when opening a folder. Without this, the
        /// picker only prompts when no template resolves automatically and two or
        /// more templates are defined.
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
    /// Show which template smux would auto-detect for a directory, and why.
    #[command(
        long_about = "Print every template whose `match` files or `match_dependencies` are present in a directory, ranked the way smux auto-selects them: highest `priority` first, then a dependency match over a file match, then the longest matched marker, then the alphabetically first name. The top entry (marked with an arrow) is the template smux would apply. Useful for debugging why a folder opens with an unexpected layout, without launching a session."
    )]
    Detect {
        #[arg(value_hint = ValueHint::DirPath)]
        path: PathBuf,
        #[arg(long)]
        #[arg(value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Switch to or attach an existing tmux session.
    Switch { session: String },
    /// Switch to the most recently used tmux session.
    Last,
    /// Kill all detached tmux sessions.
    #[command(
        long_about = "Kill every tmux session that has no attached client. The session you are currently attached to is preserved; outside tmux, all sessions are detached and will be killed."
    )]
    Prune,
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
    #[command(
        long_about = "Capture a tmux session's windows, panes, and layout as a project file.\n\nNAME defaults to the source session's name when omitted. Pass --force to overwrite (update) an existing project file."
    )]
    SaveProject {
        /// Project name. Defaults to the source session's name.
        name: Option<String>,
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
    /// Write the bundled Claude Code skill for authoring smux config.
    #[command(
        long_about = "Write (or print) the bundled Claude Code skill that teaches an AI assistant how to author and debug smux templates and projects. With --dir, writes <dir>/SKILL.md (creating the directory), e.g. `smux skill --dir ~/.claude/skills/smux`. Without --dir, prints the skill to stdout. The skill is embedded in the binary, so it always matches this version — re-run after an upgrade to refresh it."
    )]
    Skill {
        #[arg(long)]
        #[arg(value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
    },
}
