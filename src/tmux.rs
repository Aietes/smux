use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::process::{CommandOutput, CommandRunner, default_runner};
use crate::templates::{PaneLayout, PanePosition, SessionPlan};
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
        self.configure_panes(&plan.session_name, &first_window.name, first_window)?;

        for window in plan.windows.iter().skip(1) {
            self.new_window(&plan.session_name, &window.name, &window.cwd)?;
            self.configure_panes(&plan.session_name, &window.name, window)?;
        }

        self.select_window(&plan.session_name, &plan.startup_window)?;
        self.select_pane(&plan.session_name, &plan.startup_window, plan.startup_pane)?;
        Ok(())
    }

    pub fn switch_or_attach(&self, session: &str) -> Result<()> {
        if util::inside_tmux() {
            self.run_tmux(["switch-client", "-t", session])
                .context("failed to execute tmux switch-client")
        } else {
            let args = vec![
                "attach-session".to_owned(),
                "-t".to_owned(),
                session.to_owned(),
            ];

            let status = self
                .runner
                .run_inherit("tmux", &args)
                .context("failed to execute tmux attach-session")?;

            if status.success {
                Ok(())
            } else {
                bail!("tmux attach-session failed with status {:?}", status.code)
            }
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

    fn send_keys_to_target(&self, target: &str, command: &str) -> Result<()> {
        self.run_tmux(["send-keys", "-t", target, command, "C-m"])
            .context("failed to execute tmux send-keys")
    }

    fn split_window(&self, target: &str, layout: &PaneLayout, directory: &Path) -> Result<()> {
        let directory = util::path_to_string(directory)?;
        let mut args = vec![
            "split-window".to_owned(),
            "-t".to_owned(),
            target.to_owned(),
        ];

        match layout.position {
            PanePosition::Right | PanePosition::Left => args.push("-h".to_owned()),
            PanePosition::Bottom | PanePosition::Top => args.push("-v".to_owned()),
        }

        match layout.position {
            PanePosition::Left | PanePosition::Top => args.push("-b".to_owned()),
            PanePosition::Right | PanePosition::Bottom => {}
        }

        if let Some(size) = &layout.size {
            args.push("-l".to_owned());
            args.push(size.clone());
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

    fn select_pane(&self, session: &str, window: &str, pane_index: usize) -> Result<()> {
        let target = format!("{session}:{window}.{pane_index}");
        self.run_tmux(["select-pane", "-t", &target])
            .context("failed to execute tmux select-pane")
    }

    fn set_synchronize_panes(&self, session: &str, window: &str, enabled: bool) -> Result<()> {
        let target = format!("{session}:{window}");
        let value = if enabled { "on" } else { "off" };
        self.run_tmux([
            "set-window-option",
            "-t",
            &target,
            "synchronize-panes",
            value,
        ])
        .context("failed to execute tmux set-window-option")
    }

    fn configure_panes(
        &self,
        session: &str,
        window: &str,
        plan: &crate::templates::WindowPlan,
    ) -> Result<()> {
        if plan.panes.is_empty() {
            let target = format!("{session}:{window}.0");
            if let Some(pre_command) = &plan.pre_command {
                self.send_keys_to_target(&target, pre_command)?;
            }
            if let Some(command) = &plan.command {
                self.send_keys_to_target(&target, command)?;
            }
            if plan.synchronize {
                self.set_synchronize_panes(session, window, true)?;
            }
            return Ok(());
        }

        let target = format!("{session}:{window}");
        let first_pane_target = format!("{target}.0");

        if let Some(pre_command) = &plan.pre_command {
            self.send_keys_to_target(&first_pane_target, pre_command)
                .context("failed to execute tmux send-keys for first pane pre_command")?;
        }
        if let Some(command) = &plan.panes[0].command {
            self.send_keys_to_target(&first_pane_target, command)
                .context("failed to execute tmux send-keys for first pane")?;
        }

        for (pane_index, pane) in plan.panes.iter().enumerate().skip(1) {
            let layout = pane.layout.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "pane {} in window \"{}\" is missing a layout",
                    pane_index,
                    window
                )
            })?;
            self.split_window(&target, layout, &pane.cwd)?;
            let pane_target = format!("{target}.{pane_index}");
            if let Some(pre_command) = &plan.pre_command {
                self.send_keys_to_target(&pane_target, pre_command)
                    .context("failed to execute tmux send-keys for split pane pre_command")?;
            }
            if let Some(command) = &pane.command {
                self.send_keys_to_target(&pane_target, command)
                    .context("failed to execute tmux send-keys for split pane")?;
            }
        }

        if let Some(layout) = &plan.layout {
            self.select_layout(&target, layout)?;
        }

        if plan.synchronize {
            self.set_synchronize_panes(session, window, true)?;
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
    use crate::templates::{PaneLayout, PanePlan, PanePosition, SessionPlan, WindowPlan};

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
    fn inside_tmux_uses_switch_client_with_captured_io() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: Vec::new(),
            stderr: Vec::new(),
        }));

        unsafe {
            std::env::set_var("TMUX", "/tmp/tmux-test,123,0");
        }

        let tmux = Tmux::with_runner(runner.clone());
        tmux.switch_or_attach("demo")
            .expect("switch-client should succeed");

        let recorded = runner.recorded();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].program, "tmux");
        assert_eq!(recorded[0].args, vec!["switch-client", "-t", "demo"]);
        assert_eq!(recorded[0].io_mode, IoMode::Capture);

        unsafe {
            std::env::remove_var("TMUX");
        }
    }

    #[test]
    fn session_plan_emits_expected_tmux_commands() {
        let runner = Arc::new(FakeCommandRunner::new());
        for _ in 0..13 {
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
            startup_pane: 0,
            windows: vec![
                WindowPlan {
                    name: "editor".to_owned(),
                    cwd: "/tmp/demo".into(),
                    pre_command: Some("source .venv/bin/activate".to_owned()),
                    command: Some("nvim".to_owned()),
                    layout: None,
                    synchronize: false,
                    panes: Vec::new(),
                },
                WindowPlan {
                    name: "run".to_owned(),
                    cwd: "/tmp/demo".into(),
                    pre_command: Some("source .venv/bin/activate".to_owned()),
                    command: None,
                    layout: Some("main-horizontal".to_owned()),
                    synchronize: true,
                    panes: vec![
                        PanePlan {
                            layout: None,
                            cwd: "/tmp/demo".into(),
                            command: Some("cargo run".to_owned()),
                        },
                        PanePlan {
                            layout: Some(PaneLayout {
                                position: PanePosition::Right,
                                size: None,
                            }),
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
            vec![
                "send-keys",
                "-t",
                "demo:editor.0",
                "source .venv/bin/activate",
                "C-m"
            ]
        );
        assert_eq!(
            recorded[2].args,
            vec!["send-keys", "-t", "demo:editor.0", "nvim", "C-m"]
        );
        assert_eq!(
            recorded[3].args,
            vec!["new-window", "-t", "demo", "-n", "run", "-c", "/tmp/demo"]
        );
        assert_eq!(
            recorded[4].args,
            vec![
                "send-keys",
                "-t",
                "demo:run.0",
                "source .venv/bin/activate",
                "C-m"
            ]
        );
        assert_eq!(
            recorded[5].args,
            vec!["send-keys", "-t", "demo:run.0", "cargo run", "C-m"]
        );
        assert_eq!(
            recorded[6].args,
            vec!["split-window", "-t", "demo:run", "-h", "-c", "/tmp/demo"]
        );
        assert_eq!(
            recorded[7].args,
            vec![
                "send-keys",
                "-t",
                "demo:run.1",
                "source .venv/bin/activate",
                "C-m"
            ]
        );
        assert_eq!(
            recorded[8].args,
            vec!["send-keys", "-t", "demo:run.1", "cargo test", "C-m"]
        );
        assert_eq!(
            recorded[9].args,
            vec!["select-layout", "-t", "demo:run", "main-horizontal"]
        );
        assert_eq!(
            recorded[10].args,
            vec![
                "set-window-option",
                "-t",
                "demo:run",
                "synchronize-panes",
                "on"
            ]
        );
        assert_eq!(
            recorded[11].args,
            vec!["select-window", "-t", "demo:editor"]
        );
        assert_eq!(
            recorded[12].args,
            vec!["select-pane", "-t", "demo:editor.0"]
        );
    }
}
