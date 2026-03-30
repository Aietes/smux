# smux v1 Specification

## Purpose

`smux` is a small Rust CLI for tmux session selection and creation.

It combines:

- `tmux` as the execution/runtime layer
- `fzf` as the interactive picker
- `zoxide` as the recent-directory source
- declarative TOML templates for session layout

`smux` is intentionally a thin orchestration layer over existing tools. It must stay small, predictable, and easy to debug.

## Product Goals

Version 1 exists to solve four core problems well:

1. Present a unified picker of existing tmux sessions and recent directories.
2. Reuse an existing session when one already matches the selected directory-derived session name.
3. Create a new session from a directory when no session exists.
4. Apply a small, validated TOML template that defines windows, panes, layouts, and startup commands.

## Non-Goals

The following are explicitly out of scope for v1:

- full tmuxinator compatibility
- YAML support
- embedded TUI framework
- plugin system
- arbitrary pane tree modeling
- arbitrary lifecycle hooks
- full tmux option passthrough at every config layer
- git worktree management
- project-local config discovery
- session persistence beyond tmux

## Core User Flows

### Select flow

Primary usage is from inside tmux, but should also work outside when no tmux session exists.

```tmux
bind-key f display-popup -E "smux select"
```

and/or zsh alieas/keybinding.

When the user runs:

```bash
smux select
```

`smux` must:

1. query tmux for existing sessions
2. query `zoxide` for recent directories
3. merge both into one picker list
4. run `fzf`
5. perform one of the following actions:
   - selected session: switch or attach to it
   - selected directory: create or reuse a session for it
   - start tmux if not running yet

### Connect flow

When the user runs:

```bash
smux connect ~/code/myapp
smux connect --template rust ~/code/myapp
```

`smux` must:

1. normalize the input path
2. resolve the session name
3. resolve the template
4. reuse the session if it already exists
5. otherwise create the session and apply the template
6. switch or attach when ready

### Switch flow

When the user runs:

```bash
smux switch myapp
```

`smux` must:

- use `tmux switch-client -t <session>` when inside tmux
- use `tmux attach-session -t <session>` when outside tmux (or create if missing)

## CLI Surface

### `smux select`

Open the unified interactive picker.

Flags:

- `--choose-template`
- `--no-project-detect`
- `--config <path>`

### `smux connect <path>`

Create or reuse a session for a directory.

Flags:

- `--template <name>`
- `--session-name <name>`
- `--config <path>`

### `smux switch <session>`

Switch to or attach to an existing tmux session.

### `smux list-sessions`

Print current tmux session names.

### `smux list-templates`

Print configured template names.

### `smux list-projects`

Print configured project entries with resolved paths.

### `smux doctor`

Validate runtime dependencies and config.

### `smux init`

Create a starter config at `~/.config/smux/config.toml` if it does not already exist.

## Runtime Dependencies

`smux` invokes the following external programs:

- `tmux`
- `fzf`
- `zoxide`

Dependency behavior:

- missing `tmux`: hard error
- missing `fzf`: hard error for `select`
- missing `zoxide`: degrade gracefully to session-only mode, with a warning

This behavior must be documented and tested.

## Development Environment

The project must use Nix for local development.

Required setup:

- a repository `flake.nix`
- `nix-direnv` integration via `.envrc`
- a dev shell that provides the Rust toolchain and required developer utilities

The environment already has Nix and `nix-direnv` available, so the project should assume those tools are present and wire the repository around them.

The dev shell should include at minimum:

- Rust toolchain (`cargo`, `rustc`, `clippy`, `rustfmt`, `rust-analyzer`)
- `pkg-config` if needed by transitive dependencies
- `tmux`
- `fzf`
- `zoxide`

If a pinned Rust toolchain is needed, it should be declared through the flake rather than relying on host-global Rust installation.

## Configuration

Configuration format is TOML.

Default config path:

```text
~/.config/smux/config.toml
```

Top-level config sections:

1. `settings`
2. `templates`
3. `projects`

### Example

