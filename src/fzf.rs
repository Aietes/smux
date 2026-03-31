use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::process::{CommandRunner, default_runner};
use crate::ui::DisplayStyle;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntryKind {
    Session,
    Directory,
    Project,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectAction {
    Open,
    Delete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub kind: EntryKind,
    pub label: String,
    pub value: String,
}

impl Entry {
    pub fn session(style: DisplayStyle, value: String) -> Self {
        Self {
            kind: EntryKind::Session,
            label: style.session_label(&value),
            value,
        }
    }

    pub fn directory(style: DisplayStyle, value: String) -> Self {
        Self {
            kind: EntryKind::Directory,
            label: style.directory_label(&value),
            value,
        }
    }

    pub fn project(style: DisplayStyle, value: String) -> Self {
        Self {
            kind: EntryKind::Project,
            label: style.project_label(&value),
            value,
        }
    }

    fn encode(&self) -> String {
        let kind = match self.kind {
            EntryKind::Session => "session",
            EntryKind::Directory => "folder",
            EntryKind::Project => "project",
        };

        format!("{kind}\t{}\t{}", self.value, self.label)
    }

    fn decode(line: &str) -> Result<Self> {
        let mut parts = line.splitn(3, '\t');
        let kind = parts.next().context("missing entry kind")?;
        let value = parts.next().context("missing entry value")?.to_owned();
        let label = parts.next().context("missing entry label")?.to_owned();

        let kind = match kind {
            "session" => EntryKind::Session,
            "folder" => EntryKind::Directory,
            "project" => EntryKind::Project,
            other => bail!("unknown picker entry kind: {other}"),
        };

        Ok(Self { kind, label, value })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Selection {
    pub action: SelectAction,
    pub entry: Entry,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Choice {
    pub kind: String,
    pub label: String,
    pub value: String,
}

impl Choice {
    pub fn new(kind: impl Into<String>, label: String, value: String) -> Self {
        Self {
            kind: kind.into(),
            label,
            value,
        }
    }

    fn encode(&self) -> String {
        format!("{}\t{}\t{}", self.kind, self.value, self.label)
    }

    fn decode(line: &str) -> Result<Self> {
        let mut parts = line.splitn(3, '\t');
        let kind = parts.next().context("missing choice kind")?.to_owned();
        let value = parts.next().context("missing choice value")?.to_owned();
        let label = parts.next().context("missing choice label")?.to_owned();
        Ok(Self { kind, label, value })
    }
}

pub fn select(entries: Vec<Entry>) -> Result<Option<Selection>> {
    select_with_runner(default_runner(), entries, "smux> ")
}

pub fn select_value(prompt: &str, choices: Vec<Choice>) -> Result<Option<String>> {
    select_value_with_runner(default_runner(), prompt, choices)
}

struct TempInputFile {
    path: PathBuf,
}

impl TempInputFile {
    fn new(contents: &str) -> Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock should be after unix epoch")?
            .as_nanos();
        path.push(format!("smux-fzf-{}-{nanos}.tsv", std::process::id()));
        fs::write(&path, contents)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(Self { path })
    }

    fn shell_quoted_path(&self) -> String {
        shell_quote(&self.path)
    }
}

impl Drop for TempInputFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn cat_command(file: &TempInputFile) -> String {
    format!("cat {}", file.shell_quoted_path())
}

fn filter_command(file: &TempInputFile, kind: &str) -> String {
    format!(
        "awk -F '\\t' '$1 == \"{kind}\"' {}",
        file.shell_quoted_path()
    )
}

fn add_common_picker_args(args: &mut Vec<String>, prompt: &str, header: &str, bindings: &str) {
    args.extend([
        "--ansi".to_owned(),
        "--delimiter".to_owned(),
        "\t".to_owned(),
        "--layout".to_owned(),
        "reverse".to_owned(),
        "--header".to_owned(),
        header.to_owned(),
        "--bind".to_owned(),
        "tab:down,btab:up".to_owned(),
        "--bind".to_owned(),
        bindings.to_owned(),
        "--with-nth".to_owned(),
        "3".to_owned(),
        "--nth".to_owned(),
        "1,2".to_owned(),
        "--prompt".to_owned(),
        prompt.to_owned(),
        "--no-sort".to_owned(),
    ]);
}

fn select_value_with_runner(
    runner: Arc<dyn CommandRunner>,
    prompt: &str,
    choices: Vec<Choice>,
) -> Result<Option<String>> {
    let mut args = Vec::new();
    let input = choices
        .into_iter()
        .map(|choice| choice.encode())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let input_file = TempInputFile::new(&input)?;
    let all_command = cat_command(&input_file);
    let template_command = filter_command(&input_file, "template");
    add_common_picker_args(
        &mut args,
        prompt,
        "ctrl-x all  ctrl-t templates",
        &format!(
            "ctrl-x:change-prompt(template> )+clear-query+reload({all_command}),ctrl-t:change-prompt(template> )+clear-query+reload({template_command})"
        ),
    );
    let output = runner
        .run_capture_with_input("fzf", &args, &input)
        .context("failed to launch fzf")?;

    if output.status.code == Some(130) {
        return Ok(None);
    }

    if !output.status.success {
        bail!("fzf exited with status {:?}", output.status.code);
    }

    let selection = String::from_utf8(output.stdout).context("fzf output was not valid utf-8")?;
    let selection = selection.trim_end();

    if selection.is_empty() {
        return Ok(None);
    }

    Ok(Some(Choice::decode(selection)?.value))
}

fn select_with_runner(
    runner: Arc<dyn CommandRunner>,
    entries: Vec<Entry>,
    prompt: &str,
) -> Result<Option<Selection>> {
    let mut args = Vec::new();
    let input = entries
        .into_iter()
        .map(|entry| entry.encode())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let input_file = TempInputFile::new(&input)?;
    let all_command = cat_command(&input_file);
    let session_command = filter_command(&input_file, "session");
    let folder_command = filter_command(&input_file, "folder");
    let project_command = filter_command(&input_file, "project");
    add_common_picker_args(
        &mut args,
        prompt,
        "enter open  ctrl-x kill session  ctrl-c all  ctrl-s sessions  ctrl-f folders  ctrl-p projects",
        &format!(
            "ctrl-c:change-prompt(smux> )+clear-query+reload({all_command}),ctrl-s:change-prompt(session> )+clear-query+reload({session_command}),ctrl-f:change-prompt(folder> )+clear-query+reload({folder_command}),ctrl-p:change-prompt(project> )+clear-query+reload({project_command})"
        ),
    );
    args.extend(["--expect".to_owned(), "ctrl-x".to_owned()]);
    let output = runner
        .run_capture_with_input("fzf", &args, &input)
        .context("failed to launch fzf")?;

    if output.status.code == Some(130) {
        return Ok(None);
    }

    if !output.status.success {
        bail!("fzf exited with status {:?}", output.status.code);
    }

    let selection = String::from_utf8(output.stdout).context("fzf output was not valid utf-8")?;
    let selection = selection.trim_end();

    if selection.is_empty() {
        return Ok(None);
    }

    let mut lines = selection.lines();
    let first = lines
        .next()
        .context("fzf selection output was unexpectedly empty")?;
    let (action, encoded_entry) = match lines.next() {
        Some(encoded_entry) if !first.is_empty() => {
            let action = match first {
                "ctrl-x" => SelectAction::Delete,
                other => bail!("unknown picker action: {other}"),
            };
            (action, encoded_entry)
        }
        Some(encoded_entry) => (SelectAction::Open, encoded_entry),
        None => (SelectAction::Open, first),
    };

    Ok(Some(Selection {
        action,
        entry: Entry::decode(encoded_entry)?,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::process::{CommandOutput, CommandStatus, FakeCommandRunner};

    use super::{
        Choice, Entry, EntryKind, SelectAction, select_value_with_runner, select_with_runner,
    };
    use crate::config::IconMode;
    use crate::ui::DisplayStyle;

    #[test]
    fn entry_round_trip() {
        let entry = Entry {
            kind: EntryKind::Directory,
            label: "dir      /tmp/example".to_owned(),
            value: "/tmp/example".to_owned(),
        };

        let decoded = Entry::decode(&entry.encode()).expect("entry should decode");
        assert_eq!(decoded, entry);
    }

    #[test]
    fn selector_passes_entries_to_fzf() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"folder\t/tmp/example\tdir      /tmp/example\n".to_vec(),
            stderr: Vec::new(),
        }));

        let result = select_with_runner(
            runner.clone(),
            vec![Entry::directory(
                DisplayStyle::from_icon_mode(IconMode::Never),
                "/tmp/example".to_owned(),
            )],
            "smux> ",
        )
        .expect("selection should succeed");

        assert!(result.is_some());
        let recorded = runner.recorded();
        assert_eq!(recorded[0].program, "fzf");
        assert!(recorded[0].args.contains(&"--ansi".to_owned()));
        assert!(recorded[0].args.contains(&"reverse".to_owned()));
        assert!(recorded[0].args.contains(&"3".to_owned()));
        assert!(recorded[0].args.contains(&"1,2".to_owned()));
        assert!(recorded[0].args.contains(&"--expect".to_owned()));
        assert!(recorded[0].args.contains(&"ctrl-x".to_owned()));
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("enter open  ctrl-x kill session"))
        );
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("ctrl-c:change-prompt(smux> )+clear-query+reload("))
        );
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("ctrl-p projects"))
        );
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("ctrl-p:change-prompt(project> )+clear-query+reload("))
        );
        assert_eq!(
            recorded[0].stdin.as_deref(),
            Some("folder\t/tmp/example\tdir      /tmp/example\n")
        );
        let selection = result.expect("selection should be present");
        assert_eq!(selection.action, SelectAction::Open);
        assert_eq!(selection.entry.kind, EntryKind::Directory);
    }

    #[test]
    fn selector_supports_delete_action_for_sessions() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"ctrl-x\nsession\tdemo\tsession  demo\n".to_vec(),
            stderr: Vec::new(),
        }));

        let result = select_with_runner(
            runner,
            vec![Entry::session(
                DisplayStyle::from_icon_mode(IconMode::Never),
                "demo".to_owned(),
            )],
            "smux> ",
        )
        .expect("selection should succeed")
        .expect("selection should be present");

        assert_eq!(result.action, SelectAction::Delete);
        assert_eq!(result.entry.kind, EntryKind::Session);
        assert_eq!(result.entry.value, "demo");
    }

    #[test]
    fn selector_treats_empty_expect_key_as_open() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"\nfolder\t/tmp/example\tdir      /tmp/example\n".to_vec(),
            stderr: Vec::new(),
        }));

        let result = select_with_runner(
            runner,
            vec![Entry::directory(
                DisplayStyle::from_icon_mode(IconMode::Never),
                "/tmp/example".to_owned(),
            )],
            "smux> ",
        )
        .expect("selection should succeed")
        .expect("selection should be present");

        assert_eq!(result.action, SelectAction::Open);
        assert_eq!(result.entry.kind, EntryKind::Directory);
        assert_eq!(result.entry.value, "/tmp/example");
    }

    #[test]
    fn template_selector_returns_selected_value() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"template\trust\ttemplate rust\n".to_vec(),
            stderr: Vec::new(),
        }));

        let result = select_value_with_runner(
            runner.clone(),
            "template> ",
            vec![
                Choice::new(
                    "template",
                    "template default".to_owned(),
                    "default".to_owned(),
                ),
                Choice::new("template", "template rust".to_owned(), "rust".to_owned()),
            ],
        )
        .expect("selection should succeed");

        assert_eq!(result.as_deref(), Some("rust"));
        let recorded = runner.recorded();
        assert!(recorded[0].args.contains(&"--ansi".to_owned()));
        assert!(recorded[0].args.contains(&"reverse".to_owned()));
        assert!(recorded[0].args.contains(&"3".to_owned()));
        assert!(recorded[0].args.contains(&"1,2".to_owned()));
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("ctrl-t templates"))
        );
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("ctrl-t:change-prompt(template> )+clear-query+reload("))
        );
        assert_eq!(
            recorded[0].stdin.as_deref(),
            Some("template\tdefault\ttemplate default\ntemplate\trust\ttemplate rust\n")
        );
    }
}
