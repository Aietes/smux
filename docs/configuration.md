# Configuration Reference

`smux` reads a single TOML config file.

Default path:

```text
~/.config/smux/config.toml
```

If `XDG_CONFIG_HOME` is set, `smux` uses:

```text
$XDG_CONFIG_HOME/smux/config.toml
```

## Structure

The config has three top-level sections:

- `settings`
- `templates`
- `projects`

Example:

```toml
[settings]
default_template = "default"
icons = "auto"

[settings.icon_colors]
session = 75
directory = 108
template = 179

[templates.default]
startup_window = "main"

[[templates.default.windows]]
name = "main"

[templates.rust]
startup_window = "editor"

[[templates.rust.windows]]
name = "editor"
command = "nvim"

[[templates.rust.windows]]
name = "run"
layout = "main-horizontal"

[[templates.rust.windows.panes]]
command = "cargo run"

[[templates.rust.windows.panes]]
split = "vertical"
command = "cargo test"

[projects.example]
path = "~/code/example"
template = "rust"
session_name = "example"
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

These values are ANSI-256 color indexes used for picker icons.

## `[templates.<name>]`

Templates describe the tmux layout applied when `smux` creates a new session.

Example:

```toml
[templates.rust]
startup_window = "editor"

[[templates.rust.windows]]
name = "editor"
cwd = "~/code/example"
command = "nvim"

[[templates.rust.windows]]
name = "run"
layout = "main-horizontal"

[[templates.rust.windows.panes]]
command = "cargo run"

[[templates.rust.windows.panes]]
split = "vertical"
size = "40%"
command = "cargo test"
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
- `windows`
  - type: array
  - required
  - must contain at least one window

### `[[templates.<name>.windows]]`

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
- `layout`
  - type: string
  - optional
- `panes`
  - type: array
  - optional

Rules:

- a window may define `command`
- a window may define `panes`
- a window may define neither
- a window may not define both `command` and `panes`
- if `panes` is present, it must not be empty

### `[[templates.<name>.windows.panes]]`

Pane fields:

- `split`
  - type: `horizontal` | `vertical`
  - optional
- `size`
  - type: string
  - optional
- `cwd`
  - type: string
  - optional
- `command`
  - type: string
  - optional

## `[projects.<name>]`

Projects map known directories to template and session-name overrides.

Example:

```toml
[projects.myapp]
path = "~/code/myapp"
template = "rust"
session_name = "myapp"
```

Fields:

- `path`
  - type: string
  - required
  - expanded and normalized before matching
- `template`
  - type: string
  - optional
  - must refer to an existing template
- `session_name`
  - type: string
  - optional

## Resolution Order

### Template resolution

When creating or connecting to a session, template resolution order is:

1. `--template`
2. matching project `template`
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
- project `template` references must exist
- each template must contain at least one window
- `startup_window` must refer to an existing template window
- a window cannot define both `command` and `panes`
- a `panes` array cannot be empty
- project paths must be expandable and valid

## Picker Behavior

The unified picker combines:

- tmux sessions
- zoxide directories

The template picker is separate and appears only when `--choose-template` is used.

Current behavior:

- prompt is shown at the top
- `Esc` cancels cleanly
- typing `session` narrows to sessions
- typing `folder` narrows to directories
- typing `template` narrows template choices in the template picker

## Related Docs

- CLI overview: `README.md`
- design notes: `docs/design.md`
- distribution notes: `docs/distribution.md`
