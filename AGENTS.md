# AGENTS.md

## Purpose

This file records repository-specific working instructions for agents operating in `smux`.

## Project Context

- `smux` is a Rust CLI utility for tmux session management.
- The current product draft lives in `SPEC.md`.
- The development environment should be managed with `nix flake` and `nix-direnv`.

## Working Rules

- Treat `SPEC.md` as the primary source of truth for product scope and behavior unless the user overrides it.
- Prefer small, explicit modules and simple subprocess-based integration with external tools.
- Keep v1 focused. Do not add speculative features outside the spec without user approval.
- When implementation begins, keep architecture, tests, and docs aligned with the current spec.
- Before the first release, prefer the correct CLI and naming over compatibility shims. Breaking changes are acceptable while the project is unreleased.

## Commits

- Use Conventional Commits for all commit messages.
- Format commit subjects as `<type>: <summary>`.
- Preferred types include `feat`, `fix`, `docs`, `refactor`, `test`, `build`, and `chore`.
- Keep the subject concise and imperative.
- Commit in small, reasonable groups as work progresses. Do not let large unrelated changes accumulate before committing.

Examples:

- `feat: add tmux session picker`
- `fix: sanitize derived session names`
- `docs: refine swux v1 specification`

## Environment Expectations

- Prefer entering the project through the repository `.envrc` and Nix dev shell once those files exist.
- Do not assume a host-global Rust toolchain.
- Keep toolchain and developer dependencies declared through the Nix flake.
- Use the latest Rust toolchain version reasonably available through the flake.
- Use the latest stable Rust edition available for new crate configuration and code.

## Safety

- Do not overwrite or narrow the spec silently; update `SPEC.md` intentionally when requirements change.
- Avoid destructive git operations unless the user explicitly requests them.
