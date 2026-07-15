# Changelog

All notable changes to `smux` are documented in this file.

The format is based on Keep a Changelog and uses semantic-versioned release headings.

## [0.5.0] - 2026-07-15

### Changed

- **BREAKING:** clone destinations now use `--dir <path>` instead of a second positional argument (`smux clone <url> --dir <path>`). The option also works without a URL, so repository-browser selections can be cloned to an explicit destination
- **BREAKING:** the published Rust library now exposes only the supported `smux::run` entry point; implementation modules are private
- the minimum supported Rust version is now 1.88, and the Nix flake uses one pinned latest-stable Rust release for development and packaging
- GitHub releases, crates.io publication, and Homebrew tap updates now wait for formatting, tests, strict Clippy, Nix packaging, and crates.io package verification against the exact release tag

### Fixed

- `smux init` refuses to overwrite any existing starter file and rolls back files created by a partially failed initialization
- failed tmux layout setup removes the incomplete session, so retrying creates the intended layout instead of attaching to a partial one
- projects that normalize to the same path are reported as invalid instead of resolving nondeterministically
- `package.json` template detection parses the standard dependency maps as JSON, handling normal whitespace and ignoring unrelated keys and values
- template and repository picker records sanitize tabs and newlines so values cannot corrupt fzf's field structure

### Security

- picker input and hint-state files now live in automatically cleaned temporary directories with explicit owner-only (`0700`) permissions

## [0.4.0] - 2026-07-10

### Added

- **`env` tables on templates and projects** — set per-workspace environment variables (`env = { AWS_PROFILE = "dev" }`) applied via `tmux new-session -e` (needs tmux >= 3.2; the flag is only emitted when env is configured). Project entries merge over the template's and win on conflicts, regardless of whether the template came from the project, auto-detection, or the built-in fallback
- **`on_create` lifecycle hook** — a shell command on a template or project that runs once, in the session root, with the session `env` applied, *before* the session is created — so services it starts (`docker compose up -d`, direnv, ...) are ready when pane commands launch. A failing hook aborts the connect with its stderr and leaves no half-built session; re-attaching does not re-run it. Projects override the template's hook
- **`smux kill [<session>]`** — kill a session by exact name; with no name, inside tmux, the client switches to the last session first and then kills the one it was on, so the terminal survives
- **`smux clone [<url>] [<dir>]`** — clone and connect in one step. With a URL it runs `git clone` and opens the checkout with template auto-detection. Without one it opens a fuzzy browser over your GitHub repositories (requires the optional `gh` CLI) showing visibility, last update, and description, then clones the selection with `gh repo clone`. `[settings.clone] root` sets where checkouts land, `owners` adds extra GitHub users/orgs to the browser, `--no-connect` just clones and prints the path, and `--template` overrides detection
- **window-level picker mode** — `Ctrl-W` (configurable as `[settings.picker.bindings] windows`) switches the picker to individual tmux windows across all sessions; Enter jumps straight to the selected window, `Ctrl-X` closes it, `Ctrl-R` renames it, and the preview shows it in session context. Windows stay out of the default list
- `smux detect --quiet` prints only the winning template name and exits 1 when nothing matches, for scripts and prompts
- `--json` on `list-sessions`, `list-templates`, and `list-projects` for machine-readable output
- the generated zsh completions now complete `smux switch <TAB>` with live tmux session names
- `--config` is now a global flag with a `-c` short form (`smux -c x doctor`); the old per-subcommand position keeps working
- `smux doctor` reports the optional `gh` dependency

### Changed

- the picker now jumps the cursor back to the top (best) match whenever you change the search query, instead of leaving it wherever you had scrolled — so refining a search always lands on the most relevant result
- the picker scans zoxide and the folder-search roots once per run instead of after every in-picker action, so it relaunches noticeably faster on large home directories
- `smux doctor` emits ANSI colors only on a terminal and honors `NO_COLOR`
- child-process failures now read "exit code 1" / "termination by signal" instead of debug-formatted values, and a missing config file suggests `smux init`
- every CLI flag now has help text
- `Cargo.toml` declares `rust-version = "1.85"` and release builds are stripped with thin LTO

### Fixed

- tmux targets now use the exact-match `=name` form, so `smux switch app` can no longer silently prefix-match a session named `app-server` (the same applied to kill, rename, and existence checks)
- sessions created outside smux with names smux would never generate (spaces, unicode) can now be opened, killed, and renamed from the picker instead of being listed but unreachable — existing tmux names are used verbatim, and a failed delete keeps the picker alive
- `smux select` without an interactive terminal fails fast instead of hanging forever
- the `choose_template` picker binding is validated like the others, so a duplicate or empty binding is rejected at load time
- `smux save-project` rewrites captured window names that would fail on reconnect (`:` and `.` become `_`, duplicates get a suffix) and remaps `startup_window` accordingly; templates also validate window names at load time so `doctor` catches bad ones
- `smux detect` on a missing or non-directory path exits with an error instead of reporting "no template matched"
- deleting the current session from the picker now explains why it is refused instead of silently doing nothing

### Security

- `smux clone` passes `--` to `git clone`, so a URL or directory starting with `-` cannot smuggle git flags (argument injection, e.g. `--upload-pack`)

### Documentation

- updated installation and distribution docs now that `smux` is packaged in nixpkgs
- documented `env`, `on_create`, `[settings.clone]`, the window picker mode, and all new flags across the README, the configuration reference, the man pages, the JSON schemas, and the bundled Claude Code skill

## [0.3.1] - 2026-07-01

### Added

- `smux detect <dir>` prints the templates whose `match` files or `match_dependencies` are present in a directory, ranked the way smux auto-selects them (the top entry, marked with an arrow, is the one it would apply). Lets you debug why a folder resolves to a given template without launching a session
- `smux skill [--dir <dir>]` writes (or prints) a bundled Claude Code skill that teaches an AI assistant how to author, validate, and debug smux templates and projects. The skill is embedded in the binary, so it always matches the installed version; typical use is `smux skill --dir ~/.claude/skills/smux`

### Fixed

- `smux list-projects` no longer aborts with an error when a project's `path` points at a directory that doesn't exist (as the starter `example.toml` does on a fresh install); the project is listed with its absolute path, consistent with how `smux doctor` and project validation treat missing paths

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