```toml
[settings]
default_template = "default"

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

## Config Schema

### `[settings]`

```toml
[settings]
default_template = "default"
```

Fields:

- `default_template: string?`

### `[templates.<name>]`

```toml
[templates.default]
root = "."
startup_window = "editor"
```

Fields:

- `root: string?`
- `startup_window: string?`
- `windows: array[window]` required

### `[[templates.<name>.windows]]`

```toml
[[templates.default.windows]]
name = "editor"
cwd = "."
command = "nvim"
layout = "main-horizontal"
```

Fields:

- `name: string` required
- `cwd: string?`
- `command: string?`
- `layout: string?`
- `panes: array[pane]?`

Rules:

- a window may define `command`
- a window may define `panes`
- a window may define neither for an empty window
- a window must not define both `command` and `panes`

### `[[templates.<name>.windows.panes]]`

```toml
[[templates.default.windows.panes]]
command = "pnpm dev"

[[templates.default.windows.panes]]
split = "vertical"
size = "30%"
command = "pnpm test --watch"
cwd = "./server"
```

Fields:

- `split: "horizontal" | "vertical"?`
- `size: string?`
- `command: string?`
- `cwd: string?`

Rules:

- the first pane does not require `split`
- additional panes may specify `split`
- `size` is optional
- `size` should only be passed to tmux when supported by the chosen invocation

### `[projects.<name>]`

```toml
[projects.myapp]
path = "~/code/myapp"
template = "rust"
session_name = "myapp"
```

Fields:

- `path: string` required
- `template: string?`
- `session_name: string?`

Rules:

- project names are config-only identifiers
- project matching is by normalized absolute path

## Resolution Rules

### Template resolution

When connecting a directory, template resolution order is:

1. CLI `--template`
2. matching `project.template`
3. `settings.default_template`
4. built-in fallback template

### Session name resolution

Session name resolution order is:

1. CLI `--session-name`
2. matching `project.session_name`
3. derived name from selected path basename

### Working directory resolution

Working directory resolution order is:

1. window `cwd`
2. template `root`
3. selected directory

Relative `cwd` and `root` values are resolved relative to the selected directory.

## Session Naming Rules

Default session name is derived from the selected directory basename.

Required sanitization:

- trim surrounding whitespace
- replace spaces with `_`
- replace `:` with `_`
- replace `.` with `_`
- replace any remaining tmux-target-unsafe characters with `_`
- reject an empty final session name as an error

If the resolved session name already exists, `smux` must reuse it. v1 does not add numeric suffixes automatically.

## Template Application Rules

Session creation behavior:

- create the session detached first
- fully apply the template
- select the configured startup window, if any
- only then switch or attach

Window creation behavior:

- first window is created by `tmux new-session`
- additional windows are created by `tmux new-window`

Pane creation behavior:

- the first pane is the implicit starting pane
- each additional pane is created in declared order using `split-window`
- pane commands are applied after pane creation
- window layout is applied last

v1 must use a simple deterministic pane strategy. It must not attempt to model arbitrary nested pane trees.

## tmux Integration

All tmux interaction must go through subprocess calls. Do not use a Rust tmux crate in v1.

Allowed commands include:

- `tmux list-sessions -F "#{session_name}"`
- `tmux has-session -t <name>`
- `tmux new-session -d -s <name> -c <dir> -n <window>`
- `tmux new-window -t <session> -n <window> -c <dir>`
- `tmux split-window ...`
- `tmux send-keys ... C-m`
- `tmux select-layout ...`
- `tmux select-window ...`
- `tmux switch-client -t <session>`
- `tmux attach-session -t <session>`

Inside/outside tmux behavior:

- inside tmux: `switch-client`
- outside tmux: `attach-session`

## Interactive Picker

`smux select` must display a unified list of:

- `session`
- `directory`

Each picker entry must contain:

- entry kind
- display label
- raw internal value

Example display:

```text
session  dotfiles
session  api
dir      ~/code/myapp
dir      ~/code/admin
```

Required behavior:

- selecting a session switches or attaches to it
- selecting a directory creates or reuses a session
- `--choose-template` enables interactive template selection for directory entries
- v1 may additionally support an `fzf` expected key such as `ctrl-t`, but that is optional unless implementation cost remains low

Deduplication rule:

- sessions and directories may both appear even when a directory would resolve to an existing session
- directory selection must still reuse the existing session instead of creating a duplicate

## Built-In Fallback Template

`smux` must work without a config file.

The built-in fallback template for v1 is:

- one empty window named `main`

This is the default when no configured template resolves.

## Error Handling

Errors must be:

- clear
- actionable
- contextual

Examples:

- missing `tmux`
- missing `fzf`
- malformed config
- unknown template
- invalid project reference
- invalid template shape
- invalid selected path
- tmux subprocess failure

Example:

```text
error: template "rust" referenced by project "myapp" was not found
hint: define [templates.rust] in ~/.config/smux/config.toml
```

The CLI must return non-zero exit codes on failure.

## Recommended Rust Stack

Recommended dependencies:

- `clap`
- `serde`
- `toml`
- `anyhow` or `eyre`
- `thiserror`
- `directories` or `dirs`
- `which`

Optional:

- `camino`
- `tracing`
- `tracing-subscriber`

## Suggested Architecture

```text
src/
  main.rs
  cli.rs
  config.rs
  model.rs
  tmux.rs
  zoxide.rs
  fzf.rs
  session.rs
  templates.rs
  doctor.rs
  util.rs
