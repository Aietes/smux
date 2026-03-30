use std::process::Command;

use anyhow::{Context, Result, bail};

pub fn list_directories() -> Result<Vec<String>> {
    let output = Command::new("zoxide").args(["query", "--list"]).output();

    match output {
        Ok(output) if output.status.success() => {
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
