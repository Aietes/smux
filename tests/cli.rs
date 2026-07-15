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

fn write_fake_tmux_snapshot_tool(dir: &Path) {
    let path = dir.join("tmux");
    let script = r#"#!/bin/sh
case "$1" in
  has-session)
    exit 0
    ;;
  list-windows)
    printf '@1\teditor\t1\n'
    ;;
  show-window-options)
    printf 'off\n'
    ;;
  list-panes)
    printf '0\t/tmp/demo\t1\t0\t0\t100\t40\n1\t/tmp/demo/server\t0\t50\t0\t50\t40\n'
    ;;
  *)
    exit 1
    ;;
esac
"#;
    fs::write(&path, script).expect("tmux stub should be written");
    let mut permissions = fs::metadata(&path)
        .expect("tmux stub metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("tmux stub should be executable");
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
        .stdout(contains("doctor"))
        .stdout(contains("save-project"));
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
    command.env("XDG_CONFIG_HOME", tempdir.path());

    command
        .assert()
        .success()
        .stdout(contains("config"))
        .stdout(contains("not found"))
        .stdout(contains("icons"));
}

#[test]
fn doctor_reports_schema_drift_without_failing() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let config_path = tempdir.path().join("config.toml");
    let project_dir = tempdir.path().join("projects");
    fs::create_dir(&project_dir).expect("project dir should be created");
    fs::write(&config_path, "[settings]\nicons = \"auto\"\n")
        .expect("config fixture should be written");
    fs::write(
        project_dir.join("demo.toml"),
        "#:schema https://raw.githubusercontent.com/Aietes/smux/v0.1.0/schemas/smux-project.schema.json\npath = \"/tmp/demo\"\n",
    )
    .expect("project fixture should be written");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["doctor", "--config"]);
    command.arg(&config_path);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env("XDG_CONFIG_HOME", tempdir.path());

    command
        .assert()
        .success()
        .stdout(contains("config schema"))
        .stdout(contains("project schemas"))
        .stdout(contains("1 drift"))
        .stdout(contains("0 missing"));
}

#[test]
fn doctor_fix_rewrites_missing_and_stale_schema_directives() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let config_path = tempdir.path().join("config.toml");
    let project_dir = tempdir.path().join("projects");
    fs::create_dir(&project_dir).expect("project dir should be created");
    fs::write(&config_path, "[settings]\nicons = \"auto\"\n")
        .expect("config fixture should be written");
    let template_dir = tempdir.path().join("templates");
    fs::create_dir(&template_dir).expect("template dir should be created");
    let stale_template = template_dir.join("default.toml");
    fs::write(
        &stale_template,
        "#:schema https://raw.githubusercontent.com/Aietes/smux/v0.1.0/schemas/smux-template.schema.json\nwindows = [{ name = \"main\" }]\n",
    )
    .expect("stale template fixture should be written");
    let stale_project = project_dir.join("stale.toml");
    fs::write(
        &stale_project,
        "#:schema https://raw.githubusercontent.com/Aietes/smux/v0.1.0/schemas/smux-project.schema.json\npath = \"/tmp/demo\"\n",
    )
    .expect("stale project fixture should be written");
    let missing_project = project_dir.join("missing.toml");
    fs::write(&missing_project, "path = \"/tmp/other\"\n")
        .expect("missing project fixture should be written");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["doctor", "--fix", "--config"]);
    command.arg(&config_path);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env("XDG_CONFIG_HOME", tempdir.path());

    command
        .assert()
        .success()
        .stdout(contains("schema fixes"))
        .stdout(contains("updated 2 · inserted 2"));

    let config_contents = fs::read_to_string(&config_path).expect("config should be readable");
    assert!(config_contents.starts_with("#:schema "));
    let stale_contents = fs::read_to_string(&stale_project).expect("project should be readable");
    let expected_project_schema = format!(
        "/v{}/schemas/smux-project.schema.json",
        env!("CARGO_PKG_VERSION")
    );
    assert!(stale_contents.contains(&expected_project_schema));
    let missing_contents =
        fs::read_to_string(&missing_project).expect("project should be readable");
    assert!(missing_contents.starts_with("#:schema "));
    let template_contents =
        fs::read_to_string(&stale_template).expect("template should be readable");
    let expected_template_schema = format!(
        "/v{}/schemas/smux-template.schema.json",
        env!("CARGO_PKG_VERSION")
    );
    assert!(template_contents.contains(&expected_template_schema));
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
    command.env("XDG_CONFIG_HOME", tempdir.path());

    command
        .assert()
        .failure()
        .stdout(contains("config"))
        .stdout(contains("error"))
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

