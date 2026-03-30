use std::path::Path;

use anyhow::{Result, bail};

use crate::config::{self, IconMode};
use crate::ui::DisplayStyle;
use crate::util;

pub fn run(config_path: Option<&Path>) -> Result<()> {
    let tmux = util::command_available("tmux");
    let fzf = util::command_available("fzf");
    let zoxide = util::command_available("zoxide");
    let mut has_error = false;

    println!("tmux: {}", status(tmux));
    println!("fzf: {}", status(fzf));
    println!("zoxide: {}", status(zoxide));

    if !tmux || !fzf {
        has_error = true;
    }

    match config::load_optional(config_path) {
        Ok(Some(loaded)) => {
            println!("config: ok ({})", loaded.path.display());
            print_icon_status(loaded.config.settings.icons);
        }
        Ok(None) => {
            println!("config: missing");
            print_icon_status(IconMode::Auto);
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

fn print_icon_status(icon_mode: IconMode) {
    let style = DisplayStyle::from_icon_mode(icon_mode);
    let state = if style.icons_enabled() {
        "enabled"
    } else {
        "disabled"
    };

    println!(
        "icons: {state} (mode: {}; Nerd Font support is not auto-detectable)",
        style.icon_mode().as_str()
    );
}
