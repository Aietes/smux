use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::process::{CommandRunner, default_runner};
use crate::ui::DisplayStyle;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntryKind {
    Session,
    Directory,
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

    fn encode(&self) -> String {
        let kind = match self.kind {
            EntryKind::Session => "session",
            EntryKind::Directory => "folder",
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
            other => bail!("unknown picker entry kind: {other}"),
        };

        Ok(Self { kind, label, value })
    }
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

pub fn select(entries: Vec<Entry>) -> Result<Option<Entry>> {
    select_with_runner(default_runner(), entries, "smux> ")
}

pub fn select_value(prompt: &str, choices: Vec<Choice>) -> Result<Option<String>> {
    select_value_with_runner(default_runner(), prompt, choices)
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
    add_common_picker_args(
        &mut args,
        prompt,
        "ctrl-a all  ctrl-t templates",
        "ctrl-a:change-prompt(template> )+change-query(),ctrl-t:change-prompt(template> )+change-query(template )",
    );
    args.extend(["--nth".to_owned(), "1,2,3".to_owned()]);
    let input = choices
        .into_iter()
        .map(|choice| choice.encode())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
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
) -> Result<Option<Entry>> {
    let mut args = Vec::new();
    add_common_picker_args(
        &mut args,
        prompt,
        "ctrl-a all  ctrl-s sessions  ctrl-f folders",
        "ctrl-a:change-prompt(smux> )+change-query(),ctrl-s:change-prompt(session> )+change-query(session ),ctrl-f:change-prompt(folder> )+change-query(folder )",
    );
    args.extend(["--nth".to_owned(), "1,3".to_owned()]);
    let input = entries
        .into_iter()
        .map(|entry| entry.encode())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
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

    Ok(Some(Entry::decode(selection)?))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::process::{CommandOutput, CommandStatus, FakeCommandRunner};

    use super::{Choice, Entry, EntryKind, select_value_with_runner, select_with_runner};
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
        assert!(recorded[0].args.contains(&"1,3".to_owned()));
        assert!(recorded[0].args.iter().any(|arg| arg.contains("ctrl-s sessions")));
        assert!(recorded[0].args.iter().any(|arg| arg.contains("ctrl-f:change-prompt(folder> )")));
        assert_eq!(
            recorded[0].stdin.as_deref(),
            Some("folder\t/tmp/example\tdir      /tmp/example\n")
        );
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
        assert!(recorded[0].args.contains(&"1,2,3".to_owned()));
        assert!(recorded[0].args.iter().any(|arg| arg.contains("ctrl-t templates")));
        assert!(recorded[0].args.iter().any(|arg| arg.contains("ctrl-t:change-prompt(template> )")));
        assert_eq!(
            recorded[0].stdin.as_deref(),
            Some("template\tdefault\ttemplate default\ntemplate\trust\ttemplate rust\n")
        );
    }
}
