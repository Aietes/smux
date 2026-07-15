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
    /// Path to the config file (default: ~/.config/smux/config.toml).
    #[arg(long, short = 'c', global = true)]
    #[arg(value_hint = ValueHint::FilePath)]
    pub config: Option<PathBuf>,
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
        /// Skip project auto-detection and open folders with a template only.
        #[arg(long)]
        no_project_detect: bool,
    },
    /// Create or reuse a tmux session for a directory.
    Connect {
        /// Directory to open the session in.
        #[arg(value_hint = ValueHint::DirPath)]
        path: PathBuf,
        /// Template to apply instead of the auto-detected one.
        #[arg(long)]
        template: Option<String>,
        /// Session name to use instead of one derived from the directory name.
        #[arg(long)]
        session_name: Option<String>,
    },
    /// Show which template smux would auto-detect for a directory, and why.
    #[command(
        long_about = "Print every template whose `match` files or `match_dependencies` are present in a directory, ranked the way smux auto-selects them: highest `priority` first, then a dependency match over a file match, then the longest matched marker, then the alphabetically first name. The top entry (marked with an arrow) is the template smux would apply. Useful for debugging why a folder opens with an unexpected layout, without launching a session."
    )]
    Detect {
        /// Directory to run template auto-detection against.
        #[arg(value_hint = ValueHint::DirPath)]
        path: PathBuf,
        /// Print only the winning template name; exit 1 when nothing matches.
        #[arg(long, short = 'q')]
        quiet: bool,
    },
    /// Switch to or attach an existing tmux session.
    Switch {
        /// Exact name of the tmux session to switch to.
        session: String,
    },
    /// Switch to the most recently used tmux session.
    Last,
    /// Kill a tmux session (default: the current one).
    #[command(
        long_about = "Kill a tmux session by exact name. With no name, inside tmux, the client switches to the last session first and then kills the one it was on, so the terminal survives the kill."
    )]
    Kill {
        /// Exact name of the session to kill. Defaults to the current session.
        session: Option<String>,
    },
    /// Clone a git repository (or browse your GitHub repos) and connect to it.
    #[command(
        long_about = "With a URL: run `git clone` and open the result with `smux connect`, so template auto-detection picks the right layout for the fresh checkout. When the target directory already exists, the clone is skipped and smux just connects.\n\nWithout a URL: browse your GitHub repositories (and those of any `[settings.clone] owners`) in a fuzzy picker — visibility, last update, and description included — then clone the selection with `gh repo clone`. Requires the GitHub CLI (gh).\n\nClones land in `[settings.clone] root` when set (falling back to the current directory) unless a target directory is given with `--dir`."
    )]
    Clone {
        /// Repository URL — anything `git clone` accepts. Omit to browse your
        /// GitHub repositories instead (requires gh).
        url: Option<String>,
        /// Target directory. Defaults to the repository name inside
        /// `[settings.clone] root` (or the current directory).
        #[arg(long, value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
        /// Template to apply instead of the auto-detected one.
        #[arg(long)]
        template: Option<String>,
        /// Just clone and print the checkout path; skip connecting a session.
        #[arg(long)]
        no_connect: bool,
    },
    /// Kill all detached tmux sessions.
    #[command(
        long_about = "Kill every tmux session that has no attached client. The session you are currently attached to is preserved; outside tmux, all sessions are detached and will be killed."
    )]
    Prune,
    /// Print current tmux session names.
    ListSessions {
        /// Emit a JSON array instead of one name per line.
        #[arg(long)]
        json: bool,
    },
    /// Print configured template names.
    ListTemplates {
        /// Emit a JSON array instead of one name per line.
        #[arg(long)]
        json: bool,
    },
    /// Print configured project entries.
    ListProjects {
        /// Emit a JSON array of {name, path} objects instead of tab-separated lines.
        #[arg(long)]
        json: bool,
    },
    /// Validate runtime dependencies and basic environment state.
    Doctor {
        /// Apply safe fixes (create missing config and directories) instead of
        /// only reporting them.
        #[arg(long)]
        fix: bool,
    },
    /// Capture a tmux session as a project file.
    #[command(
        long_about = "Capture a tmux session's windows, panes, and layout as a project file.\n\nNAME defaults to the source session's name when omitted. Pass --force to overwrite (update) an existing project file."
    )]
    SaveProject {
        /// Project name. Defaults to the source session's name.
        name: Option<String>,
        /// Source tmux session. Defaults to the current session inside tmux.
        #[arg(long)]
        session: Option<String>,
        /// Project path to record instead of the session's active directory.
        #[arg(long)]
        #[arg(value_hint = ValueHint::DirPath)]
        path: Option<PathBuf>,
        /// Print the project TOML to stdout instead of writing a file.
        #[arg(long)]
        stdout: bool,
        /// Overwrite an existing project file with the same name.
        #[arg(long)]
        force: bool,
    },
    /// Write an initial configuration file.
    Init,
    /// Generate shell completion scripts.
    Completions {
        /// Shell to generate completions for.
        shell: Shell,
        /// Write the script into this directory instead of stdout.
        #[arg(long)]
        #[arg(value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
    },
    /// Generate man pages.
    Man {
        /// Write the man pages into this directory instead of stdout.
        #[arg(long)]
        #[arg(value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
    },
    /// Write the bundled Claude Code skill for authoring smux config.
    #[command(
        long_about = "Write (or print) the bundled Claude Code skill that teaches an AI assistant how to author and debug smux templates and projects. With --dir, writes <dir>/SKILL.md (creating the directory), e.g. `smux skill --dir ~/.claude/skills/smux`. Without --dir, prints the skill to stdout. The skill is embedded in the binary, so it always matches this version — re-run after an upgrade to refresh it."
    )]
    Skill {
        /// Write SKILL.md into this directory instead of stdout.
        #[arg(long)]
        #[arg(value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
    },
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use super::{Cli, Commands};

    #[test]
    fn clone_dir_option_does_not_require_a_url() {
        let cli = Cli::try_parse_from(["smux", "clone", "--dir", "/tmp/demo"])
            .expect("clone --dir should parse without a URL");

        let Commands::Clone { url, dir, .. } = cli.command else {
            panic!("clone command should parse");
        };
        assert_eq!(url, None);
        assert_eq!(dir, Some(PathBuf::from("/tmp/demo")));
    }

    #[test]
    fn clone_dir_option_parses_with_a_url() {
        let cli = Cli::try_parse_from([
            "smux",
            "clone",
            "https://example.com/user/demo.git",
            "--dir",
            "/tmp/demo",
        ])
        .expect("clone --dir should parse with a URL");

        let Commands::Clone { url, dir, .. } = cli.command else {
            panic!("clone command should parse");
        };
        assert_eq!(url.as_deref(), Some("https://example.com/user/demo.git"));
        assert_eq!(dir, Some(PathBuf::from("/tmp/demo")));
    }
}