#[test]
fn save_project_requires_session_outside_tmux() {
    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["save-project", "demo", "--stdout"]);
    command.env_remove("TMUX");

    command
        .assert()
        .failure()
        .stderr(contains("--session is required outside tmux"));
}

#[test]
fn save_project_stdout_exports_project_toml() {
    let tool_dir = tempfile::tempdir().expect("tempdir should be created");
    write_fake_tool(tool_dir.path(), "fzf");
    write_fake_tool(tool_dir.path(), "zoxide");
    write_fake_tmux_snapshot_tool(tool_dir.path());

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["save-project", "demo", "--session", "demo", "--stdout"]);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env_remove("TMUX");

    command
        .assert()
        .success()
        .stdout(contains("#:schema "))
        .stdout(contains("path = \"/tmp/demo\""))
        .stdout(contains("session_name = \"demo\""))
        .stdout(contains("startup_window = \"editor\""))
        .stdout(contains("windows = ["));
}

#[test]
fn list_projects_lists_a_project_whose_path_does_not_exist() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let config_path = tempdir.path().join("config.toml");
    let project_dir = tempdir.path().join("projects");
    fs::create_dir(&project_dir).expect("project dir should be created");
    fs::write(&config_path, "[settings]\n").expect("config fixture should be written");
    // Points at a directory that does not exist (like the starter example.toml).
    fs::write(
        project_dir.join("ghost.toml"),
        "path = \"/tmp/definitely-not-a-real-smux-dir\"\n",
    )
    .expect("project fixture should be written");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["list-projects", "--config"]);
    command.arg(&config_path);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env("XDG_CONFIG_HOME", tempdir.path());

    // Must succeed and still list the project, not abort on the missing path.
    command
        .assert()
        .success()
        .stdout(contains("ghost"))
        .stdout(contains("/tmp/definitely-not-a-real-smux-dir"));
}

#[test]
fn detect_reports_the_winning_template_and_its_markers() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let config_path = tempdir.path().join("config.toml");
    let template_dir = tempdir.path().join("templates");
    fs::create_dir(&template_dir).expect("template dir should be created");
    fs::write(&config_path, "[settings]\n").expect("config fixture should be written");
    fs::write(
        template_dir.join("rust.toml"),
        "match = [\"Cargo.toml\"]\nwindows = [{ name = \"main\" }]\n",
    )
    .expect("template fixture should be written");

    // A directory carrying the marker the template matches on.
    let target = tempdir.path().join("proj");
    fs::create_dir(&target).expect("target dir should be created");
    fs::write(target.join("Cargo.toml"), "").expect("marker should be written");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["detect"]);
    command.arg(&target);
    command.args(["--config"]);
    command.arg(&config_path);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env("XDG_CONFIG_HOME", tempdir.path());

    command
        .assert()
        .success()
        .stdout(contains("→"))
        .stdout(contains("rust"))
        .stdout(contains("Cargo.toml"));
}

#[test]
fn detect_quiet_prints_only_the_winning_template() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let config_path = tempdir.path().join("config.toml");
    let template_dir = tempdir.path().join("templates");
    fs::create_dir(&template_dir).expect("template dir should be created");
    fs::write(&config_path, "[settings]\n").expect("config fixture should be written");
    fs::write(
        template_dir.join("rust.toml"),
        "match = [\"Cargo.toml\"]\nwindows = [{ name = \"main\" }]\n",
    )
    .expect("template fixture should be written");

    let target = tempdir.path().join("proj");
    fs::create_dir(&target).expect("target dir should be created");
    fs::write(target.join("Cargo.toml"), "").expect("marker should be written");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["detect", "--quiet"]);
    command.arg(&target);
    command.args(["--config"]);
    command.arg(&config_path);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env("XDG_CONFIG_HOME", tempdir.path());

    command.assert().success().stdout("rust\n");
}

#[test]
fn detect_quiet_exits_one_without_a_match() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let config_path = tempdir.path().join("config.toml");
    fs::write(&config_path, "[settings]\n").expect("config fixture should be written");

    let target = tempdir.path().join("plain");
    fs::create_dir(&target).expect("target dir should be created");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["detect", "--quiet"]);
    command.arg(&target);
    command.args(["--config"]);
    command.arg(&config_path);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env("XDG_CONFIG_HOME", tempdir.path());

    command.assert().failure().code(1).stdout("");
}

