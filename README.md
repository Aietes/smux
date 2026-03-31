[![CI](https://github.com/Aietes/smux/actions/workflows/ci.yml/badge.svg)](https://github.com/Aietes/smux/actions/workflows/ci.yml)
[![Release](https://github.com/Aietes/smux/actions/workflows/release.yml/badge.svg)](https://github.com/Aietes/smux/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**smux** is a tmux session manager with `fzf`-powered session creation and switching.

[Install](#install) • [Quick Start](#quick-start) • [Common Workflows](#common-workflows) • [Projects vs Templates](#projects-vs-templates) • [Config](#configuration) • [Commands](#commands)

## Highlights:

- quickly switch between existing tmux sessions
- tmux session from directory - pick a recent directory and create or reuse a tmux session for it
- tmux session from project - launch a saved project with a defined properties (path, name, windows, panes, layout, commands...)
- apply reusable tmux templates with windows, panes, layouts, and startup commands

It works both inside and outside tmux. Inside tmux, it fits naturally in a popup. Outside tmux, it uses the current terminal.

## Install

With Homebrew:

```bash
brew install Aietes/homebrew-smux/smux
```

With Nix:

```bash
nix profile install nixpkgs#smux
```

With Cargo:

```bash
cargo install smux-cli
```

The published crates.io package is `smux-cli`, but the installed command is still `smux`.

Runtime dependencies:

- required: `tmux`, `fzf`
- optional but recommended: `zoxide`

If `zoxide` is unavailable, `smux select` still works with tmux sessions and saved projects.

## Quick Start

Create a starter config:

```bash
smux init
```

Main config path, following the default XDG config location:

```text
~/.config/smux/config.toml
```

Project definitions live alongside it:

```text
~/.config/smux/projects/*.toml
```

`smux init` writes starter files with version-matched schema directives for editors that support TOML JSON Schema integration.

Then start using it:

```bash
smux select
```

For normal day-to-day use, wire it into `tmux`:

Recommended `tmux` settings:

```tmux
set -g detach-on-destroy off # keeps tmux running when you close a session
bind-key t display-popup -w 70% -h 70% -E "smux select"
bind-key T display-popup -w 70% -h 70% -E "smux select --choose-template"
```

To launch it in `zsh` outside of `tmux` with `Ctrl-t`:

```zsh
smux-select-widget() {
  zle push-line
  BUFFER="smux select"
  zle accept-line
}
zle -N smux-select-widget
bindkey -M emacs '^T' smux-select-widget
bindkey -M viins '^T' smux-select-widget
```

Good first commands:

```bash
smux doctor
smux select
smux save-project myapp --stdout
```

`smux select` is the main entrypoint. It opens a picker that can:

- switch to an existing tmux session
- launch a saved project
- create or reuse a session from a recent directory

Use `smux select --choose-template` when you want directory selection to be followed by an explicit template picker.

It can run inside tmux (recommended as popup), or outside tmux in the terminal.

## Common Workflows

Jump to an existing session, launch a project, or pick a directory:

```bash
smux select
```

Connect a directory and let `smux` create or reuse the matching tmux session:

```bash
smux connect ~/code/myapp
```

Force a specific template for a directory:

```bash
smux connect --template rust ~/code/myapp
```

Choose a template interactively from the selector:

```bash
smux select --choose-template
```

Export the current tmux session as a project definition:

```bash
smux save-project myapp
```

Saved projects are stored in `.config/smux/projects/`, and can be edited and adjusted. As an example the `pane` command can launch Neovim and restore the last `persistence.nvim` session:

```toml
windows = [
  { name = "my_window", cwd = "~/Development/project", panes = [
    { command = "nvim -c 'lua require(\"persistence\").load()'" },
  ] },
]
```

Preview the generated project without writing a file:

```bash
smux save-project myapp --stdout
```

## Projects Vs Templates

`smux` separates reusable layout from concrete workspace definitions:

- `template`: a reusable tmux layout with windows, panes, layouts, startup behavior, and default commands
- `project`: a concrete named workspace with a known path, optional session name, and either a template reference or its own tmux definition

Use templates when you want to reuse the same shape across many folders. Use projects when you want one named workspace that already knows where it lives and how it should start.

## Picker Behavior

The unified picker combines:

- tmux sessions
- saved projects
- `zoxide` directories

The template picker is separate and appears only when `--choose-template` is used.

Current behavior:

- prompt is shown at the top
- `Esc` cancels cleanly
- the current tmux session is highlighted when `smux select` runs inside tmux
- typing still does normal fuzzy search
- `Ctrl-C` resets to the full list
- `Ctrl-S` limits the main picker to sessions
- `Ctrl-P` limits the main picker to projects
- `Ctrl-F` limits the main picker to folders
- `Ctrl-X` closes the selected non-current tmux session and keeps the picker open

If you use a Nerd Font, `smux` can show colored icons for sessions, projects, folders, and templates.
These picker keybinds can be changed in `[settings.picker.bindings]`.

## Configuration

The main config has two top-level sections:

- `settings`
- `templates`

Project files in `projects/*.toml` define concrete workspaces.

`smux save-project` writes project files into that same directory and captures:

- `path`
- `session_name`
- `startup_window`
- `startup_pane`
- windows and pane `cwd`
- best-effort pane split direction

It intentionally does not try to export shell history or reconstruct original pane commands.

Schema files are published in this repo under `schemas/`:

- `schemas/smux-config.schema.json`
- `schemas/smux-project.schema.json`

Starter files generated by `smux init` include `#:schema` directives pointing at the matching versioned schema URLs.

Template resolution order:

1. `--template`
2. matching project definition
3. `settings.default_template`
4. built-in fallback template

Session name resolution order:

1. `--session-name`
2. matching project session name
3. sanitized directory basename

Example main config:

```toml
[settings]
default_template = "default"
icons = "auto"

[settings.icon_colors]
session = 75
directory = 108
template = 179
project = 81

[settings.picker.bindings]
reset = "ctrl-c"
sessions = "ctrl-s"
folders = "ctrl-f"
projects = "ctrl-p"
delete_session = "ctrl-x"

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
      { layout = "right 40%", command = "cargo test", zoom = true },
    ] },
]
```

Example project file:

```toml
path = "~/code/example"
session_name = "example"
template = "rust"
```

If you use `folke/persistence.nvim`, this is a practical editor window command:

```toml
{ name = "editor", command = "nvim -c 'lua require(\"persistence\").load({ last = true })'" }
```

Save that as:

```text
~/.config/smux/projects/example.toml
```

For the full config reference, see:

- [docs/configuration.md](./docs/configuration.md)
- `smux-config(5)` in generated man pages

That reference also includes layout recipes such as:

- 2x2 grid windows
- one large top pane with two bottom panes
- sidebar layouts
- vertical pane stacks

## Commands

```text
smux select [--choose-template] [--no-project-detect] [--config <path>]
smux connect [--template <name>] [--session-name <name>] [--config <path>] <path>
smux switch <session>
smux list-sessions
smux list-templates [--config <path>]
smux list-projects [--config <path>]
smux doctor [--config <path>]
smux save-project <name> [--session <name>] [--path <path>] [--stdout] [--force] [--config <path>]
smux init [--config <path>]
smux completions zsh [--dir <path>]
smux man [--dir <path>]
```

## Completions And Man Pages

zsh completions:

```bash
smux completions zsh
smux completions zsh --dir ~/.local/share/zsh/site-functions
```

man pages:

```bash
smux man
smux man --dir ~/.local/share/man/man1
```

This includes the config man page:

```text
smux-config.5
```