```

Module responsibilities:

- `main.rs`: entrypoint and command dispatch
- `cli.rs`: clap definitions
- `config.rs`: config path resolution, loading, validation
- `model.rs`: config and picker types
- `tmux.rs`: subprocess wrappers for tmux operations
- `zoxide.rs`: recent directory discovery
- `fzf.rs`: picker input/output handling
- `session.rs`: core connect/switch orchestration
- `templates.rs`: convert template model into tmux operations
- `doctor.rs`: environment and config diagnostics
- `util.rs`: path expansion, normalization, sanitization helpers

## Recommended Data Model

```rust
struct Config {
    settings: Option<Settings>,
    templates: HashMap<String, Template>,
    projects: Option<HashMap<String, Project>>,
}

struct Settings {
    default_template: Option<String>,
}

struct Project {
    path: String,
    template: Option<String>,
    session_name: Option<String>,
}

struct Template {
    root: Option<String>,
    startup_window: Option<String>,
    windows: Vec<Window>,
}

struct Window {
    name: String,
    cwd: Option<String>,
    command: Option<String>,
    layout: Option<String>,
    panes: Option<Vec<Pane>>,
}

struct Pane {
    split: Option<SplitDirection>,
    size: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
}

enum SplitDirection {
    Horizontal,
    Vertical,
}
```

Picker model:

```rust
enum EntryKind {
    Session,
    Directory,
}

struct Entry {
    kind: EntryKind,
    label: String,
    value: String,
}
```

## Validation Rules

Validation must enforce:

- every referenced template exists
- `startup_window`, if set, exists in that template
- a template contains at least one window
- a window does not define both `command` and `panes`
- `panes`, if present, is not empty
- project template references are valid
- project paths are parseable
- pane split values are valid

Layout validation may either:

- pass through arbitrary tmux layout strings and rely on tmux errors
- or validate against a small whitelist

For v1, pass-through with clear tmux error reporting is acceptable.

## MVP Scope

v1 must include:

- `smux select`
- `smux connect`
- `smux switch`
- `smux list-sessions`
- `smux list-templates`
- `smux list-projects`
- `smux doctor`
- `smux init`
- TOML config loading
- template validation
- template application for windows, panes, layouts, and commands
- project path mapping
- deterministic session name derivation
- inside/outside tmux handling
- graceful operation when `zoxide` is unavailable
- zsh completion generation
- proper man pages for the CLI

v1 does not require:

- local per-project config discovery
- `fzf` preview pane
- rename/kill session commands
- tmuxinator import helpers
- config hot reload

## Testing Requirements

### Unit tests

Must cover:

- config parsing
- config validation
- template validation
- session name sanitization
- project path matching
- template-to-tmux command planning

### Integration tests

Use a mockable subprocess boundary.

Must cover:

- selecting an existing session switches or attaches
- selecting a directory with no session creates and then switches or attaches
- selecting a directory with an existing session reuses it
- template with single-command windows
- template with pane-based windows and layout
- unknown template error
- malformed config error

### Manual verification

- run inside tmux selector workflow
- run outside tmux
- run with missing config
- run with missing `zoxide`
- run after `smux init`

## Acceptance Criteria

The implementation is complete when all of the following are true:

1. `smux select` shows both tmux sessions and zoxide directories in one `fzf` picker.
2. Selecting a session switches or attaches correctly.
3. Selecting a directory creates or reuses a session correctly.
4. Session names default to the sanitized folder basename.
5. Templates support windows, panes, split directions, layouts, and commands.
6. Configured project paths apply template and session-name overrides automatically.
7. The tool works without shell scripts or tmuxinator-style wrappers.
8. Errors are readable and actionable.
9. Documentation covers install, config, tmux binding, and examples.
10. Proper man pages and zsh completions are generated and documented.

## Starter Config

```toml
[settings]
default_template = "default"

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

