# smux

Small Rust CLI for tmux session selection and creation.

## Quickstart

Inside tmux:

```tmux
bind-key f display-popup -w 70% -h 70% -E "smux select"
```

Basic usage:

```bash
smux init
smux select
smux connect ~/code/myapp
smux connect --template rust ~/code/myapp
```

## Development

This repository uses Nix and `nix-direnv`.

```bash
direnv allow
```

After activation, the dev shell provides the Rust toolchain plus `tmux`, `fzf`, and `zoxide`.

## Status

Initial scaffold only. Product scope is defined in `SPEC.md`.
