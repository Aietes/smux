[![CI](https://github.com/Aietes/smux/actions/workflows/ci.yml/badge.svg)](https://github.com/Aietes/smux/actions/workflows/ci.yml)
[![Release](https://github.com/Aietes/smux/actions/workflows/release.yml/badge.svg)](https://github.com/Aietes/smux/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**smux** — jump to any tmux session, or spin up a full project workspace, in one keystroke.

[Install](#install) • [Quick Start](#quick-start) • [Using smux](#using-smux) • [Projects vs Templates](#projects-vs-templates) • [Config](#configuration) • [Commands](#commands)

If you live in `tmux`, you know the friction: remembering session names, rebuilding the same editor/server/git layout for every project, and fumbling `attach`/`switch` to get back to where you were.

smux is a single static binary, written in Rust, that sits as a thin layer over `tmux`, `fzf`, and `zoxide`. One keystroke opens a fuzzy picker of your running sessions, saved projects, and recent directories — hit Enter to jump in or build the workspace.

- **Jump to any running session** — search, don't memorize
- **Open a saved project in one step** — windows, panes, layout, and startup commands all restored
- **Turn any directory into a session** — pick a `zoxide` directory or folder-search hit; smux creates or reuses it
- **Reuse layouts with templates** — define a shape once, apply it to any folder

It works both inside and outside tmux: inside, it fits naturally in a popup; outside, it uses the current terminal.

https://github.com/user-attachments/assets/9025660b-31c4-4cc6-8b51-fd37ec008562

## Install

With Homebrew:

```bash
brew install Aietes/homebrew-smux/smux
```

With Nix, from the project flake:

```bash
nix profile install github:Aietes/smux
```

A nixpkgs package is [pending review](https://github.com/NixOS/nixpkgs/pull/505348); once it merges, `nix profile install nixpkgs#smux` will work too.

With Cargo:

```bash
cargo install smux-cli
```

The published crates.io package is `smux-cli`, but the installed command is still `smux`.

Runtime dependencies:

- required: `tmux`, `fzf`
- optional but recommended: `zoxide`

If `zoxide` is unavailable, `smux select` still works with tmux sessions, saved projects, and folder search.

## Quick Start

Create a starter config:

```bash
smux init
```

Main config path, following the default XDG config location:

```text
~/.config/smux/config.toml
```

Template and project definitions live alongside it, one file per definition:

```text
~/.config/smux/templates/*.toml
~/.config/smux/projects/*.toml
```

`smux init` writes starter files with version-matched schema directives for editors that support TOML JSON Schema integration.

Then start using it:

```bash
smux select
```

For day-to-day use, wire it into `tmux` (recommended settings):

```tmux
set -g detach-on-destroy off # keeps tmux running when you close a session
bind-key t display-popup -w 70% -h 70% -E "smux select"
bind-key L run-shell "smux last"          # jump to the previous session
bind-key S run-shell "smux save-project"  # save/update the current session as a project
```

These `bind-key` lines are optional `tmux` conveniences (`prefix + t/L/S`) — smux works without them. Saving a project, for example, also works from inside the picker with `Alt-S` and from the command line with `smux save-project`; the `prefix + S` binding just saves the current session without opening the picker first.

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

Check that everything is wired up:

```bash
smux doctor
```

`smux doctor` reports dependency health, config validity, and schema drift without modifying files. After upgrading smux, `smux doctor --fix` refreshes any missing or stale `#:schema` lines in your config, template, and project files.

## Using smux

You normally drive smux through its **picker**: open it (as a `tmux` popup, or by running `smux select`), fuzzy-search one combined list of your tmux sessions, saved projects, and directories, then act on the highlighted item with a keyboard shortcut. That interactive picker is how most people use smux day to day.

Every one of those actions is also a plain **command**, so you can script smux, wire it into `tmux`/shell keybindings, or drive it directly when you already know exactly what you want — without ever opening the picker.

The two sections below cover each mode: the picker first, then the command line.

### The picker

Open it with your `tmux` popup binding (see [Quick Start](#quick-start)) or by running:

```bash
smux select
```

It combines, in one fuzzy-searchable list:

- tmux sessions
- saved projects
- `zoxide` directories
- directories found under `[settings.folder_search]` roots

Type to fuzzy-match, then use a keyboard shortcut to act on the highlighted item:

- `Enter` opens it — switch to a session, launch a project, or create/reuse a session for a directory
- `Ctrl-S` / `Ctrl-P` / `Ctrl-F` limit the list to sessions / projects / folders
- `Ctrl-C` resets to the full list
- `Alt-S` saves (or updates) the selected tmux session as a project
- `Ctrl-R` renames the selected tmux session
- `Ctrl-E` opens the selected project (or broken project) file in `$EDITOR`
- `Ctrl-X` closes the selected non-current session, or deletes the selected project file
- `?` shows or hides the keyboard-shortcut hint bar
- `Esc` cancels

Actions that change something (save, rename, edit, delete) keep the picker open so you can keep working. A few niceties:

- the current tmux session is highlighted when you run `smux select` inside tmux
- sessions are ordered most-recently-active first; saved projects most-recently-updated first
- typing fuzzy-matches the visible label and the path
- opening a folder shows a template chooser automatically when the choice is ambiguous (see [Smart template selection](#smart-template-selection)); `smux select --choose-template` always shows it

If you use a Nerd Font, smux can show colored icons for sessions, projects, folders, and templates. The keybindings above are configurable under `[settings.picker.bindings]`, and the right-side preview (tmux session summary, folder listing, project TOML) under `[settings.picker.preview]`.

### From the command line

Every picker action has a direct command — handy for scripts, `tmux`/shell bindings, or when you already know the target.

Create or reuse a session for a directory:

```bash
smux connect ~/code/myapp
smux connect --template rust ~/code/myapp   # force a specific template
```

Switch sessions, jump to the previous one, or clean up detached ones:

```bash
smux switch myapp   # switch to or attach a session by name
smux last           # switch to the most recently used session
smux prune          # kill all detached sessions
```

Capture the current tmux session as a reusable project definition:

```bash
smux save-project myapp            # explicit name
smux save-project                  # name defaults to the current session
smux save-project myapp --stdout   # preview without writing a file
```

Re-running `save-project` after adding windows or panes updates the project; pass `--force` to overwrite (the `Alt-S` picker action overwrites in place). Saved projects are plain TOML in `~/.config/smux/projects/`, so you can edit them by hand — or with `Ctrl-E` in the picker. For example, a pane command can launch Neovim and restore the last `persistence.nvim` session:

```toml
windows = [
  { name = "my_window", cwd = "~/Development/project", panes = [
    { command = "nvim -c 'lua require(\"persistence\").load()'" },
  ] },
]
```

See [Commands](#commands) for the full list.

## Projects Vs Templates

`smux` separates reusable layout from concrete workspace definitions:

- `template`: a reusable tmux layout with windows, panes, layouts, startup behavior, and default commands
- `project`: a concrete named workspace with a known path, optional session name, and either a template reference or its own tmux definition

Use templates when you want to reuse the same shape across many folders. Use projects when you want one named workspace that already knows where it lives and how it should start.

### Capturing and reusing projects

The easiest way to make a project is to _capture_ a session you've already built: arrange your windows and panes, run `smux save-project`, and smux writes a reusable definition (re-run it to update in place). Opening that project — or just opening its directory, which smux recognizes automatically — rebuilds the workspace, or switches to it if it's already running.

See **[docs/projects.md](./docs/projects.md)** for the full guide to capturing and managing projects.

### Smart template selection

Templates shine because you rarely pick one by hand. Each template declares the marker files it matches with a `match` list — exact names or globs like `nuxt.config.*` — and when you open a folder, smux applies the template whose markers are present (most specific pattern wins). The templates `smux init` ships already match the common project types (`Cargo.toml` → `rust`, `package.json` → `node`, …), and there's no built-in list to work around: adding a `match` to a template *is* how you extend detection. If nothing matches but you have several templates, smux pops up a quick chooser. Teach your templates what to match, and folders open with the right workspace on their own.

See **[docs/templates.md](./docs/templates.md)** for the full guide to creating and managing templates.

## Configuration

`config.toml` holds a single `[settings]` section. Templates and projects each live in their own directory, one file per definition:

- `templates/*.toml` — reusable layouts; the file name (without `.toml`) is the template name
- `projects/*.toml` — concrete named workspaces

`smux save-project` writes project files into that same directory and captures:

- a version-matched `#:schema` directive
- `path`
- `session_name`
- `startup_window`
- `startup_pane`
- windows and pane `cwd`
- best-effort pane split direction

It intentionally does not try to export shell history or reconstruct original pane commands.

Schema files are published in this repo under `schemas/`:

- `schemas/smux-config.schema.json`
- `schemas/smux-template.schema.json`
- `schemas/smux-project.schema.json`

Starter files generated by `smux init` include `#:schema` directives pointing at the matching versioned schema URLs, so schema-aware editors validate your config, templates, and projects as you type.
Projects written by `smux save-project` include the matching project schema line too.
If those schema directives drift after an upgrade, `smux doctor --fix` can refresh them in place across all three.

Template resolution order:

1. `--template`
2. matching project definition
3. `settings.default_template`
4. auto-detected template — the template whose `match` patterns are present in the directory (e.g. a `rust` template matching `Cargo.toml`); most specific pattern wins
5. built-in fallback template

When you open a folder from the picker and steps 1–4 don't resolve a template, smux prompts you to choose one instead of silently using the built-in fallback — but only if two or more templates are defined. With one or no templates, it opens straight away.

Session name resolution order:

1. `--session-name`
2. matching project session name
3. sanitized directory basename

Example main config:

```toml
[settings]
# default_template = "default"   # force one template everywhere; leaving it unset keeps smart auto-detection on
icons = "auto"

[settings.icon_colors]
session = 75
directory = 108
template = 179
project = 81

[settings.picker]
show_hints = true

[settings.picker.bindings]
reset = "ctrl-c"
sessions = "ctrl-s"
folders = "ctrl-f"
projects = "ctrl-p"
delete_session = "ctrl-x"
save_project = "alt-s"
rename_session = "ctrl-r"
edit_project = "ctrl-e"
toggle_hints = "?"

[settings.picker.preview]
# sessions = "tmux capture-pane -p -t \"$SMUX_PREVIEW_SESSION\""
# folders = "eza --tree --level=2 --color=always --icons=always \"$SMUX_PREVIEW_PATH\""
# projects = "bat --style=plain --color=always --language=toml \"$SMUX_PREVIEW_FILE\""

[settings.folder_search]
# roots = ["~"]
# max_depth = 3
# include_hidden = false
```

Example template file, saved as `~/.config/smux/templates/rust.toml` (the file name is the template name):

```toml
match = ["Cargo.toml"]   # auto-detect this template for folders with a Cargo.toml
startup_window = "editor"
startup_pane = 0
windows = [
  { name = "editor", command = "nvim" },
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
smux last
smux prune
smux list-sessions
smux list-templates [--config <path>]
smux list-projects [--config <path>]
smux doctor [--fix] [--config <path>]
smux save-project [<name>] [--session <name>] [--path <path>] [--stdout] [--force] [--config <path>]
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

## Design Principles

- **Small and focused** — a thin orchestration layer over `tmux`, `fzf`, and `zoxide`, with no dependencies beyond them
- **Predictable** — deterministic behavior that is easy to reason about
- **User experience first** — intuitive workflows and familiar key bindings

## Alternatives

smux stands on the shoulders of the tools that came before it. If it isn't the right fit, these are all excellent:

- [**sesh**](https://github.com/joshmedeski/sesh) — a fast, mature session manager (written in Go) built on the same `fzf` + `zoxide` instincts. If you mostly want to fuzzy-jump between sessions and directories, sesh is superb.
- [**smug**](https://github.com/ivaaaan/smug) — declarative YAML session layouts as a single Go binary. Reach for it if you prefer defining every session entirely up front in config.
- [**tmuxinator**](https://github.com/tmuxinator/tmuxinator) — the original project-layout manager: mature, Ruby-based, YAML configs.

Where smux fits: it's a small, focused Rust binary that pairs a unified `fzf` picker (sessions, saved projects, and `zoxide`/folder directories in one list) with full window/pane/layout definitions — and it can capture a _running_ session into a reusable project file with `save-project`, instead of only replaying config you wrote by hand.
