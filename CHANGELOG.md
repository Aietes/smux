# Changelog

All notable changes to `smux` are documented in this file.

The format is based on Keep a Changelog and uses semantic-versioned release headings.

## [Unreleased]

## [0.3.0] - 2026-07-01

### Changed

- **BREAKING:** templates now live in their own files under `~/.config/smux/templates/*.toml` (one template per file; the file name is the template name) instead of inline `[templates.<name>]` tables in `config.toml`. `config.toml` now holds only `[settings]`, and `smux init` scaffolds a new `templates/` directory (see the Added entry for the starter set). There is no automatic migration: move each `[templates.<name>]` block into `templates/<name>.toml` (dropping the header line). smux errors on startup if inline templates remain in `config.toml`.

- templates now drive auto-detection themselves — there is no hardcoded marker table. A template can declare `match` (marker files; exact names or simple `*`/`?` globs like `nuxt.config.*`), `match_dependencies` (`package.json` dependency names, for types without a marker file such as `react`), and `priority` (tie-breaker, default `0`). When several templates match, highest priority wins, then the most specific pattern, then name. Package.json is read once per detection, so there's no runtime overhead and no new dependency
- `smux init` now scaffolds templates for the common languages (`rust`, `node`, `go`, `python`, `ruby`, `java`), each carrying its own marker, so opening a recognized folder applies the right layout out of the box; the starter config leaves `default_template` unset so smart selection stays on. Copy-paste framework templates (React, Vue, Svelte, Angular, Astro, Next, Nuxt) live in the templates guide and auto-detect once added
- `schemas/smux-template.schema.json` and `#:schema` directives in template files, so schema-aware editors validate templates as you type
- `smux doctor` reports template count, broken templates, and template schema drift; `smux doctor --fix` refreshes template `#:schema` directives alongside config and project files
- picker action to open the selected folder with the template chooser (default `Ctrl-T`, configurable via `[settings.picker.bindings] choose_template`), overriding auto-detection for that one folder — the folder-scoped counterpart to `smux select --choose-template`

## [0.2.2] - 2026-07-01

### Added

- picker action to open the selected project (or broken project) file in `$EDITOR` (default `Ctrl-E`), configurable via `[settings.picker.bindings] edit_project`; the picker returns when the editor exits

### Changed

- `smux doctor` output is grouped into sections (Dependencies, Sources, Configuration, Schemas, Display, Folder search) with a per-check status symbol and a summary footer (`✓ all checks passed` / `⚠ N warnings` / `✗ N errors`), and reports missing required dependencies as errors

## [0.2.1] - 2026-06-30

### Changed

- opening a folder in the picker now prompts for a template automatically when no template resolves on its own (no `default_template`, no marker-file match) and two or more templates are defined, instead of silently using the built-in fallback; `smux select --choose-template` still forces the prompt every time

### Documentation

- added dedicated guides for [templates](docs/templates.md) and [projects](docs/projects.md), and refreshed the README and configuration reference to match the current picker and template behavior

## [0.2.0] - 2026-06-30

### Added

- `smux last` switches to the most recently used tmux session
- `smux prune` kills all detached tmux sessions, preserving the attached one
- picker action to rename the selected tmux session (default `Ctrl-R`)
- restyled, toggleable keyboard-shortcut hint bar; press `?` to show or hide it, with `[settings.picker] show_hints` for the initial state and `[settings.picker.bindings] toggle_hints` / `rename_session`
- template auto-detection from project marker files (`Cargo.toml`, `package.json`, `go.mod`, ...) when no explicit, project, or default template applies and a same-named template exists
- `save-project` name is now optional and defaults to the source session's name

### Changed

- default save-project picker binding moved from `Ctrl-Y` to `Alt-S` (frees `s` for the sessions filter and avoids fzf's `Ctrl-W` word-delete)
- the picker save action now updates an existing project in place instead of failing
- picker sessions are ordered most-recently-active first, saved projects most-recently-updated first
- picker fuzzy-matching now includes the visible label, not just the underlying value
- captured project layouts record pane split direction and size more faithfully
- template window names may no longer contain `:` or `.`, and must be unique
- friendlier tmux error messages, including a clear "tmux is not installed" hint

### Fixed

- picker entries containing tabs or newlines no longer corrupt the list
- pane commands are sent literally, so a command matching a tmux key name (e.g. `Up`) is typed rather than interpreted
- accepting the picker with no matching item is treated as no selection instead of an error
- project path resolution compares the query and configured paths symmetrically
- only a single `.toml` suffix is stripped from project names

### Security

- the picker hint-state file is created inside a private, randomly-named directory rather than at a predictable path in the world-writable temp directory, preventing a local symlink attack

## [0.1.10] - 2026-06-09

### Added

- bounded folder search in `smux select`, defaulting to the home directory
- `Ctrl-Y` picker action to save a selected tmux session as a project
- contextual `Ctrl-X` picker deletion for project and invalid-project files

### Fixed

- first pane working directories are now honored when creating tmux windows from pane-based templates
- config validation now catches invalid pane layouts and out-of-range startup panes before session creation

## [0.1.8] - 2026-04-01

### Added

- configurable picker previews for sessions, folders, and projects via `[settings.picker.preview]`
- built-in right-side previews for tmux sessions, folders, project files, and broken project files
- `smux doctor --fix` to refresh missing or stale `#:schema` directives in config and project files

### Changed

- `smux doctor` now uses aligned, colorized output and reports schema drift more clearly
- `smux save-project` now writes a version-matched `#:schema` directive into exported project files

## [0.1.7] - 2026-04-01

### Added

- `zoom = true` pane support for template and project panes
- `save-project` command for exporting the current tmux session into a reusable project file

### Fixed

- broken project files no longer make `smux select` fail globally; they remain visible but inactive
- project cwd expansion and tmux window ordering issues when launching multi-window projects

### Changed

- distribution docs were updated to reflect crates.io, the Homebrew tap, and nixpkgs PR status
- README was refined and now includes a `persistence.nvim` restore example

## [0.1.6] - 2026-03-31

### Added

- JSON Schema files for `config.toml` and `projects/*.toml`
- versioned schema directives in starter config and starter project files
- automated Homebrew tap updates from tagged releases

## [0.1.5] - 2026-03-31

### Fixed

- static config man page generation now works from installed binaries

## [0.1.4] - 2026-03-31

### Changed

- crates.io package name was changed to `smux-cli` while keeping the installed binary name as `smux`

## [0.1.3] - 2026-03-31

### Changed

- release line refreshed after workflow and formatting fixes

## [0.1.2] - 2026-03-31

### Changed

- release archives now include `LICENSE`
- release workflow uses current macOS runner names

## [0.1.1] - 2026-03-31

### Added

- configurable picker icon colors
- current-session highlighting in the picker
- tmux popup binding guidance and improved end-user README

### Fixed

- picker cancellation is treated as a no-op
- empty selector guidance is more actionable
