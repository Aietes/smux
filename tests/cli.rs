use assert_cmd::Command;
use predicates::str::contains;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn fake_tool_dir() -> tempfile::TempDir {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");

    for tool in ["tmux", "fzf", "zoxide"] {
        write_fake_tool(tempdir.path(), tool);
    }

    tempdir
}

fn write_fake_tool(dir: &Path, name: &str) {
    let path = dir.join(name);
    fs::write(&path, "#!/bin/sh\nexit 0\n").expect("tool stub should be written");
    let mut permissions = fs::metadata(&path)
        .expect("tool stub metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("tool stub should be executable");
}

fn prepend_path(dir: &Path) -> String {
    let current = env::var("PATH").unwrap_or_default();
    format!("{}:{}", dir.display(), current)
}

#[test]
fn help_includes_subcommands() {
    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.arg("--help");

    command
        .assert()
        .success()
        .stdout(contains("select"))
        .stdout(contains("doctor"));
}

#[test]
fn missing_subcommand_is_a_usage_error() {
    let mut command = Command::cargo_bin("smux").expect("binary should build");

    command
        .assert()
        .failure()
        .stderr(contains("Usage:"))
        .stderr(contains("<COMMAND>"));
}

#[test]
fn doctor_succeeds_with_missing_config_file() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let config_path = tempdir.path().join("missing.toml");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["doctor", "--config"]);
    command.arg(&config_path);
    command.env("PATH", prepend_path(tool_dir.path()));

    command
        .assert()
        .success()
        .stdout(contains("config: missing"))
        .stdout(contains("icons:"));
}

#[test]
fn doctor_fails_with_invalid_config_file() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let config_path = tempdir.path().join("invalid.toml");
    fs::write(&config_path, "not = [valid").expect("config fixture should be written");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["doctor", "--config"]);
    command.arg(&config_path);
    command.env("PATH", prepend_path(tool_dir.path()));

    command
        .assert()
        .failure()
        .stdout(contains("config: error"))
        .stderr(contains("doctor checks failed"));
}

#[test]
fn completions_command_outputs_zsh_script() {
    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["completions", "zsh"]);

    command
        .assert()
        .success()
        .stdout(contains("compdef"))
        .stdout(contains("smux"));
}

#[test]
fn man_command_outputs_manpage() {
    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.arg("man");

    command
        .assert()
        .success()
        .stdout(contains(".TH"))
        .stdout(contains("smux"));
}

#[test]
fn completions_command_writes_to_directory() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["completions", "zsh", "--dir"]);
    command.arg(tempdir.path());

    command.assert().success();
    assert!(tempdir.path().join("_smux").exists());
}

#[test]
fn man_command_writes_to_directory() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["man", "--dir"]);
    command.arg(tempdir.path());

    command.assert().success();
    assert!(tempdir.path().join("smux.1").exists());
    assert!(tempdir.path().join("smux-select.1").exists());
    assert!(tempdir.path().join("smux-config.5").exists());
}
