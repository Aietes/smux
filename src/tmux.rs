use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::config::SplitDirection;
use crate::process::{CommandOutput, CommandRunner, default_runner};
use crate::templates::SessionPlan;
use crate::util;

#[derive(Clone)]
pub struct Tmux {
    runner: Arc<dyn CommandRunner>,
}

impl Default for Tmux {
    fn default() -> Self {
        Self::new()
    }
}

impl Tmux {
    pub fn new() -> Self {
        Self {
            runner: default_runner(),
        }
    }

    pub fn with_runner(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }

    pub fn list_sessions(&self) -> Result<Vec<String>> {
        let output = self.runner.run_capture(
            "tmux",
            &[
                "list-sessions".to_owned(),
                "-F".to_owned(),
                "#{session_name}".to_owned(),
            ],
        );

        match output {
            Ok(output) if output.status.success => {
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
        let output = self
            .runner
            .run_capture(
                "tmux",
                &[
                    "has-session".to_owned(),
                    "-t".to_owned(),
                    session.to_owned(),
                ],
            )
            .context("failed to execute tmux has-session")?;

        Ok(output.status.success)
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
        let output = self
            .run_tmux_capture([
                "new-session",
                "-d",
                "-s",
                session,
                "-c",
                &directory,
                "-n",
                "main",
            ])
            .context("failed to execute tmux new-session")?;

        if output.status.success {
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
        let args = if util::inside_tmux() {
            vec![
                "switch-client".to_owned(),
                "-t".to_owned(),
                session.to_owned(),
            ]
        } else {
            vec![
                "attach-session".to_owned(),
                "-t".to_owned(),
                session.to_owned(),
            ]
        };

        let status = self
            .runner
            .run_inherit("tmux", &args)
            .context("failed to execute tmux switch/attach")?;

        if status.success {
            Ok(())
        } else {
            bail!("tmux switch/attach failed with status {:?}", status.code)
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
        let output = self.run_tmux_capture(args)?;

        if output.status.success {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("{}", stderr.trim())
        }
    }

    fn run_tmux_owned(&self, args: Vec<String>) -> Result<()> {
        let output = self.runner.run_capture("tmux", &args)?;

        if output.status.success {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("{}", stderr.trim())
        }
    }

    fn run_tmux_capture<const N: usize>(&self, args: [&str; N]) -> Result<CommandOutput> {
        let args = args.into_iter().map(ToOwned::to_owned).collect::<Vec<_>>();
        self.runner.run_capture("tmux", &args).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::process::{CommandOutput, CommandStatus, FakeCommandRunner, IoMode};
    use crate::templates::{PanePlan, SessionPlan, WindowPlan};

    use super::Tmux;

    #[test]
    fn outside_tmux_uses_inherited_stdio_for_attach() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_inherit(Ok(CommandStatus {
            success: true,
            code: Some(0),
        }));

        unsafe {
            std::env::remove_var("TMUX");
        }

        let tmux = Tmux::with_runner(runner.clone());
        tmux.switch_or_attach("demo")
            .expect("attach should succeed");

        let recorded = runner.recorded();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].program, "tmux");
        assert_eq!(recorded[0].args, vec!["attach-session", "-t", "demo"]);
        assert_eq!(recorded[0].io_mode, IoMode::Inherit);
    }

    #[test]
    fn session_plan_emits_expected_tmux_commands() {
        let runner = Arc::new(FakeCommandRunner::new());
        for _ in 0..8 {
            runner.push_capture(Ok(CommandOutput {
                status: CommandStatus {
                    success: true,
                    code: Some(0),
                },
                stdout: Vec::new(),
                stderr: Vec::new(),
            }));
        }

        let tmux = Tmux::with_runner(runner.clone());
        let plan = SessionPlan {
            session_name: "demo".to_owned(),
            startup_window: "editor".to_owned(),
            windows: vec![
                WindowPlan {
                    name: "editor".to_owned(),
                    cwd: "/tmp/demo".into(),
                    command: Some("nvim".to_owned()),
                    layout: None,
                    panes: Vec::new(),
                },
                WindowPlan {
                    name: "run".to_owned(),
                    cwd: "/tmp/demo".into(),
                    command: None,
                    layout: Some("main-horizontal".to_owned()),
                    panes: vec![
                        PanePlan {
                            split: None,
                            size: None,
                            cwd: "/tmp/demo".into(),
                            command: Some("cargo run".to_owned()),
                        },
                        PanePlan {
                            split: Some(crate::config::SplitDirection::Vertical),
                            size: None,
                            cwd: "/tmp/demo".into(),
                            command: Some("cargo test".to_owned()),
                        },
                    ],
                },
            ],
        };

        tmux.create_session_from_plan(&plan)
            .expect("session plan should succeed");

        let recorded = runner.recorded();
        assert_eq!(recorded[0].args[..4], ["new-session", "-d", "-s", "demo"]);
        assert_eq!(
            recorded[1].args,
            vec!["send-keys", "-t", "demo:editor", "nvim", "C-m"]
        );
        assert_eq!(
            recorded[2].args,
            vec!["new-window", "-t", "demo", "-n", "run", "-c", "/tmp/demo"]
        );
        assert_eq!(
            recorded[3].args,
            vec!["send-keys", "-t", "demo:run", "cargo run", "C-m"]
        );
        assert_eq!(
            recorded[4].args,
            vec!["split-window", "-t", "demo:run", "-v", "-c", "/tmp/demo"]
        );
        assert_eq!(
            recorded[5].args,
            vec!["send-keys", "-t", "demo:run", "cargo test", "C-m"]
        );
        assert_eq!(
            recorded[6].args,
            vec!["select-layout", "-t", "demo:run", "main-horizontal"]
        );
        assert_eq!(recorded[7].args, vec!["select-window", "-t", "demo:editor"]);
    }
}
