use assert_cmd::Command;
use predicates::str::contains;
use std::fs;

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
    let config_path = tempdir.path().join("missing.toml");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["doctor", "--config"]);
    command.arg(&config_path);

    command
        .assert()
        .success()
        .stdout(contains("config: missing"))
        .stdout(contains("icons:"));
}

#[test]
fn doctor_fails_with_invalid_config_file() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let config_path = tempdir.path().join("invalid.toml");
    fs::write(&config_path, "not = [valid").expect("config fixture should be written");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["doctor", "--config"]);
    command.arg(&config_path);

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
}
