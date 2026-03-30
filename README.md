# smux

Small Rust CLI for tmux session selection and creation.

`smux` is a thin Rust wrapper around:

- `tmux` for session management
- `fzf` for interactive selection
- `zoxide` for recent-directory discovery
- TOML templates for window and pane layout

It is designed to stay explicit and shell-friendly rather than becoming a TUI framework or tmux replacement.

## Install

### Development install

This repository uses Nix and `nix-direnv`.

```bash
direnv allow
cargo build
```

The dev shell provides:

- a pinned latest stable Rust toolchain
- `tmux`
- `fzf`
- `zoxide`

### Runtime requirements

`smux` expects these tools to be available at runtime:

- `tmux`
- `fzf`
- `zoxide` optional, but recommended

`zoxide` is optional because `smux select` can still work in session-only mode if `zoxide` is unavailable.

## Quickstart

Initialize the default config:

```bash
smux init
```

Default config location:

```text
~/.config/smux/config.toml
```

Inside tmux:

```tmux
bind-key f display-popup -w 70% -h 70% -E "smux select"
```

Basic usage:

```bash
smux select
smux connect ~/code/myapp
smux connect --template rust ~/code/myapp
smux doctor
```

`smux select` behaves differently depending on where it is launched:

- inside a tmux popup wrapper, it appears inside the popup
- inside a normal tmux pane, it runs `fzf` in that pane
- outside tmux, it runs `fzf` in the terminal

## Commands

```text
smux select
smux connect <path>
smux switch <session>
smux list-sessions
smux list-templates
smux list-projects
smux doctor
smux init
smux completions zsh
smux man
```

Command notes:

- `smux select` shows tmux sessions and zoxide directories in one selector
- `smux connect <path>` creates or reuses a session for a directory
- `smux switch <session>` switches to or attaches an existing session
- `smux doctor` validates runtime dependencies and config health
- `smux init` writes a starter config if one does not already exist
- `smux completions zsh` prints a zsh completion script or writes it to a directory
- `smux man` prints a man page or writes man pages to a directory

## Config

The config format is TOML with three top-level sections:

- `settings`
- `templates`
- `projects`

Example:

```toml
[settings]
default_template = "default"
icons = "auto"

[templates.default]
startup_window = "main"

[[templates.default.windows]]
name = "main"

[projects.example]
path = "~/code/example"
template = "default"
session_name = "example"
```

### `settings`

```toml
[settings]
default_template = "default"
icons = "auto"
```

- `default_template` sets the template used when no project template or CLI override applies
- `icons` controls picker icons: `auto`, `always`, or `never`

### `templates`

Templates define the tmux layout used during session creation.

```toml
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
```

Supported fields:

- template: `root`, `startup_window`
- window: `name`, `cwd`, `command`, `layout`, `panes`
- pane: `split`, `size`, `cwd`, `command`

Rules:

- a window may define `command`
- a window may define `panes`
- a window may define neither
- a window may not define both `command` and `panes`

### `projects`

Projects map known directories to template and session-name overrides.

```toml
[projects.myapp]
path = "~/code/myapp"
template = "rust"
session_name = "myapp"
```

Template resolution order:

1. `--template`
2. matching project template
3. `settings.default_template`
4. built-in fallback template

Session name resolution order:

1. `--session-name`
2. matching project session name
3. sanitized directory basename

## tmux Integration

Recommended binding:

```tmux
bind-key f display-popup -w 70% -h 70% -E "smux select"
```

That binding is optional. `smux select` can also be run directly in a pane or outside tmux.

## Icons

The selector can render Nerd Font icons for sessions, directories, and templates.

```toml
[settings]
icons = "auto"
```

Modes:

- `auto` enables icons when the terminal looks Unicode-capable
- `always` forces icons on
- `never` forces plain text labels

`smux doctor` reports the configured icon mode and the effective result. Font support itself is not detectable, so `auto` is a best-effort terminal check rather than a guarantee.

## Shell Completions

Print zsh completions to stdout:

```bash
smux completions zsh
```

Write zsh completions to a directory:

```bash
smux completions zsh --dir ./target/generated/completions
```

## Man Pages

Print the top-level man page to stdout:

```bash
smux man
```

Write man pages for the root command and subcommands to a directory:

```bash
smux man --dir ./target/generated/man
```

## Development

Useful commands during development:

```bash
direnv allow
cargo fmt
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

CI also verifies:

- formatting
- tests
- clippy
- zsh completion generation
- man page generation

## Status

Core selector, connect, config, and template workflows are implemented.
The detailed product scope and implementation notes live in `SPEC.md`.