#[test]
fn list_projects_json_emits_name_and_path_objects() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let config_path = tempdir.path().join("config.toml");
    let project_dir = tempdir.path().join("projects");
    fs::create_dir(&project_dir).expect("project dir should be created");
    fs::write(&config_path, "[settings]\n").expect("config fixture should be written");
    fs::write(
        project_dir.join("ghost.toml"),
        "path = \"/tmp/definitely-not-a-real-smux-dir\"\n",
    )
    .expect("project fixture should be written");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["list-projects", "--json", "--config"]);
    command.arg(&config_path);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env("XDG_CONFIG_HOME", tempdir.path());

    command
        .assert()
        .success()
        .stdout("[{\"name\":\"ghost\",\"path\":\"/tmp/definitely-not-a-real-smux-dir\"}]\n");
}

#[test]
fn list_templates_json_emits_a_name_array() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let config_path = tempdir.path().join("config.toml");
    let template_dir = tempdir.path().join("templates");
    fs::create_dir(&template_dir).expect("template dir should be created");
    fs::write(&config_path, "[settings]\n").expect("config fixture should be written");
    fs::write(
        template_dir.join("rust.toml"),
        "match = [\"Cargo.toml\"]\nwindows = [{ name = \"main\" }]\n",
    )
    .expect("template fixture should be written");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["list-templates", "--json", "--config"]);
    command.arg(&config_path);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env("XDG_CONFIG_HOME", tempdir.path());

    command.assert().success().stdout("[\"rust\"]\n");
}

#[test]
fn detect_rejects_missing_paths() {
    let tool_dir = fake_tool_dir();

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["detect", "/nonexistent/smux-test-path"]);
    command.env("PATH", prepend_path(tool_dir.path()));

    command
        .assert()
        .failure()
        .stderr(contains("failed to resolve path"));
}

#[test]
fn kill_reports_the_killed_session() {
    let tool_dir = fake_tool_dir();

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["kill", "demo"]);
    command.env("PATH", prepend_path(tool_dir.path()));

    command.assert().success().stdout(contains("killed demo"));
}

#[test]
fn clone_runs_git_and_connects() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    // A git stub that "clones" by creating the target directory, so the
    // subsequent connect finds a real path. It also asserts the `--`
    // separator smux must pass so `-`-prefixed URLs can't smuggle git flags
    // (argument injection, e.g. --upload-pack).
    let git = tool_dir.path().join("git");
    fs::write(
        &git,
        "#!/bin/sh\n[ \"$2\" = \"--\" ] || exit 9\nmkdir -p \"$4\"\n",
    )
    .expect("git stub should be written");
    let mut permissions = fs::metadata(&git)
        .expect("git stub metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&git, permissions).expect("git stub should be executable");

    let target = tempdir.path().join("demo");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["clone", "https://example.com/user/demo.git"]);
    command.arg("--dir");
    command.arg(&target);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env("XDG_CONFIG_HOME", tempdir.path());
    command.env_remove("TMUX");

    // The stub tmux reports the session as existing and "attaches" with
    // exit 0, so a successful run proves clone ran and connect took over.
    command.assert().success();
    assert!(target.is_dir(), "git stub should have created the checkout");
}

#[test]
fn clone_no_connect_just_prints_the_checkout_path() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let tool_dir = fake_tool_dir();
    let git = tool_dir.path().join("git");
    fs::write(
        &git,
        "#!/bin/sh\n[ \"$2\" = \"--\" ] || exit 9\nmkdir -p \"$4\"\n",
    )
    .expect("git stub should be written");
    let mut permissions = fs::metadata(&git)
        .expect("git stub metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&git, permissions).expect("git stub should be executable");

    let target = tempdir.path().join("demo");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["clone", "--no-connect", "https://example.com/user/demo.git"]);
    command.arg("--dir");
    command.arg(&target);
    command.env("PATH", prepend_path(tool_dir.path()));
    command.env("XDG_CONFIG_HOME", tempdir.path());
    command.env_remove("TMUX");

    command
        .assert()
        .success()
        .stdout(contains(target.display().to_string()));
    assert!(target.is_dir(), "git stub should have created the checkout");
}

#[test]
fn skill_writes_skill_md_to_the_given_dir() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let skill_dir = tempdir.path().join("skills").join("smux");

    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.args(["skill", "--dir"]);
    command.arg(&skill_dir);

    command.assert().success().stdout(contains("SKILL.md"));

    let written =
        fs::read_to_string(skill_dir.join("SKILL.md")).expect("SKILL.md should be written");
    assert!(written.starts_with("---\nname: smux-config\n"));
    assert!(written.contains("## Troubleshooting"));
    assert!(written.contains("## Errors → fixes"));
}

#[test]
fn skill_without_dir_prints_to_stdout() {
    let mut command = Command::cargo_bin("smux").expect("binary should build");
    command.arg("skill");

    command
        .assert()
        .success()
        .stdout(contains("name: smux-config"))
        .stdout(contains("Authoring smux templates and projects"));
}
