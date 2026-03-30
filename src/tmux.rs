use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::config::SplitDirection;
use crate::templates::SessionPlan;
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

    pub fn create_session_from_plan(&self, plan: &SessionPlan) -> Result<()> {
        let first_window = plan
            .windows
            .first()
            .context("session plan must contain at least one window")?;

        self.create_session_with_window(&plan.session_name, &first_window.name, &first_window.cwd)?;

        if let Some(command) = &first_window.command {
            self.send_keys_to_window(&plan.session_name, &first_window.name, command)?;
        }

        self.configure_panes(&plan.session_name, &first_window.name, first_window)?;

        for window in plan.windows.iter().skip(1) {
            self.new_window(&plan.session_name, &window.name, &window.cwd)?;

            if let Some(command) = &window.command {
                self.send_keys_to_window(&plan.session_name, &window.name, command)?;
            }

            self.configure_panes(&plan.session_name, &window.name, window)?;
        }

        self.select_window(&plan.session_name, &plan.startup_window)?;
        Ok(())
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

    fn create_session_with_window(
        &self,
        session: &str,
        window: &str,
        directory: &Path,
    ) -> Result<()> {
        let directory = util::path_to_string(directory)?;
        self.run_tmux([
            "new-session",
            "-d",
            "-s",
            session,
            "-c",
            &directory,
            "-n",
            window,
        ])
        .context("failed to execute tmux new-session")
    }

    fn new_window(&self, session: &str, window: &str, directory: &Path) -> Result<()> {
        let directory = util::path_to_string(directory)?;
        self.run_tmux(["new-window", "-t", session, "-n", window, "-c", &directory])
            .context("failed to execute tmux new-window")
    }

    fn send_keys_to_window(&self, session: &str, window: &str, command: &str) -> Result<()> {
        let target = format!("{session}:{window}");
        self.run_tmux(["send-keys", "-t", &target, command, "C-m"])
            .context("failed to execute tmux send-keys")
    }

    fn split_window(
        &self,
        target: &str,
        split: Option<&SplitDirection>,
        size: Option<&str>,
        directory: &Path,
    ) -> Result<()> {
        let directory = util::path_to_string(directory)?;
        let mut args = vec![
            "split-window".to_owned(),
            "-t".to_owned(),
            target.to_owned(),
        ];

        match split {
            Some(SplitDirection::Horizontal) => args.push("-h".to_owned()),
            Some(SplitDirection::Vertical) => args.push("-v".to_owned()),
            None => {}
        }

        if let Some(size) = size {
            args.push("-l".to_owned());
            args.push(size.to_owned());
        }

        args.push("-c".to_owned());
        args.push(directory);

        self.run_tmux_owned(args)
            .context("failed to execute tmux split-window")
    }

    fn select_layout(&self, target: &str, layout: &str) -> Result<()> {
        self.run_tmux(["select-layout", "-t", target, layout])
            .context("failed to execute tmux select-layout")
    }

    fn select_window(&self, session: &str, window: &str) -> Result<()> {
        let target = format!("{session}:{window}");
        self.run_tmux(["select-window", "-t", &target])
            .context("failed to execute tmux select-window")
    }

    fn configure_panes(
        &self,
        session: &str,
        window: &str,
        plan: &crate::templates::WindowPlan,
    ) -> Result<()> {
        if plan.panes.is_empty() {
            return Ok(());
        }

        let target = format!("{session}:{window}");

        if let Some(command) = &plan.panes[0].command {
            self.run_tmux(["send-keys", "-t", &target, command, "C-m"])
                .context("failed to execute tmux send-keys for first pane")?;
        }

        for pane in plan.panes.iter().skip(1) {
            self.split_window(
                &target,
                pane.split.as_ref(),
                pane.size.as_deref(),
                &pane.cwd,
            )?;
            if let Some(command) = &pane.command {
                self.run_tmux(["send-keys", "-t", &target, command, "C-m"])
                    .context("failed to execute tmux send-keys for split pane")?;
            }
        }

        if let Some(layout) = &plan.layout {
            self.select_layout(&target, layout)?;
        }

        Ok(())
    }

    fn run_tmux<const N: usize>(&self, args: [&str; N]) -> Result<()> {
        let output = Command::new("tmux").args(args).output()?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("{}", stderr.trim())
        }
    }

    fn run_tmux_owned(&self, args: Vec<String>) -> Result<()> {
        let output = Command::new("tmux").args(&args).output()?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("{}", stderr.trim())
        }
    }
}
