use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::process::{CommandOutput, CommandRunner, default_runner};
use crate::templates::{PaneLayout, PanePosition, SessionPlan};
use crate::util;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SessionSnapshot {
    pub session_name: String,
    pub active_window: String,
    pub active_pane: usize,
    pub active_path: std::path::PathBuf,
    pub windows: Vec<WindowSnapshot>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WindowSnapshot {
    pub name: String,
    pub synchronize: bool,
    pub active: bool,
    pub panes: Vec<PaneSnapshot>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PaneSnapshot {
    pub cwd: std::path::PathBuf,
    pub active: bool,
    pub layout: Option<PaneLayout>,
}

/// A window on the running tmux server, across all sessions.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GlobalWindow {
    pub id: String,
    pub session: String,
    pub name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SessionListing {
    activity: i64,
    attached: bool,
    name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct WindowRecord {
    id: String,
    name: String,
    active: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PaneRecord {
    index: usize,
    cwd: std::path::PathBuf,
    active: bool,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

#[derive(Clone)]
pub struct Tmux {
    runner: Arc<dyn CommandRunner>,
}

impl Default for Tmux {
    fn default() -> Self {
        Self::new()
    }
}

/// tmux resolves a bare `-t name` target with prefix and fnmatch fallbacks, so
/// `app` can silently match a session named `app-server` when no exact `app`
/// exists. The `=` prefix restricts resolution to the exact name.
fn exact_target(session: &str) -> String {
    format!("={session}")
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
        Ok(self
            .list_sessions_detailed()?
            .into_iter()
            .map(|session| session.name)
            .collect())
    }

    /// List detached sessions (no attached client), most recently active first.
    pub fn list_detached_sessions(&self) -> Result<Vec<String>> {
        Ok(self
            .list_sessions_detailed()?
            .into_iter()
            .filter(|session| !session.attached)
            .map(|session| session.name)
            .collect())
    }

    /// List sessions ordered by most recent activity first, carrying the
    /// attachment state used to distinguish detached sessions.
    fn list_sessions_detailed(&self) -> Result<Vec<SessionListing>> {
        let output = self.runner.run_capture(
            "tmux",
            &[
                "list-sessions".to_owned(),
                "-F".to_owned(),
                "#{session_activity}\t#{session_attached}\t#{session_name}".to_owned(),
            ],
        );

        match output {
            Ok(output) if output.status.success => {
                let stdout =
                    String::from_utf8(output.stdout).context("tmux output was not utf-8")?;
                let mut sessions = stdout
                    .lines()
                    .filter_map(parse_session_listing)
                    .collect::<Vec<_>>();
                // Most recently active first; ties keep tmux's order.
                sessions.sort_by_key(|session| std::cmp::Reverse(session.activity));
                Ok(sessions)
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);

                if is_empty_session_state(stderr.as_ref()) {
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

    pub fn current_session(&self) -> Result<Option<String>> {
        if !util::inside_tmux() {
            return Ok(None);
        }

        let output = self
            .runner
            .run_capture(
                "tmux",
                &[
                    "display-message".to_owned(),
                    "-p".to_owned(),
                    "#{session_name}".to_owned(),
                ],
            )
            .context("failed to execute tmux display-message")?;

        if !output.status.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tmux display-message failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8(output.stdout).context("tmux output was not utf-8")?;
        let session = stdout.trim();
        if session.is_empty() {
            Ok(None)
        } else {
            Ok(Some(session.to_owned()))
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
                    exact_target(session),
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
        self.create_session_with_window(session, "main", directory, &[])
    }

    pub fn create_session_from_plan(&self, plan: &SessionPlan) -> Result<()> {
        let first_window = plan
            .windows
            .first()
            .context("session plan must contain at least one window")?;

        self.create_session_with_window(
            &plan.session_name,
            &first_window.name,
            initial_pane_cwd(first_window),
            &plan.env,
        )?;

        let result: Result<()> = (|| {
            self.configure_panes(&plan.session_name, &first_window.name, first_window)?;

            let mut previous_window = first_window.name.as_str();
            for window in plan.windows.iter().skip(1) {
                self.new_window_after(
                    &plan.session_name,
                    previous_window,
                    &window.name,
                    initial_pane_cwd(window),
                )?;
                self.configure_panes(&plan.session_name, &window.name, window)?;
                previous_window = &window.name;
            }

            self.select_window(&plan.session_name, &plan.startup_window)?;
            self.select_pane_by_offset(
                &plan.session_name,
                &plan.startup_window,
                plan.startup_pane,
            )?;
            Ok(())
        })();

        if let Err(error) = result {
            return match self.kill_session(&plan.session_name) {
                Ok(()) => Err(error.context(format!(
                    "session \"{}\" setup failed; removed the incomplete session",
                    plan.session_name
                ))),
                Err(cleanup_error) => Err(error.context(format!(
                    "session \"{}\" setup failed; cleanup also failed: {cleanup_error:#}",
                    plan.session_name
                ))),
            };
        }

        Ok(())
    }

    pub fn switch_or_attach(&self, session: &str) -> Result<()> {
        if util::inside_tmux() {
            self.run_tmux(["switch-client", "-t", &exact_target(session)])
                .context("failed to execute tmux switch-client")
        } else {
            let args = vec![
                "attach-session".to_owned(),
                "-t".to_owned(),
                exact_target(session),
            ];

            let status = self
                .runner
                .run_inherit("tmux", &args)
                .context("failed to execute tmux attach-session")?;

            if status.success {
                Ok(())
            } else {
                bail!(
                    "tmux attach-session failed with {}",
                    util::exit_status_label(status.code)
                )
            }
        }
    }

    pub fn kill_session(&self, session: &str) -> Result<()> {
        self.run_tmux(["kill-session", "-t", &exact_target(session)])
            .context("failed to execute tmux kill-session")
    }

    pub fn rename_session(&self, session: &str, new_name: &str) -> Result<()> {
        self.run_tmux(["rename-session", "-t", &exact_target(session), new_name])
            .context("failed to execute tmux rename-session")
    }

    /// All windows on the server, across sessions. Window ids (`@n`) are
    /// globally unique, so they make unambiguous targets — no session:window
    /// string parsing, no collisions between same-named windows.
    pub fn list_all_windows(&self) -> Result<Vec<GlobalWindow>> {
        let output = self.runner.run_capture(
            "tmux",
            &[
                "list-windows".to_owned(),
                "-a".to_owned(),
                "-F".to_owned(),
                "#{window_id}\t#{session_name}\t#{window_name}".to_owned(),
            ],
        );

        match output {
            Ok(output) if output.status.success => {
                let stdout =
                    String::from_utf8(output.stdout).context("tmux output was not utf-8")?;
                Ok(stdout
                    .lines()
                    .filter_map(|line| {
                        let mut parts = line.splitn(3, '\t');
                        Some(GlobalWindow {
                            id: parts.next()?.to_owned(),
                            session: parts.next()?.to_owned(),
                            name: parts.next()?.to_owned(),
                        })
                    })
                    .collect())
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if is_empty_session_state(stderr.as_ref()) {
                    Ok(Vec::new())
                } else {
                    bail!("tmux list-windows failed: {}", stderr.trim())
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                bail!("tmux is not installed or not on PATH")
            }
            Err(error) => Err(error).context("failed to execute tmux list-windows"),
        }
    }

    /// Session that owns a window id.
    pub fn window_session(&self, window_id: &str) -> Result<String> {
        let output = self.run_tmux_capture([
            "display-message",
            "-p",
            "-t",
            window_id,
            "#{session_name}",
        ])?;

        if !output.status.success {
            bail!("tmux window not found: {window_id}");
        }
        let stdout = String::from_utf8(output.stdout).context("tmux output was not utf-8")?;
        let session = stdout.trim();
        if session.is_empty() {
            bail!("tmux window not found: {window_id}");
        }
        Ok(session.to_owned())
    }

    pub fn select_window_by_id(&self, window_id: &str) -> Result<()> {
        self.run_tmux(["select-window", "-t", window_id])
            .context("failed to execute tmux select-window")
    }

    pub fn kill_window(&self, window_id: &str) -> Result<()> {
        self.run_tmux(["kill-window", "-t", window_id])
            .context("failed to execute tmux kill-window")
    }

    pub fn rename_window(&self, window_id: &str, new_name: &str) -> Result<()> {
        self.run_tmux(["rename-window", "-t", window_id, new_name])
            .context("failed to execute tmux rename-window")
    }

    /// Window id of the active window in the current session, inside tmux.
    pub fn current_window_id(&self) -> Result<Option<String>> {
        if !util::inside_tmux() {
            return Ok(None);
        }

        let output = self
            .runner
            .run_capture(
                "tmux",
                &[
                    "display-message".to_owned(),
                    "-p".to_owned(),
                    "#{window_id}".to_owned(),
                ],
            )
            .context("failed to execute tmux display-message")?;

        if !output.status.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tmux display-message failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8(output.stdout).context("tmux output was not utf-8")?;
        let id = stdout.trim();
        if id.is_empty() {
            Ok(None)
        } else {
            Ok(Some(id.to_owned()))
        }
    }

    /// Switch to (or attach) the most recently used session other than the
    /// current one. Inside tmux this is tmux's own "last" client target; outside
    /// tmux it is the most recently active session.
    pub fn switch_to_last(&self) -> Result<()> {
        if util::inside_tmux() {
            return self
                .run_tmux(["switch-client", "-l"])
                .context("failed to execute tmux switch-client -l");
        }

        let session = self
            .list_sessions()?
            .into_iter()
            .next()
            .context("no tmux sessions to switch to")?;
        self.switch_or_attach(&session)
    }

    pub fn capture_session(&self, session: &str) -> Result<SessionSnapshot> {
        self.ensure_session_exists(session)?;

        let windows = self.list_windows(session)?;
        let active_window_name = windows
            .iter()
            .find(|window| window.active)
            .or_else(|| windows.first())
            .context("tmux session did not contain any windows")?;
        let active_window_name = active_window_name.name.clone();

        let mut captured_windows = Vec::with_capacity(windows.len());
        let mut active_pane = None;
        let mut active_path = None;

        for window in windows {
            let synchronize = self.window_synchronize(&window.id)?;
            let panes = self.list_pane_records(&window.id)?;
            let panes = infer_pane_layouts(panes);

            if window.active {
                let active = panes
                    .iter()
                    .enumerate()
                    .find(|(_, pane)| pane.active)
                    .or_else(|| panes.first().map(|pane| (0, pane)))
                    .context("active tmux window did not contain any panes")?;
                active_pane = Some(active.0);
                active_path = Some(active.1.cwd.clone());
            }

            captured_windows.push(WindowSnapshot {
                name: window.name,
                synchronize,
                active: window.active,
                panes,
            });
        }

        let active_path =
            active_path.context("could not determine the active pane path for the tmux session")?;

        Ok(SessionSnapshot {
            session_name: session.to_owned(),
            active_window: active_window_name,
            active_pane: active_pane.unwrap_or(0),
            active_path,
            windows: captured_windows,
        })
    }

    fn create_session_with_window(
        &self,
        session: &str,
        window: &str,
        directory: &Path,
        env: &[(String, String)],
    ) -> Result<()> {
        let directory = util::path_to_string(directory)?;
        let mut args = vec![
            "new-session".to_owned(),
            "-d".to_owned(),
            "-s".to_owned(),
            session.to_owned(),
            "-c".to_owned(),
            directory,
            "-n".to_owned(),
            window.to_owned(),
        ];
        // `-e` needs tmux >= 3.2; only emitted when a template or project
        // actually configures env, so older tmux keeps working otherwise.
        for (key, value) in env {
            args.push("-e".to_owned());
            args.push(format!("{key}={value}"));
        }
        self.run_tmux_owned(args)
            .context("failed to execute tmux new-session")
    }

    /// Run a session lifecycle hook on the host: `sh -c <command>` in the
    /// session root with the session env applied, blocking until it finishes.
    pub fn run_session_hook(
        &self,
        command: &str,
        cwd: &Path,
        env: &[(String, String)],
    ) -> Result<()> {
        let args = vec!["-c".to_owned(), command.to_owned()];
        let output = self
            .runner
            .run_capture_in("sh", &args, cwd, env)
            .context("failed to execute on_create hook")?;
        if output.status.success {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            bail!(
                "on_create hook failed with {}: {command}",
                util::exit_status_label(output.status.code)
            );
        }
        bail!("on_create hook failed: {stderr}");
    }

    fn new_window_after(
        &self,
        session: &str,
        after_window: &str,
        window: &str,
        directory: &Path,
    ) -> Result<()> {
        let directory = util::path_to_string(directory)?;
        let target = format!("{session}:{after_window}");
        self.run_tmux([
            "new-window",
            "-a",
            "-t",
            &target,
            "-n",
            window,
            "-c",
            &directory,
        ])
        .context("failed to execute tmux new-window")
    }

    fn send_keys_to_target(&self, target: &str, command: &str) -> Result<()> {
        // Send the command text literally (-l) so a command that happens to
        // match a tmux key name (e.g. `Up`, `Enter`, `C-c`) is typed rather
        // than interpreted, then submit it with a separate Enter keypress.
        self.run_tmux(["send-keys", "-t", target, "-l", command])
            .context("failed to execute tmux send-keys")?;
        self.run_tmux(["send-keys", "-t", target, "Enter"])
            .context("failed to execute tmux send-keys")
    }

    fn split_window(&self, target: &str, layout: &PaneLayout, directory: &Path) -> Result<String> {
        let directory = util::path_to_string(directory)?;
        let mut args = vec![
            "split-window".to_owned(),
            "-t".to_owned(),
            target.to_owned(),
            "-P".to_owned(),
            "-F".to_owned(),
            "#{pane_id}".to_owned(),
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

        let output = self
            .runner
            .run_capture("tmux", &args)
            .context("failed to execute tmux split-window")?;

        if !output.status.success {
            bail!("{}", tmux_failure_message("split-window", &output));
        }

        let pane_id =
            String::from_utf8(output.stdout).context("tmux split-window output was not utf-8")?;
        Ok(pane_id.trim().to_owned())
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

    fn select_pane_target(&self, target: &str) -> Result<()> {
        self.run_tmux(["select-pane", "-t", target])
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

    fn zoom_pane(&self, target: &str) -> Result<()> {
        self.run_tmux(["resize-pane", "-Z", "-t", target])
            .context("failed to execute tmux resize-pane -Z")
    }

    fn configure_panes(
        &self,
        session: &str,
        window: &str,
        plan: &crate::templates::WindowPlan,
    ) -> Result<()> {
        let target = format!("{session}:{window}");
        let pane_ids = self.list_panes(&target)?;
        let first_pane_target = pane_ids
            .first()
            .cloned()
            .context("tmux window did not contain an initial pane")?;
        let mut zoom_target = if plan.panes.is_empty() {
            None
        } else if plan.panes[0].zoom {
            Some(first_pane_target.clone())
        } else {
            None
        };

        if plan.panes.is_empty() {
            if let Some(pre_command) = &plan.pre_command {
                self.send_keys_to_target(&first_pane_target, pre_command)?;
            }
            if let Some(command) = &plan.command {
                self.send_keys_to_target(&first_pane_target, command)?;
            }
            if plan.synchronize {
                self.set_synchronize_panes(session, window, true)?;
            }
            if let Some(target) = zoom_target.as_deref() {
                self.zoom_pane(target)?;
            }
            return Ok(());
        }

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
            let pane_target = self.split_window(&target, layout, &pane.cwd)?;
            if let Some(pre_command) = &plan.pre_command {
                self.send_keys_to_target(&pane_target, pre_command)
                    .context("failed to execute tmux send-keys for split pane pre_command")?;
            }
            if let Some(command) = &pane.command {
                self.send_keys_to_target(&pane_target, command)
                    .context("failed to execute tmux send-keys for split pane")?;
            }
            if pane.zoom {
                zoom_target = Some(pane_target.clone());
            }
        }

        if let Some(layout) = &plan.layout {
            self.select_layout(&target, layout)?;
        }

        if plan.synchronize {
            self.set_synchronize_panes(session, window, true)?;
        }

        if let Some(target) = zoom_target.as_deref() {
            self.zoom_pane(target)?;
        }

        Ok(())
    }

    fn list_panes(&self, target: &str) -> Result<Vec<String>> {
        let output = self
            .run_tmux_capture(["list-panes", "-t", target, "-F", "#{pane_id}"])
            .context("failed to execute tmux list-panes")?;

        if !output.status.success {
            bail!("{}", tmux_failure_message("list-panes", &output));
        }

        let stdout =
            String::from_utf8(output.stdout).context("tmux list-panes output was not utf-8")?;
        Ok(stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect())
    }

    fn select_pane_by_offset(&self, session: &str, window: &str, pane_offset: usize) -> Result<()> {
        let target = format!("{session}:{window}");
        let panes = self.list_panes(&target)?;
        let pane = panes.get(pane_offset).with_context(|| {
            format!(
                "startup pane offset {} was not found in window {}",
                pane_offset, target
            )
        })?;
        self.select_pane_target(pane)
    }

    fn run_tmux<const N: usize>(&self, args: [&str; N]) -> Result<()> {
        self.run_tmux_owned(args.into_iter().map(ToOwned::to_owned).collect())
    }

    fn run_tmux_owned(&self, args: Vec<String>) -> Result<()> {
        let subcommand = args.first().cloned().unwrap_or_else(|| "tmux".to_owned());
        let output = self.runner.run_capture("tmux", &args).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!("tmux is not installed or not on PATH")
            } else {
                anyhow::Error::new(error).context("failed to execute tmux")
            }
        })?;

        if output.status.success {
            Ok(())
        } else {
            bail!("{}", tmux_failure_message(&subcommand, &output))
        }
    }

    fn run_tmux_capture<const N: usize>(&self, args: [&str; N]) -> Result<CommandOutput> {
        let args = args.into_iter().map(ToOwned::to_owned).collect::<Vec<_>>();
        self.runner.run_capture("tmux", &args).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!("tmux is not installed or not on PATH")
            } else {
                anyhow::Error::new(error).context("failed to execute tmux")
            }
        })
    }

    fn list_windows(&self, session: &str) -> Result<Vec<WindowRecord>> {
        let output = self
            .run_tmux_capture([
                "list-windows",
                "-t",
                session,
                "-F",
                "#{window_id}\t#{window_name}\t#{window_active}",
            ])
            .context("failed to execute tmux list-windows")?;

        if !output.status.success {
            bail!("{}", tmux_failure_message("list-windows", &output));
        }

        let stdout =
            String::from_utf8(output.stdout).context("tmux list-windows output was not utf-8")?;
        stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(parse_window_record)
            .collect()
    }

    fn window_synchronize(&self, window_id: &str) -> Result<bool> {
        let output = self
            .run_tmux_capture([
                "show-window-options",
                "-t",
                window_id,
                "-v",
                "synchronize-panes",
            ])
            .context("failed to execute tmux show-window-options")?;

        if !output.status.success {
            bail!("{}", tmux_failure_message("show-window-options", &output));
        }

        let stdout = String::from_utf8(output.stdout)
            .context("tmux show-window-options output was not utf-8")?;
        Ok(stdout.trim() == "on")
    }

    fn list_pane_records(&self, window_id: &str) -> Result<Vec<PaneRecord>> {
        let output = self
            .run_tmux_capture([
                "list-panes",
                "-t",
                window_id,
                "-F",
                "#{pane_index}\t#{pane_current_path}\t#{pane_active}\t#{pane_left}\t#{pane_top}\t#{pane_width}\t#{pane_height}",
            ])
            .context("failed to execute tmux list-panes")?;

        if !output.status.success {
            bail!("{}", tmux_failure_message("list-panes", &output));
        }

        let stdout =
            String::from_utf8(output.stdout).context("tmux list-panes output was not utf-8")?;
        let mut panes = stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(parse_pane_record)
            .collect::<Result<Vec<_>>>()?;
        panes.sort_by_key(|pane| pane.index);
        Ok(panes)
    }
}

fn is_empty_session_state(stderr: &str) -> bool {
    let stderr = stderr.trim();

    stderr.contains("no server running")
        || stderr.contains("failed to connect to server")
        || (stderr.contains("error connecting to") && stderr.contains("No such file or directory"))
}

fn parse_window_record(line: &str) -> Result<WindowRecord> {
    let mut parts = line.splitn(3, '\t');
    let id = parts.next().context("missing tmux window id")?.to_owned();
    let name = parts.next().context("missing tmux window name")?.to_owned();
    let active = match parts.next().context("missing tmux window active flag")? {
        "1" => true,
        "0" => false,
        other => bail!("invalid tmux window active flag: {other}"),
    };

    Ok(WindowRecord { id, name, active })
}

fn parse_pane_record(line: &str) -> Result<PaneRecord> {
    let mut parts = line.splitn(7, '\t');
    let index = parts
        .next()
        .context("missing tmux pane index")?
        .parse()
        .context("tmux pane index was not a number")?;
    let cwd = std::path::PathBuf::from(parts.next().context("missing tmux pane cwd")?);
    let active = match parts.next().context("missing tmux pane active flag")? {
        "1" => true,
        "0" => false,
        other => bail!("invalid tmux pane active flag: {other}"),
    };
    let left = parts
        .next()
        .context("missing tmux pane left coordinate")?
        .parse()
        .context("tmux pane left was not a number")?;
    let top = parts
        .next()
        .context("missing tmux pane top coordinate")?
        .parse()
        .context("tmux pane top was not a number")?;
    let width = parts
        .next()
        .context("missing tmux pane width")?
        .parse()
        .context("tmux pane width was not a number")?;
    let height = parts
        .next()
        .context("missing tmux pane height")?
        .parse()
        .context("tmux pane height was not a number")?;

    Ok(PaneRecord {
        index,
        cwd,
        active,
        left,
        top,
        width,
        height,
    })
}

/// Parse a `#{session_activity}\t#{session_attached}\t#{session_name}` line.
/// Lines without a name are dropped; unparsable activity/attached default to 0.
fn parse_session_listing(line: &str) -> Option<SessionListing> {
    let mut parts = line.splitn(3, '\t');
    let activity = parts.next()?.trim().parse().unwrap_or(0);
    let attached = parts.next()?.trim().parse::<u32>().unwrap_or(0) > 0;
    let name = parts.next()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(SessionListing {
        activity,
        attached,
        name: name.to_owned(),
    })
}

/// Build a failure message for a tmux subcommand, falling back to the exit code
/// when tmux produced no stderr (so the error is never an empty string).
fn tmux_failure_message(subcommand: &str, output: &CommandOutput) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        format!(
            "tmux {subcommand} failed with {}",
            util::exit_status_label(output.status.code)
        )
    } else {
        format!("tmux {subcommand} failed: {stderr}")
    }
}

fn infer_pane_layouts(panes: Vec<PaneRecord>) -> Vec<PaneSnapshot> {
    panes
        .iter()
        .enumerate()
        .map(|(index, pane)| PaneSnapshot {
            cwd: pane.cwd.clone(),
            active: pane.active,
            layout: index
                .checked_sub(1)
                .map(|previous| infer_pane_layout(pane, &panes[previous])),
        })
        .collect()
}

/// Infer the split that produced `pane` relative to the pane it was split from.
/// `smux` creates each extra pane by splitting the previously created one, so
/// the inverse is a comparison against the previous pane's geometry: panes that
/// share a top edge sit side by side (a horizontal split), otherwise they are
/// stacked. The recovered size is the new pane's extent along the split axis in
/// cells, which `split-window -l` reproduces.
fn infer_pane_layout(pane: &PaneRecord, previous: &PaneRecord) -> PaneLayout {
    let position = if pane.top == previous.top {
        if pane.left >= previous.left {
            PanePosition::Right
        } else {
            PanePosition::Left
        }
    } else if pane.top > previous.top {
        PanePosition::Bottom
    } else {
        PanePosition::Top
    };

    let extent = match position {
        PanePosition::Right | PanePosition::Left => pane.width,
        PanePosition::Bottom | PanePosition::Top => pane.height,
    };
    let size = (extent > 0).then(|| extent.to_string());

    PaneLayout { position, size }
}

fn initial_pane_cwd(window: &crate::templates::WindowPlan) -> &Path {
    window
        .panes
        .first()
        .map(|pane| pane.cwd.as_path())
        .unwrap_or(window.cwd.as_path())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::process::{CommandOutput, CommandStatus, FakeCommandRunner, IoMode};
    use crate::templates::{PaneLayout, PanePlan, PanePosition, SessionPlan, WindowPlan};

    use super::{PaneRecord, Tmux, infer_pane_layout};

    fn pane_record(left: i32, top: i32, width: i32, height: i32) -> PaneRecord {
        PaneRecord {
            index: 0,
            cwd: std::path::PathBuf::from("/tmp"),
            active: false,
            left,
            top,
            width,
            height,
        }
    }

    #[test]
    fn infers_split_position_and_size_from_geometry() {
        let base = pane_record(0, 0, 100, 40);

        // Side-by-side panes share a top edge: a horizontal split.
        let right = infer_pane_layout(&pane_record(50, 0, 50, 40), &base);
        assert_eq!(right.position, PanePosition::Right);
        assert_eq!(right.size.as_deref(), Some("50"));

        let left = infer_pane_layout(&base, &pane_record(50, 0, 50, 40));
        assert_eq!(left.position, PanePosition::Left);

        // Stacked panes differ in top: a vertical split, sized by height.
        let bottom = infer_pane_layout(&pane_record(0, 20, 100, 20), &base);
        assert_eq!(bottom.position, PanePosition::Bottom);
        assert_eq!(bottom.size.as_deref(), Some("20"));

        let top = infer_pane_layout(&base, &pane_record(0, 20, 100, 20));
        assert_eq!(top.position, PanePosition::Top);
    }

    #[test]
    fn outside_tmux_uses_inherited_stdio_for_attach() {
        let _guard = crate::util::test_env::lock();
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
        assert_eq!(recorded[0].args, vec!["attach-session", "-t", "=demo"]);
        assert_eq!(recorded[0].io_mode, IoMode::Inherit);
    }

    #[test]
    fn list_sessions_returns_empty_when_tmux_server_is_not_running() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: false,
                code: Some(1),
            },
            stdout: Vec::new(),
            stderr: b"no server running on /tmp/tmux-1000/default\n".to_vec(),
        }));

        let tmux = Tmux::with_runner(runner);
        assert_eq!(
            tmux.list_sessions().expect("query should succeed"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn list_sessions_returns_empty_when_tmux_socket_is_missing() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: false,
                code: Some(1),
            },
            stdout: Vec::new(),
            stderr: b"error connecting to /tmp/tmux-1000/default (No such file or directory)\n"
                .to_vec(),
        }));

        let tmux = Tmux::with_runner(runner);
        assert_eq!(
            tmux.list_sessions().expect("query should succeed"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn list_sessions_orders_by_most_recent_activity() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(
            b"100\t1\talpha\n300\t0\tbeta\n200\t0\tgamma\n".to_vec(),
        ));

        let tmux = Tmux::with_runner(runner);
        assert_eq!(
            tmux.list_sessions().expect("query should succeed"),
            vec!["beta", "gamma", "alpha"]
        );
    }

    #[test]
    fn list_detached_sessions_excludes_attached() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(
            b"100\t1\talpha\n300\t0\tbeta\n200\t0\tgamma\n".to_vec(),
        ));

        let tmux = Tmux::with_runner(runner);
        assert_eq!(
            tmux.list_detached_sessions().expect("query should succeed"),
            vec!["beta", "gamma"]
        );
    }

    #[test]
    fn outside_tmux_has_no_current_session() {
        let _guard = crate::util::test_env::lock();
        let runner = Arc::new(FakeCommandRunner::new());

        unsafe {
            std::env::remove_var("TMUX");
        }

        let tmux = Tmux::with_runner(runner.clone());
        assert_eq!(tmux.current_session().expect("query should succeed"), None);
        assert!(runner.recorded().is_empty());
    }

    #[test]
    fn inside_tmux_uses_switch_client_with_captured_io() {
        let _guard = crate::util::test_env::lock();
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
        assert_eq!(recorded[0].args, vec!["switch-client", "-t", "=demo"]);
        assert_eq!(recorded[0].io_mode, IoMode::Capture);

        unsafe {
            std::env::remove_var("TMUX");
        }
    }

    #[test]
    fn inside_tmux_reads_current_session() {
        let _guard = crate::util::test_env::lock();
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"demo\n".to_vec(),
            stderr: Vec::new(),
        }));

        unsafe {
            std::env::set_var("TMUX", "/tmp/tmux-test,123,0");
        }

        let tmux = Tmux::with_runner(runner.clone());
        assert_eq!(
            tmux.current_session()
                .expect("query should succeed")
                .as_deref(),
            Some("demo")
        );

        let recorded = runner.recorded();
        assert_eq!(recorded.len(), 1);
        assert_eq!(
            recorded[0].args,
            vec!["display-message", "-p", "#{session_name}"]
        );

        unsafe {
            std::env::remove_var("TMUX");
        }
    }

    #[test]
    fn session_plan_emits_expected_tmux_commands() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new())); // 0 new-session
        runner.push_capture(ok_capture(b"%1\n".to_vec())); // 1 list-panes editor
        runner.push_capture(ok_capture(Vec::new())); // 2 send-keys -l pre_command
        runner.push_capture(ok_capture(Vec::new())); // 3 send-keys Enter
        runner.push_capture(ok_capture(Vec::new())); // 4 send-keys -l nvim
        runner.push_capture(ok_capture(Vec::new())); // 5 send-keys Enter
        runner.push_capture(ok_capture(Vec::new())); // 6 new-window run
        runner.push_capture(ok_capture(b"%2\n".to_vec())); // 7 list-panes run
        runner.push_capture(ok_capture(Vec::new())); // 8 send-keys -l pre_command
        runner.push_capture(ok_capture(Vec::new())); // 9 send-keys Enter
        runner.push_capture(ok_capture(Vec::new())); // 10 send-keys -l cargo run
        runner.push_capture(ok_capture(Vec::new())); // 11 send-keys Enter
        runner.push_capture(ok_capture(b"%3\n".to_vec())); // 12 split-window
        runner.push_capture(ok_capture(Vec::new())); // 13 send-keys -l pre_command
        runner.push_capture(ok_capture(Vec::new())); // 14 send-keys Enter
        runner.push_capture(ok_capture(Vec::new())); // 15 send-keys -l cargo test
        runner.push_capture(ok_capture(Vec::new())); // 16 send-keys Enter
        runner.push_capture(ok_capture(Vec::new())); // 17 select-layout
        runner.push_capture(ok_capture(Vec::new())); // 18 set-window-option
        runner.push_capture(ok_capture(Vec::new())); // 19 select-window
        runner.push_capture(ok_capture(b"%1\n".to_vec())); // 20 list-panes editor
        runner.push_capture(ok_capture(Vec::new())); // 21 select-pane

        let tmux = Tmux::with_runner(runner.clone());
        let plan = SessionPlan {
            root: PathBuf::from("/tmp/demo"),
            env: Vec::new(),
            on_create: None,
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
                            zoom: false,
                        },
                        PanePlan {
                            layout: Some(PaneLayout {
                                position: PanePosition::Right,
                                size: None,
                            }),
                            cwd: "/tmp/demo".into(),
                            command: Some("cargo test".to_owned()),
                            zoom: false,
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
            vec!["list-panes", "-t", "demo:editor", "-F", "#{pane_id}"]
        );
        assert_eq!(
            recorded[2].args,
            vec!["send-keys", "-t", "%1", "-l", "source .venv/bin/activate"]
        );
        assert_eq!(recorded[3].args, vec!["send-keys", "-t", "%1", "Enter"]);
        assert_eq!(
            recorded[4].args,
            vec!["send-keys", "-t", "%1", "-l", "nvim"]
        );
        assert_eq!(recorded[5].args, vec!["send-keys", "-t", "%1", "Enter"]);
        assert_eq!(
            recorded[6].args,
            vec![
                "new-window",
                "-a",
                "-t",
                "demo:editor",
                "-n",
                "run",
                "-c",
                "/tmp/demo"
            ]
        );
        assert_eq!(
            recorded[7].args,
            vec!["list-panes", "-t", "demo:run", "-F", "#{pane_id}"]
        );
        assert_eq!(
            recorded[8].args,
            vec!["send-keys", "-t", "%2", "-l", "source .venv/bin/activate"]
        );
        assert_eq!(recorded[9].args, vec!["send-keys", "-t", "%2", "Enter"]);
        assert_eq!(
            recorded[10].args,
            vec!["send-keys", "-t", "%2", "-l", "cargo run"]
        );
        assert_eq!(recorded[11].args, vec!["send-keys", "-t", "%2", "Enter"]);
        assert_eq!(
            recorded[12].args,
            vec![
                "split-window",
                "-t",
                "demo:run",
                "-P",
                "-F",
                "#{pane_id}",
                "-h",
                "-c",
                "/tmp/demo"
            ]
        );
        assert_eq!(
            recorded[13].args,
            vec!["send-keys", "-t", "%3", "-l", "source .venv/bin/activate"]
        );
        assert_eq!(recorded[14].args, vec!["send-keys", "-t", "%3", "Enter"]);
        assert_eq!(
            recorded[15].args,
            vec!["send-keys", "-t", "%3", "-l", "cargo test"]
        );
        assert_eq!(recorded[16].args, vec!["send-keys", "-t", "%3", "Enter"]);
        assert_eq!(
            recorded[17].args,
            vec!["select-layout", "-t", "demo:run", "main-horizontal"]
        );
        assert_eq!(
            recorded[18].args,
            vec![
                "set-window-option",
                "-t",
                "demo:run",
                "synchronize-panes",
                "on"
            ]
        );
        assert_eq!(
            recorded[19].args,
            vec!["select-window", "-t", "demo:editor"]
        );
        assert_eq!(
            recorded[20].args,
            vec!["list-panes", "-t", "demo:editor", "-F", "#{pane_id}"]
        );
        assert_eq!(recorded[21].args, vec!["select-pane", "-t", "%1"]);
    }

    #[test]
    fn failed_session_setup_removes_the_incomplete_session() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new())); // new-session
        runner.push_capture(ok_capture(b"%1\n".to_vec())); // list-panes
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: false,
                code: Some(1),
            },
            stdout: Vec::new(),
            stderr: b"pane disappeared".to_vec(),
        })); // send-keys
        runner.push_capture(ok_capture(Vec::new())); // rollback kill-session

        let tmux = Tmux::with_runner(runner.clone());
        let plan = SessionPlan {
            root: PathBuf::from("/tmp/demo"),
            env: Vec::new(),
            on_create: None,
            session_name: "demo".to_owned(),
            startup_window: "main".to_owned(),
            startup_pane: 0,
            windows: vec![WindowPlan {
                name: "main".to_owned(),
                cwd: "/tmp/demo".into(),
                pre_command: None,
                command: Some("nvim".to_owned()),
                layout: None,
                synchronize: false,
                panes: Vec::new(),
            }],
        };

        let error = tmux
            .create_session_from_plan(&plan)
            .expect_err("session setup should fail");
        assert!(error.to_string().contains("removed the incomplete session"));

        let recorded = runner.recorded();
        assert_eq!(
            recorded.last().expect("cleanup command").args,
            ["kill-session", "-t", "=demo"]
        );
    }

    #[test]
    fn first_pane_cwd_is_used_when_creating_window() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new())); // new-session
        runner.push_capture(ok_capture(b"%1\n".to_vec())); // list-panes
        runner.push_capture(ok_capture(Vec::new())); // send-keys -l nvim
        runner.push_capture(ok_capture(Vec::new())); // send-keys Enter
        runner.push_capture(ok_capture(b"%2\n".to_vec())); // split-window
        runner.push_capture(ok_capture(Vec::new())); // send-keys -l cargo run
        runner.push_capture(ok_capture(Vec::new())); // send-keys Enter
        runner.push_capture(ok_capture(Vec::new())); // select-window
        runner.push_capture(ok_capture(b"%1\n%2\n".to_vec())); // list-panes
        runner.push_capture(ok_capture(Vec::new())); // select-pane

        let tmux = Tmux::with_runner(runner.clone());
        let plan = SessionPlan {
            root: PathBuf::from("/tmp/demo"),
            env: Vec::new(),
            on_create: None,
            session_name: "demo".to_owned(),
            startup_window: "main".to_owned(),
            startup_pane: 0,
            windows: vec![WindowPlan {
                name: "main".to_owned(),
                cwd: "/tmp/demo".into(),
                pre_command: None,
                command: None,
                layout: None,
                synchronize: false,
                panes: vec![
                    PanePlan {
                        layout: None,
                        cwd: "/tmp/demo/app".into(),
                        command: Some("nvim".to_owned()),
                        zoom: false,
                    },
                    PanePlan {
                        layout: Some(PaneLayout {
                            position: PanePosition::Right,
                            size: None,
                        }),
                        cwd: "/tmp/demo/server".into(),
                        command: Some("cargo run".to_owned()),
                        zoom: false,
                    },
                ],
            }],
        };

        tmux.create_session_from_plan(&plan)
            .expect("session plan should succeed");

        let recorded = runner.recorded();
        assert_eq!(
            recorded[0].args,
            vec![
                "new-session",
                "-d",
                "-s",
                "demo",
                "-c",
                "/tmp/demo/app",
                "-n",
                "main"
            ]
        );
        assert_eq!(
            recorded[4].args,
            vec![
                "split-window",
                "-t",
                "demo:main",
                "-P",
                "-F",
                "#{pane_id}",
                "-h",
                "-c",
                "/tmp/demo/server"
            ]
        );
    }

    #[test]
    fn kill_session_uses_captured_tmux_command() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new()));

        let tmux = Tmux::with_runner(runner.clone());
        tmux.kill_session("demo")
            .expect("kill-session should succeed");

        let recorded = runner.recorded();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].program, "tmux");
        assert_eq!(recorded[0].args, vec!["kill-session", "-t", "=demo"]);
        assert_eq!(recorded[0].io_mode, IoMode::Capture);
    }

    #[test]
    fn capture_session_reads_windows_and_panes() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"@1\teditor\t1\n@2\trun\t0\n".to_vec()));
        runner.push_capture(ok_capture(b"off\n".to_vec()));
        runner.push_capture(ok_capture(
            b"0\t/tmp/demo\t1\t0\t0\t100\t40\n1\t/tmp/demo/server\t0\t50\t0\t50\t40\n".to_vec(),
        ));
        runner.push_capture(ok_capture(b"on\n".to_vec()));
        runner.push_capture(ok_capture(b"0\t/tmp/demo\t1\t0\t0\t100\t40\n".to_vec()));

        let tmux = Tmux::with_runner(runner);
        let snapshot = tmux
            .capture_session("demo")
            .expect("capture should succeed");

        assert_eq!(snapshot.session_name, "demo");
        assert_eq!(snapshot.active_window, "editor");
        assert_eq!(snapshot.active_pane, 0);
        assert_eq!(snapshot.active_path, std::path::PathBuf::from("/tmp/demo"));
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].name, "editor");
        assert!(!snapshot.windows[0].synchronize);
        assert_eq!(snapshot.windows[0].panes.len(), 2);
        assert_eq!(
            snapshot.windows[0].panes[1].layout,
            Some(PaneLayout {
                position: PanePosition::Right,
                size: Some("50".to_owned()),
            })
        );
        assert!(snapshot.windows[1].synchronize);
    }

    #[test]
    fn startup_pane_uses_zero_based_offset_not_tmux_base_index() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new())); // 0 new-session
        runner.push_capture(ok_capture(b"%10\n".to_vec())); // 1 list-panes
        runner.push_capture(ok_capture(Vec::new())); // 2 send-keys -l shell
        runner.push_capture(ok_capture(Vec::new())); // 3 send-keys Enter
        runner.push_capture(ok_capture(b"%11\n".to_vec())); // 4 split-window
        runner.push_capture(ok_capture(Vec::new())); // 5 send-keys -l tests
        runner.push_capture(ok_capture(Vec::new())); // 6 send-keys Enter
        runner.push_capture(ok_capture(Vec::new())); // 7 select-window
        runner.push_capture(ok_capture(b"%10\n%11\n".to_vec())); // 8 list-panes
        runner.push_capture(ok_capture(Vec::new())); // 9 select-pane

        let tmux = Tmux::with_runner(runner.clone());
        let plan = SessionPlan {
            root: PathBuf::from("/tmp/demo"),
            env: Vec::new(),
            on_create: None,
            session_name: "demo".to_owned(),
            startup_window: "main".to_owned(),
            startup_pane: 1,
            windows: vec![WindowPlan {
                name: "main".to_owned(),
                cwd: "/tmp/demo".into(),
                pre_command: None,
                command: None,
                layout: None,
                synchronize: false,
                panes: vec![
                    PanePlan {
                        layout: None,
                        cwd: "/tmp/demo".into(),
                        command: Some("shell".to_owned()),
                        zoom: false,
                    },
                    PanePlan {
                        layout: Some(PaneLayout {
                            position: PanePosition::Right,
                            size: None,
                        }),
                        cwd: "/tmp/demo".into(),
                        command: Some("tests".to_owned()),
                        zoom: false,
                    },
                ],
            }],
        };

        tmux.create_session_from_plan(&plan)
            .expect("session plan should succeed");

        let recorded = runner.recorded();
        assert_eq!(
            recorded[8].args,
            vec!["list-panes", "-t", "demo:main", "-F", "#{pane_id}"]
        );
        assert_eq!(recorded[9].args, vec!["select-pane", "-t", "%11"]);
    }

    #[test]
    fn zoomed_pane_emits_resize_pane_command() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new())); // 0 new-session
        runner.push_capture(ok_capture(b"%20\n".to_vec())); // 1 list-panes
        runner.push_capture(ok_capture(Vec::new())); // 2 send-keys -l shell
        runner.push_capture(ok_capture(Vec::new())); // 3 send-keys Enter
        runner.push_capture(ok_capture(b"%21\n".to_vec())); // 4 split-window
        runner.push_capture(ok_capture(Vec::new())); // 5 send-keys -l tests
        runner.push_capture(ok_capture(Vec::new())); // 6 send-keys Enter
        runner.push_capture(ok_capture(Vec::new())); // 7 resize-pane (zoom)
        runner.push_capture(ok_capture(Vec::new())); // 8 select-window
        runner.push_capture(ok_capture(b"%20\n%21\n".to_vec())); // 9 list-panes
        runner.push_capture(ok_capture(Vec::new())); // 10 select-pane

        let tmux = Tmux::with_runner(runner.clone());
        let plan = SessionPlan {
            root: PathBuf::from("/tmp/demo"),
            env: Vec::new(),
            on_create: None,
            session_name: "demo".to_owned(),
            startup_window: "main".to_owned(),
            startup_pane: 0,
            windows: vec![WindowPlan {
                name: "main".to_owned(),
                cwd: "/tmp/demo".into(),
                pre_command: None,
                command: None,
                layout: None,
                synchronize: false,
                panes: vec![
                    PanePlan {
                        layout: None,
                        cwd: "/tmp/demo".into(),
                        command: Some("shell".to_owned()),
                        zoom: false,
                    },
                    PanePlan {
                        layout: Some(PaneLayout {
                            position: PanePosition::Right,
                            size: None,
                        }),
                        cwd: "/tmp/demo".into(),
                        command: Some("tests".to_owned()),
                        zoom: true,
                    },
                ],
            }],
        };

        tmux.create_session_from_plan(&plan)
            .expect("session plan should succeed");

        let recorded = runner.recorded();
        assert_eq!(recorded[7].args, vec!["resize-pane", "-Z", "-t", "%21"]);
    }

    fn ok_capture(stdout: Vec<u8>) -> std::io::Result<CommandOutput> {
        Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout,
            stderr: Vec::new(),
        })
    }

    #[test]
    fn lists_all_windows_across_sessions() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(b"@1\tdemo\teditor\n@2\tapi\tserver\n".to_vec()));
        let tmux = Tmux::with_runner(runner.clone());

        let windows = tmux.list_all_windows().expect("listing should succeed");
        assert_eq!(
            windows,
            vec![
                super::GlobalWindow {
                    id: "@1".to_owned(),
                    session: "demo".to_owned(),
                    name: "editor".to_owned(),
                },
                super::GlobalWindow {
                    id: "@2".to_owned(),
                    session: "api".to_owned(),
                    name: "server".to_owned(),
                },
            ]
        );
        assert_eq!(
            runner.recorded()[0].args,
            vec![
                "list-windows",
                "-a",
                "-F",
                "#{window_id}\t#{session_name}\t#{window_name}",
            ]
        );
    }

    #[test]
    fn window_operations_target_window_ids() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(b"demo\n".to_vec())); // display-message
        runner.push_capture(ok_capture(Vec::new())); // select-window
        runner.push_capture(ok_capture(Vec::new())); // kill-window
        runner.push_capture(ok_capture(Vec::new())); // rename-window
        let tmux = Tmux::with_runner(runner.clone());

        assert_eq!(
            tmux.window_session("@7").expect("lookup should succeed"),
            "demo"
        );
        tmux.select_window_by_id("@7").expect("select should work");
        tmux.kill_window("@7").expect("kill should work");
        tmux.rename_window("@7", "logs").expect("rename should work");

        let recorded = runner.recorded();
        assert_eq!(
            recorded[0].args,
            vec!["display-message", "-p", "-t", "@7", "#{session_name}"]
        );
        assert_eq!(recorded[1].args, vec!["select-window", "-t", "@7"]);
        assert_eq!(recorded[2].args, vec!["kill-window", "-t", "@7"]);
        assert_eq!(recorded[3].args, vec!["rename-window", "-t", "@7", "logs"]);
    }

    #[test]
    fn new_session_passes_env_flags() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new()));
        let tmux = Tmux::with_runner(runner.clone());

        tmux.create_session_with_window(
            "demo",
            "main",
            std::path::Path::new("/tmp/demo"),
            &[
                ("AWS_PROFILE".to_owned(), "dev".to_owned()),
                ("DATABASE_URL".to_owned(), "postgres://x".to_owned()),
            ],
        )
        .expect("new-session should succeed");

        assert_eq!(
            runner.recorded()[0].args,
            vec![
                "new-session",
                "-d",
                "-s",
                "demo",
                "-c",
                "/tmp/demo",
                "-n",
                "main",
                "-e",
                "AWS_PROFILE=dev",
                "-e",
                "DATABASE_URL=postgres://x",
            ]
        );
    }

    #[test]
    fn session_hook_runs_shell_in_root_with_env() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new()));
        let tmux = Tmux::with_runner(runner.clone());

        tmux.run_session_hook(
            "docker compose up -d",
            std::path::Path::new("/tmp/demo"),
            &[("AWS_PROFILE".to_owned(), "dev".to_owned())],
        )
        .expect("hook should succeed");

        let recorded = runner.recorded();
        assert_eq!(recorded[0].program, "sh");
        assert_eq!(recorded[0].args, vec!["-c", "docker compose up -d"]);
        assert_eq!(
            recorded[0].cwd.as_deref(),
            Some(std::path::Path::new("/tmp/demo"))
        );
        assert_eq!(
            recorded[0].env,
            vec![("AWS_PROFILE".to_owned(), "dev".to_owned())]
        );
    }

    #[test]
    fn session_hook_failure_reports_stderr() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: false,
                code: Some(1),
            },
            stdout: Vec::new(),
            stderr: b"compose file not found".to_vec(),
        }));
        let tmux = Tmux::with_runner(runner.clone());

        let error = tmux
            .run_session_hook("docker compose up -d", std::path::Path::new("/tmp/demo"), &[])
            .expect_err("hook should fail");
        assert!(error.to_string().contains("compose file not found"));
    }
}
