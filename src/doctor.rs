use std::path::Path;

use anyhow::{Result, bail};

use crate::config::{self, IconMode};
use crate::tmux::Tmux;
use crate::ui::DisplayStyle;
use crate::util;
use crate::zoxide;

pub fn run(config_path: Option<&Path>) -> Result<()> {
    let tmux = util::command_available("tmux");
    let fzf = util::command_available("fzf");
    let zoxide_available = util::command_available("zoxide");
    let mut has_error = false;

    println!("tmux: {}", status(tmux));
    println!("fzf: {}", status(fzf));
    println!("zoxide: {}", status(zoxide_available));

    if tmux {
        match Tmux::new().list_sessions() {
            Ok(sessions) => println!("tmux_sessions: {}", sessions.len()),
            Err(error) => {
                println!("tmux_sessions: error");
                println!("tmux_sessions_error: {error:#}");
            }
        }
    } else {
        println!("tmux_sessions: unavailable");
    }

    if zoxide_available {
        match zoxide::list_directories() {
            Ok(directories) => println!("zoxide_directories: {}", directories.len()),
            Err(error) => println!("zoxide_directories: error ({error:#})"),
        }
    } else {
        println!("zoxide_directories: unavailable");
    }

    if !tmux || !fzf {
        has_error = true;
    }

    match config::load_optional(config_path) {
        Ok(Some(loaded)) => {
            println!("config: ok ({})", loaded.path.display());
            print_icon_status(
                loaded.config.settings.icons,
                loaded.config.settings.icon_colors,
            );
        }
        Ok(None) => {
            println!("config: missing");
            print_icon_status(IconMode::Auto, Default::default());
        }
        Err(error) => {
            has_error = true;
            println!("config: error");
            println!("config_error: {error:#}");
            println!("icons: unknown (config error)");
        }
    }

    if has_error {
        println!("doctor: error");
        bail!("doctor checks failed");
    }

    println!("doctor: ok");

    Ok(())
}

fn status(available: bool) -> &'static str {
    if available { "ok" } else { "missing" }
}

fn print_icon_status(icon_mode: IconMode, icon_colors: crate::config::IconColors) {
    let style = DisplayStyle::new(icon_mode, icon_colors);
    let state = if style.icons_enabled() {
        "enabled"
    } else {
        "disabled"
    };

    println!(
        "icons: {state} (mode: {}; colors: session={}, directory={}, template={}; Nerd Font support is not auto-detectable)",
        style.icon_mode().as_str(),
        style.icon_colors().session,
        style.icon_colors().directory,
        style.icon_colors().template,
    );
}
