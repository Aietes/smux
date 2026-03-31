# Design Notes

## Goal

`smux` is a small CLI for selecting, creating, and attaching tmux sessions without introducing a separate long-running service, shell-script framework, or embedded TUI runtime.

The design goal is to stay:

- explicit
- deterministic
- easy to debug
- easy to extend

## Core Approach

`smux` is a thin orchestration layer over existing tools.

It relies on:

- `tmux` for all session and window operations
- `fzf` for interactive selection
- `zoxide` for recent-directory discovery
- TOML config for templates and project mappings

The implementation intentionally prefers subprocess integration over embedding tool-specific internals. This keeps behavior close to the user’s existing shell setup and avoids coupling to unstable internal APIs.

## Why Subprocesses

### tmux

Using tmux subprocess calls keeps the behavior transparent and easy to reproduce manually. It also avoids introducing a tmux-specific Rust dependency that would abstract away command behavior we still need to understand and debug.

### zoxide

`zoxide` is treated as a CLI dependency rather than a Rust library integration. The supported interface for `smux` is `zoxide query --list`, which matches how users already interact with zoxide and preserves graceful degradation when the tool is missing.

### fzf

`fzf` remains the selector layer. `smux select` prepares structured rows and decodes the chosen entry, but does not attempt to replace `fzf` with a custom terminal UI.

## Session Creation Model

Session creation is split into two logical stages:

1. resolve config, project mappings, session name, and effective template
2. build a deterministic session plan and execute it through tmux commands

That separation is useful because it keeps template resolution testable without requiring live tmux state, while still keeping the runtime behavior simple.

## Template Planning

Templates are converted into a `SessionPlan` before execution.

The plan includes:

- session name
- ordered windows
- pane layout instructions
- pane commands
- window layouts
- startup window

This planning step exists to make template behavior deterministic and easier to test. It also keeps tmux execution logic straightforward: execute the plan in order rather than mixing resolution and execution together.

## Config Model

The config is intentionally small and centered on three sections:

- `settings`
- `templates`
- project definition files under `projects/`

This keeps the behavior understandable:

- `settings` defines global defaults
- `templates` describe tmux layouts
- project files define concrete named workspaces with a path and optional inline tmux layout

The config path follows an XDG-style CLI convention:

```text
~/.config/smux/config.toml
```

Project definitions live alongside it:

```text
~/.config/smux/projects/*.toml
```

## CLI Semantics

The primary user-facing command is `smux select`.

It was intentionally named after the action rather than the presentation style. A popup is only one way to host the selector; outside a tmux popup wrapper, the same command still runs correctly in a normal terminal or tmux pane.

The command model is:

- `select`: interactive entrypoint
- `connect`: direct path-based session creation/reuse
- `switch`: direct session switch/attach
- `save-project`: best-effort export of a live tmux session into a project file
- `doctor`: environment and config diagnostics

`save-project` is intentionally scoped as a project exporter, not a tmux session serializer. It captures stable structural data such as the active path, startup selection, windows, panes, and recoverable split direction, but it does not try to reconstruct shell history or arbitrary original pane commands.

## Error Philosophy

Errors should be:

- direct
- contextual
- actionable

Examples:

- required dependency missing
- config parse failure
- template not found
- invalid template shape
- tmux command failure

`doctor` is stricter than normal runtime behavior: it fails when required dependencies or a present config are invalid, but it does not fail merely because config is absent or `zoxide` is unavailable.

## Testing Strategy

There are three layers of validation:

1. unit tests for config parsing, validation, naming, and planning
2. CLI tests for help and selected command behavior
3. manual smoke tests for tmux/fzf interaction

Manual smoke testing remains important because tmux popup behavior, session switching, and `fzf` interaction are inherently integration-heavy and not fully captured by unit tests alone.

## Non-Goals

The project deliberately does not try to become:

- a tmux replacement
- a general-purpose terminal UI framework
- a plugin platform
- a tmuxinator clone

Those constraints are intentional. They keep the implementation small and prevent feature sprawl from obscuring the primary workflow.
