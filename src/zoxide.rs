use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::process::{CommandRunner, default_runner};

pub fn list_directories() -> Result<Vec<String>> {
    list_directories_with_runner(default_runner())
}

fn list_directories_with_runner(runner: Arc<dyn CommandRunner>) -> Result<Vec<String>> {
    let args = vec!["query".to_owned(), "--list".to_owned()];
    let output = runner.run_capture("zoxide", &args);

    match output {
        Ok(output) if output.status.success => {
            let stdout =
                String::from_utf8(output.stdout).context("zoxide output was not valid utf-8")?;

            Ok(stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("zoxide query failed: {}", stderr.trim())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("zoxide is not installed or not on PATH")
        }
        Err(error) => Err(error).context("failed to execute zoxide query"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::process::{CommandOutput, CommandStatus, FakeCommandRunner};

    use super::list_directories_with_runner;

    #[test]
    fn parses_zoxide_output() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"/tmp/a\n/tmp/b\n".to_vec(),
            stderr: Vec::new(),
        }));

        let directories =
            list_directories_with_runner(runner.clone()).expect("zoxide query should work");
        assert_eq!(directories, vec!["/tmp/a", "/tmp/b"]);

        let recorded = runner.recorded();
        assert_eq!(recorded[0].program, "zoxide");
        assert_eq!(recorded[0].args, vec!["query", "--list"]);
    }
}
