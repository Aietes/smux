use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

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
    pub fn session(value: String) -> Self {
        Self {
            kind: EntryKind::Session,
            label: format!("session\t{value}"),
            value,
        }
    }

    pub fn directory(value: String) -> Self {
        Self {
            kind: EntryKind::Directory,
            label: format!("dir\t{value}"),
            value,
        }
    }

    fn encode(&self) -> String {
        let kind = match self.kind {
            EntryKind::Session => "session",
            EntryKind::Directory => "dir",
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
            "dir" => EntryKind::Directory,
            other => bail!("unknown picker entry kind: {other}"),
        };

        Ok(Self { kind, label, value })
    }
}

pub fn select(entries: Vec<Entry>) -> Result<Option<Entry>> {
    let mut child = Command::new("fzf")
        .args([
            "--delimiter",
            "\t",
            "--with-nth",
            "3",
            "--prompt",
            "smux> ",
            "--no-sort",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to launch fzf")?;

    {
        let mut stdin = child.stdin.take().context("failed to open fzf stdin")?;
        for entry in entries {
            writeln!(stdin, "{}", entry.encode()).context("failed to write picker entry")?;
        }
    }

    let output = child.wait_with_output().context("failed to wait for fzf")?;

    if output.status.code() == Some(130) {
        return Ok(None);
    }

    if !output.status.success() {
        bail!("fzf exited with status {}", output.status);
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
    use super::{Entry, EntryKind};

    #[test]
    fn entry_round_trip() {
        let entry = Entry {
            kind: EntryKind::Directory,
            label: "dir\t~/code/example".to_owned(),
            value: "/tmp/example".to_owned(),
        };

        let decoded = Entry::decode(&entry.encode()).expect("entry should decode");
        assert_eq!(decoded, entry);
    }
}
