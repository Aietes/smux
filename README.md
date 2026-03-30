# smux

Small Rust CLI for tmux session selection and creation.

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
```

## Config

The config format is TOML with three top-level sections:

- `settings`
- `templates`
- `projects`

Example:

```toml
[settings]
default_template = "default"

[templates.default]
startup_window = "main"

[[templates.default.windows]]
name = "main"

[projects.example]
path = "~/code/example"
template = "default"
session_name = "example"
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

## Development

This repository uses Nix and `nix-direnv`.

```bash
direnv allow
```

After activation, the dev shell provides the Rust toolchain plus `tmux`, `fzf`, and `zoxide`.

## Status

Core selector, connect, config, and template workflows are implemented.
The detailed product scope and implementation notes live in `SPEC.md`.
