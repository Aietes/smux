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

        let mut previous_window = first_window.name.as_str();
        for window in plan.windows.iter().skip(1) {
            self.new_window_after(
                &plan.session_name,
                previous_window,
                &window.name,
                &window.cwd,
            )?;
            self.configure_panes(&plan.session_name, &window.name, window)?;
            previous_window = &window.name;
        }

        self.select_window(&plan.session_name, &plan.startup_window)?;
        self.select_pane_by_offset(&plan.session_name, &plan.startup_window, plan.startup_pane)?;
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

    pub fn kill_session(&self, session: &str) -> Result<()> {
        self.run_tmux(["kill-session", "-t", session])
            .context("failed to execute tmux kill-session")
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
        self.run_tmux(["send-keys", "-t", target, command, "C-m"])
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
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tmux split-window failed: {}", stderr.trim());
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
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tmux list-panes failed: {}", stderr.trim());
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
        let output = self.run_tmux_capture(args)?;

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
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tmux list-windows failed: {}", stderr.trim());
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
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tmux show-window-options failed: {}", stderr.trim());
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
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tmux list-panes failed: {}", stderr.trim());
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

fn infer_pane_layouts(panes: Vec<PaneRecord>) -> Vec<PaneSnapshot> {
    let mut inferred = Vec::with_capacity(panes.len());

    for pane in panes {
        let layout = if inferred.is_empty() {
            None
        } else {
            Some(PaneLayout {
                position: infer_pane_position(&pane, &inferred),
                size: None,
            })
        };

        inferred.push(PaneSnapshot {
            cwd: pane.cwd,
            active: pane.active,
            layout,
        });
    }

    inferred
}

fn infer_pane_position(pane: &PaneRecord, previous: &[PaneSnapshot]) -> PanePosition {
    let _ = previous;
    if pane.left > 0 && pane.top == 0 {
        PanePosition::Right
    } else if pane.top > 0 && pane.left == 0 {
        PanePosition::Bottom
    } else if pane.left > 0 {
        PanePosition::Right
    } else if pane.top > 0 {
        PanePosition::Bottom
    } else {
        PanePosition::Right
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;

    use crate::process::{CommandOutput, CommandStatus, FakeCommandRunner, IoMode};
    use crate::templates::{PaneLayout, PanePlan, PanePosition, SessionPlan, WindowPlan};

    use super::Tmux;

    static TMUX_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn outside_tmux_uses_inherited_stdio_for_attach() {
        let _guard = TMUX_ENV_LOCK.lock().expect("tmux env lock should work");
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
    fn outside_tmux_has_no_current_session() {
        let _guard = TMUX_ENV_LOCK.lock().expect("tmux env lock should work");
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
        let _guard = TMUX_ENV_LOCK.lock().expect("tmux env lock should work");
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
    fn inside_tmux_reads_current_session() {
        let _guard = TMUX_ENV_LOCK.lock().expect("tmux env lock should work");
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
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"%1\n".to_vec()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"%2\n".to_vec()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"%3\n".to_vec()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"%1\n".to_vec()));
        runner.push_capture(ok_capture(Vec::new()));

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
            vec!["send-keys", "-t", "%1", "source .venv/bin/activate", "C-m"]
        );
        assert_eq!(
            recorded[3].args,
            vec!["send-keys", "-t", "%1", "nvim", "C-m"]
        );
        assert_eq!(
            recorded[4].args,
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
            recorded[5].args,
            vec!["list-panes", "-t", "demo:run", "-F", "#{pane_id}"]
        );
        assert_eq!(
            recorded[6].args,
            vec!["send-keys", "-t", "%2", "source .venv/bin/activate", "C-m"]
        );
        assert_eq!(
            recorded[7].args,
            vec!["send-keys", "-t", "%2", "cargo run", "C-m"]
        );
        assert_eq!(
            recorded[8].args,
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
            recorded[9].args,
            vec!["send-keys", "-t", "%3", "source .venv/bin/activate", "C-m"]
        );
        assert_eq!(
            recorded[10].args,
            vec!["send-keys", "-t", "%3", "cargo test", "C-m"]
        );
        assert_eq!(
            recorded[11].args,
            vec!["select-layout", "-t", "demo:run", "main-horizontal"]
        );
        assert_eq!(
            recorded[12].args,
            vec![
                "set-window-option",
                "-t",
                "demo:run",
                "synchronize-panes",
                "on"
            ]
        );
        assert_eq!(
            recorded[13].args,
            vec!["select-window", "-t", "demo:editor"]
        );
        assert_eq!(
            recorded[14].args,
            vec!["list-panes", "-t", "demo:editor", "-F", "#{pane_id}"]
        );
        assert_eq!(recorded[15].args, vec!["select-pane", "-t", "%1"]);
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
        assert_eq!(recorded[0].args, vec!["kill-session", "-t", "demo"]);
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
                size: None,
            })
        );
        assert!(snapshot.windows[1].synchronize);
    }

    #[test]
    fn startup_pane_uses_zero_based_offset_not_tmux_base_index() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"%10\n".to_vec()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"%11\n".to_vec()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"%10\n%11\n".to_vec()));
        runner.push_capture(ok_capture(Vec::new()));

        let tmux = Tmux::with_runner(runner.clone());
        let plan = SessionPlan {
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
            recorded[6].args,
            vec!["list-panes", "-t", "demo:main", "-F", "#{pane_id}"]
        );
        assert_eq!(recorded[7].args, vec!["select-pane", "-t", "%11"]);
    }

    #[test]
    fn zoomed_pane_emits_resize_pane_command() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"%20\n".to_vec()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"%21\n".to_vec()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(Vec::new()));
        runner.push_capture(ok_capture(b"%20\n%21\n".to_vec()));
        runner.push_capture(ok_capture(Vec::new()));

        let tmux = Tmux::with_runner(runner.clone());
        let plan = SessionPlan {
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
        assert_eq!(recorded[5].args, vec!["resize-pane", "-Z", "-t", "%21"]);
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
}
