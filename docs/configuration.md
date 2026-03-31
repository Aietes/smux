# Configuration Reference

`smux` reads a main TOML config file plus optional project definition files.

Default path:

```text
~/.config/smux/config.toml
```

If `XDG_CONFIG_HOME` is set, `smux` uses:

```text
$XDG_CONFIG_HOME/smux/config.toml
```

Project definitions live in:

```text
~/.config/smux/projects/*.toml
```

or, when `XDG_CONFIG_HOME` is set:

```text
$XDG_CONFIG_HOME/smux/projects/*.toml
```

## Structure

The main config has two top-level sections:

- `settings`
- `templates`

Example:

```toml
[settings]
default_template = "default"
icons = "auto"

[settings.icon_colors]
session = 75
directory = 108
template = 179
project = 81

[templates.default]
startup_window = "main"
windows = [{ name = "main" }]

[templates.rust]
startup_window = "editor"
startup_pane = 0
windows = [
  { name = "editor", pre_command = "source .venv/bin/activate", command = "nvim" },
  { name = "run", synchronize = true, layout = "main-horizontal", panes = [
      { command = "cargo run" },
      { layout = "right 40%", command = "cargo test" },
    ] },
]
```

Example project file:

```toml
path = "~/code/example"
session_name = "example"
template = "rust"
```

## `[settings]`

```toml
[settings]
default_template = "default"
icons = "auto"
```

Fields:

- `default_template`
  - type: string
  - optional
  - used when neither `--template` nor a matching project provides a template
- `icons`
  - type: `auto` | `always` | `never`
  - default: `auto`
  - controls whether picker icons are shown

### `[settings.icon_colors]`

```toml
[settings.icon_colors]
session = 75
directory = 108
template = 179
project = 81
```

Fields:

- `session`
  - type: integer
  - default: `75`
- `directory`
  - type: integer
  - default: `108`
- `template`
  - type: integer
  - default: `179`
- `project`
  - type: integer
  - default: `81`

These values are ANSI-256 color indexes used for picker icons.

## `[templates.<name>]`

Templates describe the tmux layout applied when `smux` creates a new session.
The recommended and documented format uses TOML 1.1 inline tables for `windows` and nested `panes`.
That means template definitions should normally be written as `windows = [{ ... }]` with nested `panes = [{ ... }]`, rather than verbose `[[templates...]]` arrays of tables.

Example:

```toml
[templates.rust]
startup_window = "editor"
startup_pane = 0
windows = [
  { name = "editor", cwd = "~/code/example", pre_command = "source .venv/bin/activate", command = "nvim" },
  { name = "run", synchronize = true, layout = "main-horizontal", panes = [
      { command = "cargo run" },
      { layout = "right 40%", command = "cargo test" },
    ] },
]
```

Template fields:

- `root`
  - type: string
  - optional
  - currently accepted by the config schema
- `startup_window`
  - type: string
  - optional
  - must match one of the template window names if set
- `startup_pane`
  - type: integer
  - optional
  - default: `0`
  - zero-based pane index within the startup window
- `windows`
  - type: array of inline tables
  - required
  - must contain at least one window

### `windows = [{ ... }]`

Window fields:

- `name`
  - type: string
  - required
- `cwd`
  - type: string
  - optional
- `command`
  - type: string
  - optional
- `pre_command`
  - type: string
  - optional
  - runs in each pane of the window before the pane or window command
- `layout`
  - type: string
  - optional
  - tmux window layout name passed to `tmux select-layout`
  - examples: `tiled`, `main-horizontal`, `main-vertical`, `even-horizontal`, `even-vertical`
  - applied after pane creation, so it may rearrange the final pane geometry
- `synchronize`
  - type: boolean
  - default: `false`
  - enables tmux synchronized panes for that window
- `panes`
  - type: array of inline tables
  - optional

Rules:

- a window may define `command`
- a window may define `panes`
- a window may define neither
- a window may not define both `command` and `panes`
- if `panes` is present, it must not be empty
- `pre_command` runs as a separate command before the window or pane command
- `window.layout` is a tmux layout name and is passed through to tmux without additional validation

### `panes = [{ ... }]`

Pane fields:

- `layout`
  - type: string
  - optional
  - format: `<position>` or `<position> <size>`
  - supported positions: `right`, `left`, `bottom`, `top`
  - controls how each new pane is created before any window-level tmux layout is applied
- `cwd`
  - type: string
  - optional
- `command`
  - type: string
  - optional

Examples:

- `layout = "right 40%"`
- `layout = "bottom 12"`
- `layout = "left"`

Interaction between pane and window layout:

