use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::util;

#[derive(Debug, Default, Clone, Copy)]
pub struct Tmux;

impl Tmux {
    pub fn new() -> Self {
        Self
    }

    pub fn list_sessions(&self) -> Result<Vec<String>> {
        let output = Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let stdout =
                    String::from_utf8(output.stdout).context("tmux output was not utf-8")?;
                Ok(stdout
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(ToOwned::to_owned)
                    .collect())
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);

                if stderr.contains("no server running") {
                    Ok(Vec::new())
                } else {
                    bail!("tmux list-sessions failed: {}", stderr.trim())
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                bail!("tmux is not installed or not on PATH")
            }
            Err(error) => Err(error).context("failed to execute tmux list-sessions"),
        }
    }

    pub fn has_session(&self, session: &str) -> Result<bool> {
        let output = Command::new("tmux")
            .args(["has-session", "-t", session])
            .output()
            .context("failed to execute tmux has-session")?;

        Ok(output.status.success())
    }

    pub fn ensure_session_exists(&self, session: &str) -> Result<()> {
        if self.has_session(session)? {
            Ok(())
        } else {
            bail!("tmux session not found: {session}")
        }
    }

    pub fn create_session(&self, session: &str, directory: &Path) -> Result<()> {
        let directory = util::path_to_string(directory)?;
        let output = Command::new("tmux")
            .args([
                "new-session",
                "-d",
                "-s",
                session,
                "-c",
                &directory,
                "-n",
                "main",
            ])
            .output()
            .context("failed to execute tmux new-session")?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tmux new-session failed: {}", stderr.trim())
        }
    }

    pub fn switch_or_attach(&self, session: &str) -> Result<()> {
        let mut command = if util::inside_tmux() {
            let mut command = Command::new("tmux");
            command.args(["switch-client", "-t", session]);
            command
        } else {
            let mut command = Command::new("tmux");
            command.args(["attach-session", "-t", session]);
            command
        };

        let output = command
            .output()
            .context("failed to execute tmux switch/attach")?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tmux switch/attach failed: {}", stderr.trim())
        }
    }
}
