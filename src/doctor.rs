use anyhow::Result;

use crate::util;

pub fn run() -> Result<()> {
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

    Ok(())
}

fn status(available: bool) -> &'static str {
    if available { "ok" } else { "missing" }
}