## Documentation Requirements

`README.md` should include:

- project purpose
- install instructions
- quickstart
- config reference
- tmux binding example
- shell completion usage
- man page usage or installation notes

Quickstart should include:

```tmux
bind-key f display-popup -w 70% -h 70% -E "smux select"
```

```bash
smux init
smux select
smux connect ~/code/myapp
smux connect --template rust ~/code/myapp
```

Additional deliverables:

- starter config generated by `smux init`
- tests for core parsing and session behavior
- clear user-facing error messages
- short design note in `docs/design.md`
- `flake.nix` and `.envrc` for reproducible development
- generated man pages
- generated zsh completion scripts

## Implementation Plan

### Phase 0: Project skeleton

Deliver:

- `flake.nix`
- `.envrc`
- cargo project scaffold
- `clap` command tree
- subprocess abstraction
- shared error type and result wiring

Exit criteria:

- entering the repo activates the Nix dev shell through `nix-direnv`
- binary builds
- help output is stable
- subprocess boundary is testable

### Phase 1: Session discovery and selector MVP

Deliver:

- tmux session listing
- zoxide directory listing
- unified picker entry model
- `fzf` integration
- session selection path

Exit criteria:

- `smux select` can list sessions and directories
- selecting a session switches or attaches correctly
- missing `zoxide` degrades cleanly

### Phase 2: Connect orchestration

Deliver:

- path normalization and expansion
- session name derivation and sanitization
- existing-session reuse
- inside/outside tmux attach behavior

Exit criteria:

- `smux connect <path>` works without templates
- reuse-vs-create behavior is deterministic

### Phase 3: Config loading and resolution

Deliver:

- config path resolution
- TOML parsing
- validation
- project matching
- template resolution

Exit criteria:

- config errors are surfaced clearly
- template/project selection rules match this spec

### Phase 4: Template application

Deliver:

- window creation planner
- pane creation planner
- command dispatch to tmux
- startup window selection

Exit criteria:

- multi-window templates work
- pane-based templates work
- layout application is deterministic

### Phase 5: Operator commands and docs

Deliver:

- `list-templates`
- `list-projects`
- `doctor`
- `init`
- README
- `docs/design.md`
- man pages
- zsh completions

Exit criteria:

- doctor reports dependency and config issues
- init writes the starter config safely
- docs are sufficient for first use

### Phase 6: Test hardening

Deliver:

- unit tests for config, naming, validation, and planning
- integration tests against mocked subprocesses
- manual verification checklist execution

Exit criteria:

- core happy paths and failure paths are covered
- manual tmux workflow is validated once

## Risks and Open Decisions

The draft is mostly stable. The remaining implementation decisions worth making early are:

1. Whether `ctrl-t` template selection is in v1 or deferred behind `--choose-template` only. Recommendation: make `--choose-template` required for v1, add `ctrl-t` only if trivial.
2. Whether tmux commands are executed immediately during template application or planned first and then executed. Recommendation: plan first, execute second, because it improves testability.
3. Whether to validate tmux layout strings proactively. Recommendation: pass through in v1 and surface tmux errors clearly.
4. How much shell quoting logic is owned by `smux` for `send-keys`. Recommendation: keep commands as raw strings sent to tmux, avoid inventing a shell mini-language.

## Final Guidance

`smux` should remain:

- small
- explicit
- deterministic
- easy to debug
- easy to extend

It should not become a framework.
