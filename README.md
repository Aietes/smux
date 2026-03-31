# smux

`smux` is a tmux session manager with `fzf`-powered project and template selection.

It helps you:

- jump to an existing tmux session
- launch a named project from a saved project definition
- pick a recent directory and create or reuse a session for it
- apply tmux templates with windows, panes, layouts, and startup commands

`smux` works both inside and outside tmux. Inside tmux, it fits naturally in a popup. Outside tmux, it uses the current terminal.

## Install

With Homebrew:

```bash
brew install smux
```

With Nix:

```bash
nix profile install nixpkgs#smux
```

With Cargo:

```bash
cargo install smux
```

Runtime dependencies:

- `tmux`
- `fzf`
- `zoxide` recommended, but optional

If `zoxide` is unavailable, `smux select` still works with tmux sessions.

## Quickstart

Create a starter config:

```bash
smux init
```

Default config path:

```text
~/.config/smux/config.toml
```

Project definitions live in:

```text
~/.config/smux/projects/*.toml
```

Then run:

```bash
smux select
```

Useful commands:

```bash
smux select
smux connect ~/code/myapp
smux connect --template rust ~/code/myapp
smux doctor
```

`smux select` behaves like this:

- inside a tmux popup wrapper, it appears in the popup
- inside a tmux pane, it runs `fzf` in that pane
- outside tmux, it runs `fzf` in the terminal

Canceling the picker with `Esc` exits cleanly without creating or switching anything.

## tmux And zsh Bindings

Recommended tmux settings:

```tmux
set -g detach-on-destroy off
bind-key t display-popup -w 70% -h 70% -E "smux select"
bind-key T display-popup -w 70% -h 70% -E "smux select --choose-template"
```

`detach-on-destroy off` keeps tmux running when you close a session instead of dropping you out of tmux entirely.

zsh `Ctrl-T`:

```zsh
smux-select-widget() {
  smux select
  zle reset-prompt
}
zle -N smux-select-widget
bindkey '^T' smux-select-widget
```

## Picker Behavior

The picker keeps the prompt at the top and supports category-aware filtering.

Type:

- `session` to narrow to tmux sessions
- `project` to narrow to saved projects
- `folder` to narrow to directories
- `template` in the template picker to narrow template choices

Shortcuts:

- `Ctrl-X` resets to the full list
- `Ctrl-S` limits the main picker to sessions and keeps fuzzy search active
- `Ctrl-P` limits the main picker to projects and keeps fuzzy search active
- `Ctrl-F` limits the main picker to folders and keeps fuzzy search active
- `Ctrl-T` limits the template picker to templates and keeps fuzzy search active

If you use a Nerd Font, `smux` can show colored icons for sessions, projects, folders, and templates.

## Example Main Config

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

## Example Project File

```toml
path = "~/code/example"
session_name = "example"
template = "rust"
```

Save that as:

```text
~/.config/smux/projects/example.toml
```

Projects can either:

- point at a reusable template
- define their own windows, panes, commands, and startup behavior directly
- use a template as a base and override it with project-specific session details

## Config Overview

The main config has two top-level sections:

- `settings`
- `templates`

In short:

- `settings` defines defaults and picker appearance
- `templates` define tmux windows, panes, and layouts
- project files in `projects/*.toml` define concrete workspaces

Template resolution order:

1. `--template`
2. matching project definition
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
