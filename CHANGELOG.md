# Changelog

All notable changes to `smux` are documented in this file.

The format is based on Keep a Changelog and uses semantic-versioned release headings.

## [Unreleased]

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

