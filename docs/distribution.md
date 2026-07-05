# Distribution Notes

This document tracks the current external distribution state for `smux`.

## Channels

Current distribution channels:

- GitHub Releases
- crates.io as `smux-cli`
- Homebrew tap: `Aietes/homebrew-smux`
- nixpkgs as `smux`

## Current Status

What is already live:

- GitHub release artifacts are built by GitHub Actions
- generated man pages and zsh completions are included in release artifacts
- crates.io publication is live for `cargo install smux-cli`
- Homebrew installation is live through `brew install Aietes/homebrew-smux/smux`
- nixpkgs installation is live through `nix profile install nixpkgs#smux`
- local Nix installation still works with `nix profile install .#smux`

What is still pending:

- possible later submission to Homebrew core if the project becomes a good fit

## Install Commands

Cargo:

```bash
cargo install smux-cli
```

Homebrew tap:

```bash
brew install Aietes/homebrew-smux/smux
```

Nix:

```bash
nix profile install nixpkgs#smux
```

Nix, from this repository checkout:

```bash
nix profile install .#smux
```

Nix, from the project flake:

```bash
nix profile install github:Aietes/smux
```

## Automation

Distribution automation is intentionally split by channel:

- `Release`: GitHub release artifacts only
- `Publish crates.io`: crates.io publication only
- `Update Homebrew Tap`: Homebrew tap update only

This keeps reruns, secrets, and approvals scoped to a single distribution channel at a time.
The `Release` workflow also publishes GitHub release notes from the matching version section in [CHANGELOG.md](../CHANGELOG.md).

## crates.io

Current status:

- the package is published as `smux-cli`
- the installed binary remains `smux`
- publishing is automated through the `Publish crates.io` workflow

Operational notes:

1. the `crates-io` GitHub Actions environment must remain configured
2. the `CARGO_REGISTRY_TOKEN` environment secret must remain valid
3. release publication can be triggered by tag push or manually through the workflow UI

## Homebrew

Current status:

- the maintained installation path is the `Aietes/homebrew-smux` tap
- the tap formula is updated automatically from tagged releases

Operational notes:

1. the `HOMEBREW_TAP_PAT` secret in `Aietes/smux` must remain valid
2. tagged releases update the tap automatically
3. the workflow can also be run manually from the Actions UI

Homebrew core is not the primary path right now. It may make sense later if project adoption justifies it.

## nixpkgs

Current status:

- the project is packaged in nixpkgs as `smux`
- the initial nixpkgs PR was merged as [NixOS/nixpkgs#505348](https://github.com/NixOS/nixpkgs/pull/505348)
- the merged package version is `0.3.1`

The repository flake remains useful for local development, local installation, and users whose selected nixpkgs registry or channel has not caught up to the merged package yet.
