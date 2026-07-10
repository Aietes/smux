---
name: smux-config
description: >-
  Author, edit, and debug smux configuration — tmux templates and projects
  stored as per-file TOML under ~/.config/smux/. Covers the template/project
  schema, marker-based auto-detection (match / match_dependencies / priority),
  validation with `smux doctor`, and fixing common errors. Use when creating or
  editing smux templates or projects, wiring up auto-detection for a project
  type, debugging why smux opens the wrong (or no) layout, or resolving a
  `smux doctor` "invalid template/project" report.
---

# Authoring smux templates and projects

smux is a tmux session manager. A **template** is a reusable window/pane layout;
a **project** is a concrete named workspace (a path, plus either a template
reference or an inline layout). Both are individual TOML files. `config.toml`
holds only `[settings]` — templates and projects are **never** inline in it
(smux errors on load if a `[templates.*]` table appears there).

## File layout

```
~/.config/smux/
  config.toml            # [settings] only
  templates/<name>.toml  # one template per file; file stem = template name
  projects/<name>.toml   # one project per file; file stem = project name
```

Always write to `~/.config/smux/templates/` or `~/.config/smux/projects/`. The
file name (without `.toml`) is the name smux uses — there is no `name` field.

## Editing a live config safely

You are writing to the user's real `~/.config/smux`. Before changing anything:

- Run `smux list-templates` / `smux list-projects` to see what already exists.
- **Read an existing file before overwriting it** — never blind-clobber a
  hand-tuned template or project. Prefer editing the specific fields in question.
- When creating something new, pick a name that isn't already taken (or confirm
  you mean to replace it — the file name *is* the identity).
- After any change, run `smux doctor` and report what changed.

## Workflow (do this every time)

1. Write the `.toml` file to the correct directory.
2. Run `smux doctor --fix` — validates every template/project and adds/refreshes
   the `#:schema` directive on the file. Do **not** hand-write the `#:schema`
   line; let doctor manage it.
