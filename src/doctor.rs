use std::path::Path;

use anyhow::{Result, bail};

use crate::config;
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
        }
        Ok(None) => {
            println!("config: missing");
        }
        Err(error) => {
            has_error = true;
            println!("config: error");
            println!("config_error: {error:#}");
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
