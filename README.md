# smux

`smux` is a small Rust CLI for tmux session selection and creation.

It combines:

- `tmux` for session management
- `fzf` for interactive selection
- `zoxide` for recent-directory discovery
- TOML templates for tmux window and pane layouts

The goal is to stay explicit and shell-friendly. `smux` is not a tmux replacement, a general TUI framework, or a tmuxinator clone.

## What It Does

`smux` gives you one fast entrypoint for the common tmux workflow:

- jump to an existing tmux session
- pick a recent directory and create or reuse a session for it
- apply a simple template with windows, panes, layouts, and startup commands

The main command is:

```bash
smux select
```

Inside tmux, it works well in a popup. Outside tmux, it still works in the terminal.

## Install

### Runtime Dependencies

`smux` expects:

- `tmux`
- `fzf`
- `zoxide` recommended, but optional

If `zoxide` is unavailable, `smux select` still works in session-only mode.

### Download a Release

When release artifacts are published, install `smux` by downloading the matching archive from GitHub Releases and placing the `smux` binary somewhere on your `PATH`.

The release workflow also produces:

- `man` pages
- zsh completions
- `SHA256SUMS`

### Install With Nix

From the repository itself:

```bash
nix profile install .#smux
```

This now builds the packaged CLI, installs the `smux` binary, and includes the generated man pages and zsh completions in the Nix package output.

### Install With Cargo

Local source install:

```bash
cargo install --path .
```

Registry install will make sense once `smux` is published to crates.io:

```bash
cargo install smux
```

That publication path is technically ready in broad terms, but still needs a final project license before it should be pushed publicly.

### Build From Source

This repository uses Nix and `nix-direnv`.

```bash
direnv allow
cargo build --release
```

The dev shell provides:

- a pinned latest stable Rust toolchain
- `tmux`
- `fzf`
- `zoxide`

## Quickstart

Create a starter config:

```bash
smux init
```

Default config path:

```text
~/.config/smux/config.toml
```

Recommended tmux binding:

```tmux
bind-key t display-popup -w 70% -h 70% -E "smux select"
```

That gives you:

- `prefix` then `t` to open `smux select` in a tmux popup

Example zsh keybind:

```zsh
smux-select-widget() {
  smux select
  zle reset-prompt
}
zle -N smux-select-widget
bindkey '^T' smux-select-widget
```

Basic usage:

```bash
smux select
smux connect ~/code/myapp
smux connect --template rust ~/code/myapp
smux doctor
```

`smux select` behavior depends on where it is launched:

- inside a tmux popup wrapper, it appears in the popup
- inside a tmux pane, it runs `fzf` in that pane
- outside tmux, it runs `fzf` in the terminal

Canceling the picker with `Esc` exits cleanly without creating or switching anything.

The picker is configured with the prompt at the top and category-aware filtering:

- type `session` to narrow to tmux sessions
- type `folder` to narrow to directories
- type `template` in the template picker to narrow template choices

## Commands

```text
smux select [--choose-template] [--no-project-detect] [--config <path>]
smux connect [--template <name>] [--session-name <name>] [--config <path>] <path>
smux switch <session>
smux list-sessions
smux list-templates [--config <path>]
smux list-projects [--config <path>]
smux doctor [--config <path>]
smux init [--config <path>]
smux completions zsh [--dir <path>]
smux man [--dir <path>]
```

Command notes:

- `smux select` shows tmux sessions and zoxide directories in one selector
- `smux connect` creates or reuses a tmux session for a directory
- `smux switch` switches to a session inside tmux or attaches outside tmux
- `smux doctor` validates required tools, config, and reports selector source status
- `smux init` writes a starter config if one does not already exist
- `smux completions zsh` prints a zsh completion script or writes it to a directory
- `smux man` prints a man page or writes man pages to a directory

## Shell Integration

### tmux popup via `prefix t`

```tmux
bind-key t display-popup -w 70% -h 70% -E "smux select"
```

### zsh `Ctrl-T`

```zsh
smux-select-widget() {
  smux select
  zle reset-prompt
}
zle -N smux-select-widget
bindkey '^T' smux-select-widget
```

This mirrors the common `Ctrl-T` muscle memory while keeping the implementation simple.

## Example Config

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

[projects.example]
path = "~/code/example"
template = "rust"
session_name = "example"
```

## Config Overview

The config has three top-level sections:

- `settings`
- `templates`
- `projects`

Short version:

- `settings` defines defaults and picker appearance
- `templates` define tmux windows, panes, and layouts
- templates can also choose `startup_pane`, per-window `pre_command`, and per-window `synchronize`
- `projects` map known paths to template and session-name overrides
- the recommended format uses TOML 1.1 inline tables for `windows` and `panes`

Template resolution order:

1. `--template`
2. matching project template
3. `settings.default_template`
4. built-in fallback template

Session name resolution order:

1. `--session-name`
2. matching project session name
3. sanitized directory basename

For the full config reference, see:

- [docs/configuration.md](/Users/stefan/Development/smux/docs/configuration.md)
- `smux-config(5)` in generated man pages

That reference also includes practical layout recipes, including:

- 2x2 grid windows
- one large top pane with two bottom panes
- sidebar layouts
- vertical pane stacks

## Icons

The selector can render Nerd Font icons for sessions, directories, and templates.

```toml
[settings]
icons = "auto"

[settings.icon_colors]
session = 75
directory = 108
template = 179
```

Modes:

- `auto` enables icons when the terminal looks Unicode-capable
- `always` forces icons on
- `never` forces plain text labels

`icon_colors` uses ANSI-256 palette indexes, so you can tune the icon palette to your terminal theme.

`smux doctor` reports the configured icon mode, palette, and effective result. Font support itself is not reliably detectable, so `auto` is a best-effort terminal check rather than a guarantee.

## Shell Completions

Print zsh completions to stdout:

```bash
smux completions zsh
```

Write zsh completions to a directory:

```bash
smux completions zsh --dir ./target/generated/completions
```

Typical install location:

```bash
mkdir -p ~/.local/share/zsh/site-functions
smux completions zsh --dir ~/.local/share/zsh/site-functions
```

## Man Pages

Print the top-level man page to stdout:

```bash
smux man
```

Write man pages to a directory:

```bash
smux man --dir ./target/generated/man
```

This also includes the static config man page:

```text
smux-config.5
```

Typical install location:

```bash
mkdir -p ~/.local/share/man/man1
smux man --dir ~/.local/share/man/man1
```

## Release Artifacts

The GitHub release workflow builds and publishes:

- platform-specific binary archives
- `SHA256SUMS`
- generated `man` pages
- generated zsh completions

That keeps the installation story simple for users who do not want to build from source.

## Distribution Status

Current state:

- GitHub release artifacts are prepared by workflow
- local `nix profile install .#smux` is supported
- local `cargo install --path .` is supported

Still needed before public distribution:

- choose and add a project license
- add a project license and license metadata
- publish the crate for `cargo install smux`
- submit package updates to Homebrew and nixpkgs

## Development

Useful commands during development:

```bash
direnv allow
cargo fmt
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

CI verifies:

- formatting
- tests
- clippy
- zsh completion generation
- man page generation

The detailed product scope and implementation notes live in `SPEC.md` and `docs/design.md`.