3. If doctor reports the file under "invalid templates"/"invalid projects", read
   the error, fix (see "Errors → fixes"), and re-run. A broken file is skipped
   (it won't crash smux), but it won't be usable until valid.
4. Confirm it's picked up: `smux list-templates` / `smux list-projects`.
5. If the template is meant to **auto-detect** a project type, verify it actually
   wins for a representative folder: `smux detect <dir>` prints every matching
   template, ranked, with the markers that matched, and marks the one smux would
   apply with an arrow (`→`). Adjust `priority` / `match` / `match_dependencies`
   until your template is the arrow-marked winner. This checks detection without
   launching a session.
6. Or apply it directly: `smux connect --template <name> <path>`.

## Template grammar

`~/.config/smux/templates/<name>.toml`. Only `windows` is required.

| Field                | Type        | Notes |
|----------------------|-------------|-------|
| `match`              | `[string]`  | Marker files that auto-select this template. Bare filenames or simple globs (`*`, `?`), e.g. `"Cargo.toml"`, `"nuxt.config.*"`. **No path separators, no empty strings** (rejected on load). Matched against entries directly inside the folder. |
| `match_dependencies` | `[string]`  | `package.json` dependency names (object keys), e.g. `"react"`. Use for types with no marker file. |
| `priority`           | integer     | Tie-breaker, default `0`. Higher wins. Meta-frameworks use higher (`next`/`nuxt` = 20 over `react`/`vue` = 10). |
| `root`               | string      | Base cwd applied to windows/panes that don't set their own `cwd`. |
| `startup_window`     | string      | Name of the window to focus. Must match a real window `name`. |
| `startup_pane`       | integer     | 0-based pane index within `startup_window`. |
| `env`                | table       | Environment variables set on the session, e.g. `env = { AWS_PROFILE = "dev" }`. Applied via `tmux new-session -e` (needs tmux ≥ 3.2). Keys must not contain `=`. |
| `on_create`          | string      | Shell command run **once, in the session root, before the session is created** (e.g. `"docker compose up -d"`). Runs with `env` applied and blocks; a failure aborts the connect and no session is created. Not re-run when re-attaching to an existing session. |
| `windows`            | `[window]`  | **Required**, ≥1. |

**Window** (`name` required):

| Field         | Type       | Notes |
|---------------|------------|-------|
| `name`        | string     | Required. Unique within the template. Must not contain `:` or `.`. |
| `cwd`         | string     | Working directory (supports `~`). |
| `command`     | string     | Command to run. **A window has either `command` or `panes`, never both.** |
| `pre_command` | string     | Runs first in each pane before the pane/window command. |
| `layout`      | string     | tmux window layout name passed to `tmux select-layout`: `tiled`, `main-horizontal`, `main-vertical`, `even-horizontal`, `even-vertical`. Applied *after* panes are created. |
| `synchronize` | bool       | Mirror typing across panes (default false). |
| `panes`       | `[pane]`   | ≥1. Mutually exclusive with `command`. |

**Pane** (all optional except a non-first pane needs a `layout`):

| Field     | Type   | Notes |
|-----------|--------|-------|
| `layout`  | string | `<position>` or `<position> <size>`, position ∈ `right`/`left`/`top`/`bottom`. Size is a percent or cell count, e.g. `"right 40%"`, `"bottom 12"`, `"left"`. Controls how the pane is split at creation. Every pane **after the first** must set one. |
| `command` | string | Command to run in the pane. |
| `cwd`     | string | Working directory. |
| `zoom`    | bool   | Zoom this pane after creation. **At most one pane per window** may set `zoom = true`. |

Note: `zoom` and `synchronize` live in different places — `synchronize` is a
**window** field, `zoom` is a **pane** field. There is no window-level `zoom`.

**`pane.layout` vs `window.layout`:** `pane.layout` sets each split's direction
and size as panes are created; `window.layout` (if set) runs `tmux select-layout`
*afterward* and may rearrange the final geometry. Set `window.layout` when you
want tmux to normalize the arrangement (e.g. `tiled`, `main-horizontal`); omit it
to preserve your exact split sequence from the `pane.layout` values.

### Template example

```toml
match = ["Cargo.toml"]
startup_window = "editor"
startup_pane = 0
env = { RUST_LOG = "debug" }        # optional: session environment (tmux >= 3.2)
on_create = "docker compose up -d"  # optional: runs once in the session root before creation
windows = [
  { name = "editor", command = "nvim" },
  { name = "cargo", layout = "main-horizontal", panes = [
      { command = "cargo run" },
      { layout = "right 40%", command = "cargo test", zoom = true },
    ] },
]
```

### Auto-detection via package.json dependencies

For types identified by a dependency rather than a marker file:

```toml
# templates/react.toml
match_dependencies = ["react"]
priority = 10
startup_window = "editor"
windows = [
  { name = "editor", command = "nvim" },
  { name = "dev", command = "npm run dev" },
]
```

## Project grammar

`~/.config/smux/projects/<name>.toml`. Only `path` is required. A project either
**references a template** or **defines an inline layout** (the same
`root`/`startup_window`/`startup_pane`/`windows` fields a template uses).

| Field            | Type   | Notes |
|------------------|--------|-------|
| `path`           | string | **Required.** Project directory (supports `~`). |
| `session_name`   | string | tmux session name; defaults to the file/dir name. |
| `template`       | string | Name of a template in `templates/`. |
| `env`            | table  | Merged over the template's `env`; project entries win on conflicts. |
| `on_create`      | string | Overrides the template's `on_create`. |
| `root`, `startup_window`, `startup_pane`, `windows` | | Inline layout, same shape as a template. Use *instead of* `template` for a one-off layout. |

```toml
# projects/api.toml — reference a template
path = "~/code/api"
session_name = "api"
template = "rust"
```

```toml
# projects/notes.toml — inline layout, no template
path = "~/notes"
startup_window = "main"
windows = [{ name = "main", command = "nvim ." }]
```

If a project defines its own `windows`, they **replace** the referenced
template's windows entirely — there is no window-by-window merge. Use a
`template` reference for the shared shape, or inline `windows` for a one-off; a
project that mixes both takes the inline `windows`.

If a project's `template` points at a name that isn't a valid template, `smux
doctor` flags it. "failed to load; run `smux doctor`" means the template file
exists but is broken; "was not found" means no such file. A project whose `path`
doesn't exist yet is still valid and listed.

## How smux resolves a template for a folder

1. explicit `--template <name>`
2. a matching saved project's `template`
3. `settings.default_template` (leave **unset** to keep auto-detection on)
4. **auto-detection** — templates whose `match` files or `match_dependencies`
   are present. When several match:
   1. highest `priority`
   2. then a **dependency** match outranks a **file** match
   3. then the longest matched pattern
   4. then the alphabetically first template name
5. built-in fallback (one window, your shell)

Detection reads the folder's `package.json` once; there is no built-in marker
list — a template's `match`/`match_dependencies` **is** how you extend detection.

In the picker, `Ctrl-T` on a folder forces the template chooser even when one
would auto-detect; `smux select --choose-template` forces it for the whole
session.

## Useful commands

smux commands operate on `~/.config/smux` by default — no flag needed for normal
use. To target a different config root, pass the global `-c`/`--config <path>`
flag (either position works: `smux -c <path> doctor` or `smux doctor --config <path>`).

- `smux list-templates` / `smux list-projects` — what smux currently sees
  (`--json` for machine-readable output)
- `smux detect <dir>` — show (and debug) which template auto-detects for a folder,
  ranked, with the matched markers; the `→` entry is the one smux would apply.
  `--quiet` prints only the winning name and exits 1 on no match — ideal for scripts
- `smux doctor` — validate everything; `smux doctor --fix` — also refresh `#:schema`
- `smux connect --template <name> <path>` — apply a template to a folder
- `smux save-project <session> --stdout` — print a running session's captured
  windows/panes as TOML (same shape a template uses — a good starting point to
  generalize into a `templates/<name>.toml`)
- `smux prune` — kill detached tmux sessions (see the reload gotcha below)

## Recipes

### Reopen your Neovim session (persistence.nvim)

Because a pane/window `command` is just a shell command, the editor window can
restore your last Neovim session on launch. With
[`folke/persistence.nvim`](https://github.com/folke/persistence.nvim):

```toml
# restore the session saved for THIS directory
{ name = "editor", command = "nvim -c 'lua require(\"persistence\").load()'" }

# restore the most recent session regardless of directory
{ name = "editor", command = "nvim -c 'lua require(\"persistence\").load({ last = true })'" }
```

Use `load()` in a per-project template/project (each folder reopens its own
session); `load({ last = true })` is handy in a generic editor template. Escape
the inner double quotes as `\"` inside a TOML string. Combined with
auto-detection, opening the folder drops you straight back into your buffers,
splits, and cursor positions.

### Layout patterns

```toml
# 2x2 grid — let tmux tile it
{ name = "grid", layout = "tiled", panes = [
    { command = "nvim" }, { layout = "right", command = "npm run dev" },
    { layout = "bottom", command = "npm test" }, { layout = "right", command = "git status -sb" },
  ] }

# one big top pane, two below
{ name = "run", layout = "main-horizontal", panes = [
    { command = "nvim" }, { layout = "bottom 30%", command = "cargo run" },
    { layout = "right", command = "cargo test" },
  ] }

# sidebar left, work area right (no window.layout — preserve the split)
{ name = "main", panes = [
    { command = "nvim" }, { layout = "left 25%", command = "yazi" },
  ] }

# vertical stack
{ name = "stack", panes = [
    { command = "htop" }, { layout = "bottom 40%", command = "cargo watch -x test" },
    { layout = "bottom 40%", command = "tail -f log/development.log" },
  ] }
```

Run first in every pane with `pre_command`, e.g.
`pre_command = "source .venv/bin/activate"` on the window.

## Troubleshooting

- **Edited a template but reopening the folder looks unchanged.** smux **reuses an
  existing tmux session**: if a session with that name is already running, opening
  the folder just attaches to it and does **not** rebuild the layout from the
  template. Kill the session first (`tmux kill-session -t <name>`, or `smux prune`
  to kill all detached sessions), then reopen. This is the single most common
  "my changes aren't taking effect" cause.
- **Wrong template opens for a folder.** Run `smux detect <dir>` to see the ranked
  matches, then adjust `priority` / `match` / `match_dependencies` so the intended
  template is the `→` winner.
- **The picker prompts for a template on every unmatched folder.** By design when
  `default_template` is unset and two or more templates exist. Pick on demand with
  `Ctrl-T`, live with the prompt, or (turns off auto-detection) set
  `default_template`.
- **A new template/project doesn't appear in `list-*`.** The file isn't in
  `templates/`/`projects/`, has the wrong extension, or failed to load — check
  `smux doctor` "invalid templates/projects" and fix the reported error.
- **A project opens with the wrong layout.** If it defines inline `windows`, those
  **replace** the template's windows; or its `template` reference is broken
  (`doctor` will say which).
- **A pane command behaves oddly** (a word gets interpreted rather than typed).
  Pane commands are sent literally to the shell; quote/escape as needed.

## Errors → fixes

When load or `smux doctor` reports one of these (`…` = the template/project name):

| Message | Fix |
|---------|-----|
| `must contain at least one window` | Add a `windows = [...]` with ≥1 window. |
| `window "X" cannot define both command and panes` | Pick one: a `command` **or** a `panes` array. |
| `window "X" cannot define an empty panes array` | Give `panes` ≥1 entry, or use `command`. |
| `pane N in window "X" is missing a layout` | Every pane after the first needs a `layout` (split direction), e.g. `layout = "right 40%"`. |
| `window "X" may define at most one zoomed pane` | Set `zoom = true` on only one pane. |
| `references missing startup window "X"` | `startup_window` must equal a window `name`. |
| `startup_pane N is out of range for window "X" with M pane(s)` | Use a 0-based index < M. |
| `window name "X" must not contain ':' or '.'` | Rename the window (no `:` or `.`). |
| `duplicate window name "X" in template` | Window names must be unique within the template. |
| `has an empty \`match\` pattern` / `\`match\` pattern "X" must be a bare filename, not a path` | `match` entries are bare filenames/globs — no empty strings, no `/`. |
| `has an empty \`match_dependencies\` entry` | Remove the empty string. |
| `default_template "X" was not found` | Point it at an existing template, or unset it. |
| `template "X" referenced by project "Y" was not found` | Create `templates/X.toml`, or fix the `template =` name. |
| `template "X" … failed to load; run smux doctor` | The template file exists but is invalid — fix it (its own error is under "invalid templates"). |
| `templates are no longer defined in config.toml; move each [templates.<name>] block…` | Move inline `[templates.*]` into `templates/<name>.toml` files. |

## Reference

The authoritative, exhaustive docs — defer to these when unsure:

- `smux-config(5)` — installed man page: full field reference and layout rules
- Templates guide: https://github.com/Aietes/smux/blob/main/docs/templates.md
- Projects guide: https://github.com/Aietes/smux/blob/main/docs/projects.md
- Configuration reference & recipes: https://github.com/Aietes/smux/blob/main/docs/configuration.md
- README: https://github.com/Aietes/smux/blob/main/README.md

## Common mistakes to avoid

- Putting `[templates.*]` in `config.toml` — templates are separate files now.
- A window with both `command` and `panes` — pick one.
- `startup_window` naming a window that doesn't exist, or `startup_pane` out of
  range for that window.
- `match` patterns with `/` or empty strings — rejected on load.
- Expecting a dependency template to win without a `priority` — at equal priority
  a dependency match already beats a generic file marker, but set `priority` when
  a meta-framework must beat its base.
- Hand-writing or copying a stale `#:schema` line — run `smux doctor --fix` instead.
- Editing a template and expecting a running session to change — kill it first.
