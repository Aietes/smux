use anyhow::Result;

use crate::config;
use crate::util;

pub fn run(config_path: Option<&std::path::Path>) -> Result<()> {
    let tmux = util::command_available("tmux");
    let fzf = util::command_available("fzf");
    let zoxide = util::command_available("zoxide");

    println!("tmux: {}", status(tmux));
    println!("fzf: {}", status(fzf));
    println!("zoxide: {}", status(zoxide));

    if tmux && fzf {
        println!("doctor: ok");
    } else {
        println!("doctor: missing required dependencies");
    }

    match config::load_optional(config_path) {
        Ok(Some(loaded)) => {
            println!("config: ok ({})", loaded.path.display());
        }
        Ok(None) => {
            println!("config: missing");
        }
        Err(error) => {
            println!("config: error");
            println!("config_error: {error:#}");
        }
    }

    Ok(())
}

fn status(available: bool) -> &'static str {
    if available { "ok" } else { "missing" }
}
