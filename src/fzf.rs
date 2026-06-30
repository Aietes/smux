use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::config::{PickerBindings, PickerPreviewSettings};
use crate::process::{CommandRunner, default_runner};
use crate::ui::DisplayStyle;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntryKind {
    Session,
    Directory,
    Project,
    InvalidProject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectAction {
    Open,
    Delete,
    SaveProject,
    Rename,
    Edit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub kind: EntryKind,
    pub label: String,
    pub value: String,
    pub preview: Option<String>,
}

impl Entry {
    pub fn session(style: DisplayStyle, value: String) -> Self {
        Self {
            kind: EntryKind::Session,
            label: style.session_label(&value),
            value,
            preview: None,
        }
    }

    pub fn directory(style: DisplayStyle, value: String) -> Self {
        Self {
            kind: EntryKind::Directory,
            label: style.directory_label(&value),
            value,
            preview: None,
        }
    }

    pub fn project(
        style: DisplayStyle,
        value: String,
        label_value: String,
        preview: Option<String>,
    ) -> Self {
        Self {
            kind: EntryKind::Project,
            label: style.project_label(&label_value),
            value,
            preview,
        }
    }

    pub fn invalid_project(
        style: DisplayStyle,
        value: String,
        error: &str,
        preview: Option<String>,
    ) -> Self {
        Self {
            kind: EntryKind::InvalidProject,
            label: style.invalid_project_label(&value, error),
            value,
            preview,
        }
    }

    fn encode(&self) -> String {
        let kind = match self.kind {
            EntryKind::Session => "session",
            EntryKind::Directory => "folder",
            EntryKind::Project => "project",
            EntryKind::InvalidProject => "project-broken",
        };

        let value = sanitize_field(&self.value);
        let label = sanitize_field(&self.label);
        let preview = sanitize_field(self.preview.as_deref().unwrap_or_default());
        format!("{kind}\t{value}\t{label}\t{preview}")
    }

    fn decode(line: &str) -> Result<Self> {
        let mut parts = line.splitn(4, '\t');
        let kind = parts.next().context("missing entry kind")?;
        let value = parts.next().context("missing entry value")?.to_owned();
        let label = parts.next().context("missing entry label")?.to_owned();
        let preview = parts
            .next()
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        let kind = match kind {
            "session" => EntryKind::Session,
            "folder" => EntryKind::Directory,
            "project" => EntryKind::Project,
            "project-broken" => EntryKind::InvalidProject,
            other => bail!("unknown picker entry kind: {other}"),
        };

        Ok(Self {
            kind,
            label,
            value,
            preview,
        })
    }
}

/// Replace the field/record delimiters fzf relies on so that values containing
/// tabs or newlines (legal in Unix paths) cannot shift or split the encoded
/// record.
fn sanitize_field(value: &str) -> String {
    value.replace(['\t', '\n', '\r'], " ")
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

pub fn select(
    entries: Vec<Entry>,
    bindings: &PickerBindings,
    preview: &PickerPreviewSettings,
    hint_state: &HintState,
) -> Result<Option<Selection>> {
    select_with_runner(
        default_runner(),
        entries,
        "smux> ",
        bindings,
        preview,
        hint_state.is_shown(),
        Some(hint_state.path()),
    )
}

pub fn select_value(prompt: &str, choices: Vec<Choice>) -> Result<Option<String>> {
    select_value_with_runner(default_runner(), prompt, choices)
}

/// Tracks whether the picker hint bar is currently shown, persisted in a temp
/// file so the runtime toggle survives the picker relaunching between actions.
/// Convention: the file existing means the hints are hidden.
///
/// The state file lives inside a private, randomly-named `0700` directory
/// (created via `mkdtemp`) rather than at a predictable path in the world-
/// writable temp dir, so neither the initial write nor the shell toggle that
/// creates/removes it can be redirected through an attacker-planted symlink.
pub struct HintState {
    // Held only for its `Drop`, which removes the directory and the file in it.
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl HintState {
    pub fn new(initially_shown: bool) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("smux-hints-")
            .tempdir()
            .context("failed to create hint state directory")?;
        let path = dir.path().join("state");
        // "Hidden" is represented by the file existing, so only create it when
        // the hints should start hidden.
        if !initially_shown {
            fs::write(&path, b"").with_context(|| format!("failed to write {}", path.display()))?;
        }
        Ok(Self { _dir: dir, path })
    }

    pub fn is_shown(&self) -> bool {
        !self.path.exists()
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

struct TempInputFile {
    path: PathBuf,
}

impl TempInputFile {
    fn new(contents: &str) -> Result<Self> {
        use std::io::Write;

        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock should be after unix epoch")?
            .as_nanos();
        path.push(format!("smux-fzf-{}-{nanos}.tsv", std::process::id()));
        // create_new (O_CREAT|O_EXCL) refuses to follow or reuse a pre-existing
        // path, defeating a symlink planted at this name; the temp dir's sticky
        // bit then prevents another user from replacing the file afterwards.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        file.write_all(contents.as_bytes())
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
    shell_quote_str(&path.to_string_lossy())
}

fn shell_quote_str(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn cat_command(file: &TempInputFile) -> String {
    format!("cat {}", file.shell_quoted_path())
}

fn filter_command(file: &TempInputFile, kind: &str) -> String {
    if kind == "project" {
        format!(
            "awk -F '\\t' '$1 == \"project\" || $1 == \"project-broken\"' {}",
            file.shell_quoted_path()
        )
    } else {
        format!(
            "awk -F '\\t' '$1 == \"{kind}\"' {}",
            file.shell_quoted_path()
        )
    }
}

const HINT_DIM: &str = "\x1b[2m";
const HINT_KEY: &str = "\x1b[1m";
const HINT_RESET: &str = "\x1b[0m";

/// Render a configured fzf key token compactly (`ctrl-x` -> `^x`, `alt-h` -> `⌥h`).
fn pretty_key(token: &str) -> String {
    if let Some(rest) = token.strip_prefix("ctrl-") {
        format!("^{rest}")
    } else if let Some(rest) = token.strip_prefix("alt-") {
        format!("⌥{rest}")
    } else {
        token.to_owned()
    }
}

/// A `<key> <label>` hint with the key emphasised and the label dimmed.
fn hint_segment(key: &str, label: &str) -> String {
    format!("{HINT_KEY}{key}{HINT_RESET}{HINT_DIM} {label}{HINT_RESET}")
}

fn join_hints(segments: &[String]) -> String {
    segments.join(&format!("{HINT_DIM} · {HINT_RESET}"))
}

/// Compact, dimmed hint bar for the main picker: actions on the left, scope
/// filters on the right, separated by a faint divider.
fn render_picker_hints(bindings: &PickerBindings) -> String {
    let actions = join_hints(&[
        hint_segment("↵", "open"),
        hint_segment(&pretty_key(&bindings.delete_session), "del"),
        hint_segment(&pretty_key(&bindings.save_project), "save"),
        hint_segment(&pretty_key(&bindings.rename_session), "ren"),
        hint_segment(&pretty_key(&bindings.edit_project), "edit"),
    ]);
    let filters = join_hints(&[
        hint_segment(&pretty_key(&bindings.reset), "all"),
        hint_segment(&pretty_key(&bindings.sessions), "sess"),
        hint_segment(&pretty_key(&bindings.folders), "dirs"),
        hint_segment(&pretty_key(&bindings.projects), "proj"),
    ]);
    format!("{actions}{HINT_DIM}  │  filter: {HINT_RESET}{filters}")
}

/// Build the toggle-header key binding. When a state-file path is given, the
/// toggle also flips that file (file present = hidden) via `execute-silent`, so
/// the choice persists across picker relaunches.
fn toggle_hints_bind(key: &str, state_path: Option<&Path>) -> String {
    match state_path {
        Some(path) => {
            let quoted = shell_quote(path);
            format!(
                "{key}:toggle-header+execute-silent(sh -c 'if [ -e \"$1\" ]; then rm -f \"$1\"; else : > \"$1\"; fi' _ {quoted})"
            )
        }
        None => format!("{key}:toggle-header"),
    }
}

fn render_template_hints() -> String {
    join_hints(&[
        hint_segment("↵", "pick"),
        hint_segment("^x", "all"),
        hint_segment("^t", "templates"),
    ])
}

fn add_common_picker_args(
    args: &mut Vec<String>,
    prompt: &str,
    header: &str,
    bindings: &str,
    toggle_bind: &str,
) {
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
        "--bind".to_owned(),
        toggle_bind.to_owned(),
        "--with-nth".to_owned(),
        "3".to_owned(),
        // Match the visible label (field 3) and the underlying value/kind, but
        // not the hidden preview (field 4), so typing matches what is shown.
        "--nth".to_owned(),
        "1,2,3".to_owned(),
        "--prompt".to_owned(),
        prompt.to_owned(),
        "--no-sort".to_owned(),
    ]);
}

fn add_preview_args(args: &mut Vec<String>, preview: &PickerPreviewSettings) {
    let session_preview = preview.sessions.as_deref().unwrap_or(
        "tmux list-panes -s -t \"$SMUX_PREVIEW_SESSION\" -F \"#{window_index}\t#{window_name}\t#{?window_active,1,0}\t#{pane_index}\t#{?pane_active,1,0}\t#{pane_current_command}\t#{pane_current_path}\" 2>/dev/null | awk -F '\t' 'BEGIN { current = \"\"; first_pane = 1; esc = sprintf(\"%c\", 27); reset = esc \"[0m\"; window_box = esc \"[38;2;26;33;36;48;2;149;192;202m\"; window_active_box = esc \"[38;2;26;33;36;48;2;81;156;174m\"; pane_box = esc \"[38;2;26;33;36;48;2;231;198;100m\"; pane_active_box = esc \"[38;2;26;33;36;48;2;243;150;96m\"; window_name = esc \"[38;2;149;192;202m\"; window_name_active = esc \"[1;38;2;118;204;224m\"; pane_text = esc \"[38;2;231;198;100m\"; pane_text_active = esc \"[1;38;2;243;150;96m\"; path_color = esc \"[2;38;2;149;192;202m\" } { if ($1 != current) { if (NR > 1) print \"\"; box = ($3 == \"1\" ? window_active_box : window_box); name = ($3 == \"1\" ? window_name_active : window_name); printf \"%s %s %s %s%s%s\\n\", box, $1, reset, name, $2, reset; current = $1; first_pane = 1 } pbox = ($5 == \"1\" ? pane_active_box : pane_box); ptext = ($5 == \"1\" ? pane_text_active : pane_text); printf \"\\n  %s %s %s %s%s%s\\n\", pbox, $4, reset, ptext, $6, reset; printf \"    %s%s%s\\n\", path_color, $7, reset }'",
    );
    let folder_preview = preview.folders.as_deref().unwrap_or(
        "if command -v eza >/dev/null 2>&1; then eza --tree --level=2 --color=always --icons=always \"$SMUX_PREVIEW_PATH\"; else ls -la \"$SMUX_PREVIEW_PATH\"; fi",
    );
    let project_preview = preview.projects.as_deref().unwrap_or(
        "if command -v bat >/dev/null 2>&1; then bat --style=plain --color=always --language=toml \"$SMUX_PREVIEW_FILE\"; else sed -n '1,200p' \"$SMUX_PREVIEW_FILE\"; fi",
    );
    let session_preview = shell_quote_str(session_preview);
    let folder_preview = shell_quote_str(folder_preview);
    let project_preview = shell_quote_str(project_preview);
    let preview_command = format!(
        "SMUX_SESSION_PREVIEW={session_preview} SMUX_FOLDER_PREVIEW={folder_preview} SMUX_PROJECT_PREVIEW={project_preview} sh -c 'kind=\"$1\"; value=\"$2\"; extra=\"$3\"; case \"$kind\" in session) SMUX_PREVIEW_KIND=\"$kind\" SMUX_PREVIEW_SESSION=\"$value\" sh -lc \"$SMUX_SESSION_PREVIEW\" ;; folder) SMUX_PREVIEW_PATH=\"$value\" SMUX_PREVIEW_KIND=\"$kind\" sh -lc \"$SMUX_FOLDER_PREVIEW\" ;; project|project-broken) if [ -n \"$extra\" ] && [ -f \"$extra\" ]; then SMUX_PREVIEW_KIND=\"$kind\" SMUX_PREVIEW_FILE=\"$extra\" sh -lc \"$SMUX_PROJECT_PREVIEW\"; else printf \"No preview available\\n\"; fi ;; *) printf \"No preview available\\n\" ;; esac' _ {{1}} {{2}} {{4}}"
    );
    args.extend([
        "--preview".to_owned(),
        preview_command,
        "--preview-window".to_owned(),
        "right:55%".to_owned(),
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
        &render_template_hints(),
        &format!(
            "ctrl-x:change-prompt(template> )+clear-query+reload({all_command}),ctrl-t:change-prompt(template> )+clear-query+reload({template_command})"
        ),
        "?:toggle-header",
    );
    let output = runner
        .run_capture_with_input("fzf", &args, &input)
        .context("failed to launch fzf")?;

    // 130 = interrupted (Esc/Ctrl-C), 1 = accepted with no matching item.
    // Both mean "no selection", not a failure. Only 2 (and anything else) is a
    // genuine fzf error.
    if matches!(output.status.code, Some(1) | Some(130)) {
        return Ok(None);
    }

    if !output.status.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            bail!("fzf exited with status {:?}", output.status.code);
        }
        bail!("fzf exited with status {:?}: {stderr}", output.status.code);
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
    bindings: &PickerBindings,
    preview: &PickerPreviewSettings,
    show_hints: bool,
    hint_state_path: Option<&Path>,
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
        &render_picker_hints(bindings),
        &format!(
            "{reset}:change-prompt(smux> )+clear-query+reload({all_command}),{sessions}:change-prompt(session> )+clear-query+reload({session_command}),{folders}:change-prompt(folder> )+clear-query+reload({folder_command}),{projects}:change-prompt(project> )+clear-query+reload({project_command})",
            reset = bindings.reset,
            sessions = bindings.sessions,
            folders = bindings.folders,
            projects = bindings.projects,
        ),
        &toggle_hints_bind(&bindings.toggle_hints, hint_state_path),
    );
    // The hint bar is always passed to fzf; when it should start hidden we hide
    // it at launch so the toggle key can reveal it on demand.
    if !show_hints {
        args.extend(["--bind".to_owned(), "start:toggle-header".to_owned()]);
    }
    add_preview_args(&mut args, preview);
    args.extend([
        "--expect".to_owned(),
        format!(
            "{},{},{},{}",
            bindings.delete_session,
            bindings.save_project,
            bindings.rename_session,
            bindings.edit_project
        ),
    ]);
    let output = runner
        .run_capture_with_input("fzf", &args, &input)
        .context("failed to launch fzf")?;

    // 130 = interrupted (Esc/Ctrl-C), 1 = accepted with no matching item.
    // Both mean "no selection", not a failure. Only 2 (and anything else) is a
    // genuine fzf error.
    if matches!(output.status.code, Some(1) | Some(130)) {
        return Ok(None);
    }

    if !output.status.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            bail!("fzf exited with status {:?}", output.status.code);
        }
        bail!("fzf exited with status {:?}: {stderr}", output.status.code);
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
                key if key == bindings.delete_session => SelectAction::Delete,
                key if key == bindings.save_project => SelectAction::SaveProject,
                key if key == bindings.rename_session => SelectAction::Rename,
                key if key == bindings.edit_project => SelectAction::Edit,
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
    use crate::config::{IconMode, PickerBindings, PickerPreviewSettings};
    use crate::ui::DisplayStyle;

    #[test]
    fn entry_round_trip() {
        let entry = Entry {
            kind: EntryKind::Directory,
            label: "dir      /tmp/example".to_owned(),
            value: "/tmp/example".to_owned(),
            preview: None,
        };

        let decoded = Entry::decode(&entry.encode()).expect("entry should decode");
        assert_eq!(decoded, entry);
    }

    #[test]
    fn encode_neutralizes_delimiters_in_value_and_label() {
        let entry = Entry {
            kind: EntryKind::Directory,
            label: "dir\tweird\nlabel".to_owned(),
            value: "/tmp/has\ttab\nand-newline".to_owned(),
            preview: None,
        };

        let encoded = entry.encode();
        // Exactly four tab-delimited fields on a single line, regardless of the
        // tabs/newlines that were present in the value and label.
        assert_eq!(encoded.lines().count(), 1);
        assert_eq!(encoded.matches('\t').count(), 3);

        let decoded = Entry::decode(&encoded).expect("entry should decode");
        assert_eq!(decoded.kind, EntryKind::Directory);
        assert_eq!(decoded.value, "/tmp/has tab and-newline");
        assert_eq!(decoded.label, "dir weird label");
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
            &PickerBindings::default(),
            &PickerPreviewSettings::default(),
            true,
            None,
        )
        .expect("selection should succeed");

        assert!(result.is_some());
        let recorded = runner.recorded();
        assert_eq!(recorded[0].program, "fzf");
        assert!(recorded[0].args.contains(&"--ansi".to_owned()));
        assert!(recorded[0].args.contains(&"reverse".to_owned()));
        assert!(recorded[0].args.contains(&"3".to_owned()));
        assert!(recorded[0].args.contains(&"1,2,3".to_owned()));
        assert!(recorded[0].args.contains(&"--expect".to_owned()));
        assert!(recorded[0].args.contains(&"--preview".to_owned()));
        assert!(
            recorded[0]
                .args
                .contains(&"ctrl-x,alt-s,ctrl-r,ctrl-e".to_owned())
        );
        // The hint bar is restyled (ANSI-decorated), so assert on stable
        // visible fragments rather than the whole literal line.
        assert!(recorded[0].args.iter().any(|arg| {
            arg.contains("↵") && arg.contains(" open") && arg.contains("^x") && arg.contains(" del")
        }));
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("^p") && arg.contains(" proj"))
        );
        // The scope-filter group is labelled to distinguish it from actions.
        assert!(recorded[0].args.iter().any(|arg| arg.contains("filter:")));
        // `?` toggles the hint bar's visibility.
        assert!(recorded[0].args.iter().any(|arg| arg == "?:toggle-header"));
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
                .any(|arg| arg.contains("ctrl-p:change-prompt(project> )+clear-query+reload("))
        );
        assert_eq!(
            recorded[0].stdin.as_deref(),
            Some("folder\t/tmp/example\tdir      /tmp/example\t\n")
        );
        let selection = result.expect("selection should be present");
        assert_eq!(selection.action, SelectAction::Open);
        assert_eq!(selection.entry.kind, EntryKind::Directory);
    }

    #[test]
    fn selector_treats_no_match_exit_as_no_selection() {
        // fzf exits 1 when the user accepts with no matching item; that is a
        // clean "no selection", not an error.
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: false,
                code: Some(1),
            },
            stdout: Vec::new(),
            stderr: Vec::new(),
        }));

        let result = select_with_runner(
            runner,
            vec![Entry::session(
                DisplayStyle::from_icon_mode(IconMode::Never),
                "demo".to_owned(),
            )],
            "smux> ",
            &PickerBindings::default(),
            &PickerPreviewSettings::default(),
            true,
            None,
        )
        .expect("no-match exit should not be an error");

        assert!(result.is_none());
    }

    #[test]
    fn hidden_hints_default_binds_start_toggle_header() {
        for (show_hints, expect_start_bind) in [(true, false), (false, true)] {
            let runner = Arc::new(FakeCommandRunner::new());
            runner.push_capture(Ok(CommandOutput {
                status: CommandStatus {
                    success: true,
                    code: Some(0),
                },
                stdout: b"folder\t/tmp/example\tdir      /tmp/example\n".to_vec(),
                stderr: Vec::new(),
            }));

            let _ = select_with_runner(
                runner.clone(),
                vec![Entry::directory(
                    DisplayStyle::from_icon_mode(IconMode::Never),
                    "/tmp/example".to_owned(),
                )],
                "smux> ",
                &PickerBindings::default(),
                &PickerPreviewSettings::default(),
                show_hints,
                None,
            )
            .expect("selection should succeed");

            let recorded = runner.recorded();
            let has_start_bind = recorded[0]
                .args
                .iter()
                .any(|arg| arg == "start:toggle-header");
            assert_eq!(
                has_start_bind, expect_start_bind,
                "show_hints = {show_hints}"
            );
        }
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
            &PickerBindings::default(),
            &PickerPreviewSettings::default(),
            true,
            None,
        )
        .expect("selection should succeed")
        .expect("selection should be present");

        assert_eq!(result.action, SelectAction::Delete);
        assert_eq!(result.entry.kind, EntryKind::Session);
        assert_eq!(result.entry.value, "demo");
    }

    #[test]
    fn selector_supports_save_project_action_for_sessions() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"alt-s\nsession\tdemo\tsession  demo\n".to_vec(),
            stderr: Vec::new(),
        }));

        let result = select_with_runner(
            runner,
            vec![Entry::session(
                DisplayStyle::from_icon_mode(IconMode::Never),
                "demo".to_owned(),
            )],
            "smux> ",
            &PickerBindings::default(),
            &PickerPreviewSettings::default(),
            true,
            None,
        )
        .expect("selection should succeed")
        .expect("selection should be present");

        assert_eq!(result.action, SelectAction::SaveProject);
        assert_eq!(result.entry.kind, EntryKind::Session);
        assert_eq!(result.entry.value, "demo");
    }

    #[test]
    fn selector_supports_edit_action_for_projects() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"ctrl-e\nproject\tmyapp\tproject  myapp\n".to_vec(),
            stderr: Vec::new(),
        }));

        let result = select_with_runner(
            runner,
            vec![Entry::project(
                DisplayStyle::from_icon_mode(IconMode::Never),
                "myapp".to_owned(),
                "myapp".to_owned(),
                None,
            )],
            "smux> ",
            &PickerBindings::default(),
            &PickerPreviewSettings::default(),
            true,
            None,
        )
        .expect("selection should succeed")
        .expect("selection should be present");

        assert_eq!(result.action, SelectAction::Edit);
        assert_eq!(result.entry.kind, EntryKind::Project);
        assert_eq!(result.entry.value, "myapp");
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
            &PickerBindings::default(),
            &PickerPreviewSettings::default(),
            true,
            None,
        )
        .expect("selection should succeed")
        .expect("selection should be present");

        assert_eq!(result.action, SelectAction::Open);
        assert_eq!(result.entry.kind, EntryKind::Directory);
        assert_eq!(result.entry.value, "/tmp/example");
    }

    #[test]
    fn selector_uses_configured_picker_bindings() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"\nfolder\t/tmp/example\tdir      /tmp/example\n".to_vec(),
            stderr: Vec::new(),
        }));

        let bindings = PickerBindings {
            reset: "alt-a".to_owned(),
            sessions: "alt-s".to_owned(),
            folders: "alt-f".to_owned(),
            projects: "alt-p".to_owned(),
            delete_session: "alt-x".to_owned(),
            save_project: "alt-y".to_owned(),
            rename_session: "alt-r".to_owned(),
            edit_project: "alt-e".to_owned(),
            toggle_hints: "alt-h".to_owned(),
        };

        let _ = select_with_runner(
            runner.clone(),
            vec![Entry::directory(
                DisplayStyle::from_icon_mode(IconMode::Never),
                "/tmp/example".to_owned(),
            )],
            "smux> ",
            &bindings,
            &PickerPreviewSettings::default(),
            true,
            None,
        )
        .expect("selection should succeed");

        let recorded = runner.recorded();
        assert!(
            recorded[0]
                .args
                .contains(&"alt-x,alt-y,alt-r,alt-e".to_owned())
        );
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("alt-a:change-prompt(smux> )+clear-query+reload("))
        );
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("alt-s:change-prompt(session> )+clear-query+reload("))
        );
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("alt-f:change-prompt(folder> )+clear-query+reload("))
        );
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("alt-p:change-prompt(project> )+clear-query+reload("))
        );
    }

    #[test]
    fn selector_uses_configured_folder_preview_command() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: b"\nfolder\t/tmp/example\tdir      /tmp/example\t\n".to_vec(),
            stderr: Vec::new(),
        }));

        let _ = select_with_runner(
            runner.clone(),
            vec![Entry::directory(
                DisplayStyle::from_icon_mode(IconMode::Never),
                "/tmp/example".to_owned(),
            )],
            "smux> ",
            &PickerBindings::default(),
            &PickerPreviewSettings {
                folders: Some("eza --tree --level=2 \"$SMUX_PREVIEW_PATH\"".to_owned()),
                ..Default::default()
            },
            true,
            None,
        )
        .expect("selection should succeed");

        let recorded = runner.recorded();
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("eza --tree --level=2"))
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
        assert!(recorded[0].args.contains(&"3".to_owned()));
        assert!(recorded[0].args.contains(&"1,2,3".to_owned()));
        assert!(
            recorded[0]
                .args
                .iter()
                .any(|arg| arg.contains("^t") && arg.contains(" templates"))
        );
        assert!(recorded[0].args.iter().any(|arg| arg == "?:toggle-header"));
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