- `pane.layout` controls split direction and optional size during pane creation
- `window.layout` is applied afterward with `tmux select-layout`
- if both are set, `window.layout` may rearrange the final pane geometry
- if you want tmux to normalize the final layout, set `window.layout`
- if you want to preserve the split sequence more closely, omit `window.layout`

## Recipes

These examples are meant to show practical combinations of pane `layout` and window `layout`.

### 2x2 grid

Use `tiled` when you want a simple 2x2-style workspace and do not care about preserving a specific split sequence.

```toml
[templates.grid]
startup_window = "grid"
windows = [
  { name = "grid", layout = "tiled", panes = [
      { command = "nvim" },
      { layout = "right", command = "cargo run" },
      { layout = "bottom", command = "cargo test" },
      { layout = "right", command = "git status -sb" },
    ] },
]
```

### One large top pane, two bottom panes

This is a good fit for `main-horizontal`.

```toml
[templates.dev]
startup_window = "run"
windows = [
  { name = "run", layout = "main-horizontal", panes = [
      { command = "nvim" },
      { layout = "bottom 30%", command = "cargo run" },
      { layout = "right", command = "cargo test" },
    ] },
]
```

This gives you one dominant pane at the top and two smaller panes below it.

### Sidebar on the left, work area on the right

```toml
[templates.sidebar]
startup_window = "main"
windows = [
  { name = "main", panes = [
      { command = "nvim" },
      { layout = "left 25%", command = "yazi" },
    ] },
]
```

### Vertical stack

```toml
[templates.stack]
startup_window = "stack"
windows = [
  { name = "stack", panes = [
      { command = "htop" },
      { layout = "bottom 40%", command = "cargo watch -x test" },
      { layout = "bottom 40%", command = "tail -f log/development.log" },
    ] },
]
```

## Project Definitions

Project definitions are stored as individual files in `~/.config/smux/projects/`.
The project name comes from the file name, for example:

```text
~/.config/smux/projects/myapp.toml
```

This project appears in `smux select` as `myapp`.

Minimal project file:

```toml
path = "~/code/myapp"
template = "rust"
session_name = "myapp"
```

Fully defined project file:

```toml
path = "~/code/myapp"
session_name = "myapp"
startup_window = "editor"
windows = [
  { name = "editor", command = "nvim" },
  { name = "run", layout = "main-horizontal", panes = [
      { command = "cargo run" },
      { layout = "right 40%", command = "cargo test" },
    ] },
]
```

Project fields:

- `path`
  - type: string
  - required
  - expanded and normalized before matching
- `session_name`
  - type: string
  - optional
- `template`
  - type: string
  - optional
  - must refer to an existing template if set
- `root`
  - type: string
  - optional
- `startup_window`
  - type: string
  - optional
- `startup_pane`
  - type: integer
  - optional
- `windows`
  - type: array of inline tables
  - optional

Project behavior:

- a project may point at a template and use it as-is
- a project may define its own windows directly without using a template
- a project may use a template as a base and override it with project-specific session details
- when a project defines `windows`, they replace the template windows rather than merging window-by-window

## Resolution Order

### Template resolution

When creating or connecting to a session, template resolution order is:

1. `--template`
2. matching project definition
3. `settings.default_template`
4. built-in fallback template

### Session name resolution

Session name resolution order is:

1. `--session-name`
2. matching project `session_name`
3. sanitized directory basename

## Validation Rules

`smux` validates the config when it is loaded.

Validation includes:

- `default_template` must exist if set
- each template must contain at least one window
- `startup_window` must refer to an existing template window
- `startup_pane` must be valid for the chosen startup window
- a window cannot define both `command` and `panes`
- a `panes` array cannot be empty
- project `template` references must exist
- project paths must be expandable and valid

## Picker Behavior

The unified picker combines:

- tmux sessions
- saved projects
- zoxide directories

The template picker is separate and appears only when `--choose-template` is used.

Current behavior:

- prompt is shown at the top
- `Esc` cancels cleanly
- typing `project` narrows to saved projects
- typing `session` narrows to sessions
- typing `folder` narrows to directories
- typing `template` narrows template choices in the template picker
- `Ctrl-X` resets the picker
- `Ctrl-S` limits the main picker to sessions and keeps fuzzy search active
- `Ctrl-P` limits the main picker to projects and keeps fuzzy search active
- `Ctrl-F` limits the main picker to folders and keeps fuzzy search active
- `Ctrl-T` limits the template picker to templates and keeps fuzzy search active

## Related Docs

- CLI overview: `README.md`
- design notes: `docs/design.md`
- distribution notes: `docs/distribution.md`
