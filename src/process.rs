use std::io;
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};

#[cfg(test)]
use std::collections::VecDeque;
#[cfg(test)]
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CommandStatus {
    pub success: bool,
    pub code: Option<i32>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommandOutput {
    pub status: CommandStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait CommandRunner: Send + Sync {
    fn run_capture(&self, program: &str, args: &[String]) -> io::Result<CommandOutput>;
    fn run_capture_with_input(
        &self,
        program: &str,
        args: &[String],
        input: &str,
    ) -> io::Result<CommandOutput>;
    fn run_inherit(&self, program: &str, args: &[String]) -> io::Result<CommandStatus>;
}

pub fn default_runner() -> Arc<dyn CommandRunner> {
    static RUNNER: OnceLock<Arc<dyn CommandRunner>> = OnceLock::new();
    RUNNER.get_or_init(|| Arc::new(RealCommandRunner)).clone()
}

#[derive(Debug)]
struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run_capture(&self, program: &str, args: &[String]) -> io::Result<CommandOutput> {
        let output = Command::new(program).args(args).output()?;
        Ok(CommandOutput {
            status: CommandStatus {
                success: output.status.success(),
                code: output.status.code(),
            },
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    fn run_capture_with_input(
        &self,
        program: &str,
        args: &[String],
        input: &str,
    ) -> io::Result<CommandOutput> {
        use std::io::Write;

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input.as_bytes())?;
        }

        let output = child.wait_with_output()?;
        Ok(CommandOutput {
            status: CommandStatus {
                success: output.status.success(),
                code: output.status.code(),
            },
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    fn run_inherit(&self, program: &str, args: &[String]) -> io::Result<CommandStatus> {
        let status = Command::new(program)
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;

        Ok(CommandStatus {
            success: status.success(),
            code: status.code(),
        })
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IoMode {
    Capture,
    Inherit,
}

#[cfg(test)]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RecordedCommand {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Option<String>,
    pub io_mode: IoMode,
}

#[cfg(test)]
#[derive(Debug)]
enum PlannedResponse {
    Capture(io::Result<CommandOutput>),
    Inherit(io::Result<CommandStatus>),
}

#[cfg(test)]
#[derive(Debug)]
pub struct FakeCommandRunner {
    planned: Mutex<VecDeque<PlannedResponse>>,
    recorded: Mutex<Vec<RecordedCommand>>,
}

#[cfg(test)]
impl FakeCommandRunner {
    pub fn new() -> Self {
        Self {
            planned: Mutex::new(VecDeque::new()),
            recorded: Mutex::new(Vec::new()),
        }
    }

    pub fn push_capture(&self, result: io::Result<CommandOutput>) {
        self.planned
            .lock()
            .expect("planned queue should lock")
            .push_back(PlannedResponse::Capture(result));
    }

    pub fn push_inherit(&self, result: io::Result<CommandStatus>) {
        self.planned
            .lock()
            .expect("planned queue should lock")
            .push_back(PlannedResponse::Inherit(result));
    }

    pub fn recorded(&self) -> Vec<RecordedCommand> {
        self.recorded
            .lock()
            .expect("recorded queue should lock")
            .clone()
    }
}

#[cfg(test)]
impl Default for FakeCommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl CommandRunner for FakeCommandRunner {
    fn run_capture(&self, program: &str, args: &[String]) -> io::Result<CommandOutput> {
        self.recorded
            .lock()
            .expect("recorded queue should lock")
            .push(RecordedCommand {
                program: program.to_owned(),
                args: args.to_vec(),
                stdin: None,
                io_mode: IoMode::Capture,
            });

        match self
            .planned
            .lock()
            .expect("planned queue should lock")
            .pop_front()
            .expect("missing planned capture response")
        {
            PlannedResponse::Capture(result) => result,
            PlannedResponse::Inherit(_) => {
                panic!("expected capture response but inherit response was queued")
            }
        }
    }

    fn run_capture_with_input(
        &self,
        program: &str,
        args: &[String],
        input: &str,
    ) -> io::Result<CommandOutput> {
        self.recorded
            .lock()
            .expect("recorded queue should lock")
            .push(RecordedCommand {
                program: program.to_owned(),
                args: args.to_vec(),
                stdin: Some(input.to_owned()),
                io_mode: IoMode::Capture,
            });

        match self
            .planned
            .lock()
            .expect("planned queue should lock")
            .pop_front()
            .expect("missing planned capture-with-input response")
        {
            PlannedResponse::Capture(result) => result,
            PlannedResponse::Inherit(_) => {
                panic!("expected capture response but inherit response was queued")
            }
        }
    }

    fn run_inherit(&self, program: &str, args: &[String]) -> io::Result<CommandStatus> {
        self.recorded
            .lock()
            .expect("recorded queue should lock")
            .push(RecordedCommand {
                program: program.to_owned(),
                args: args.to_vec(),
                stdin: None,
                io_mode: IoMode::Inherit,
            });

        match self
            .planned
            .lock()
            .expect("planned queue should lock")
            .pop_front()
            .expect("missing planned inherit response")
        {
            PlannedResponse::Inherit(result) => result,
            PlannedResponse::Capture(_) => {
                panic!("expected inherit response but capture response was queued")
            }
        }
    }
}
