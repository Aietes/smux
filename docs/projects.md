# Projects

A **project** is a concrete, named workspace: a known path, an optional session
name, and a layout — either a reference to a [template](./templates.md) or its
own windows and panes. Where a template is a reusable *shape*, a project is the
real thing, pinned to a specific directory.

Projects live as **individual files** in `~/.config/smux/projects/`. The file
name is the project name:

```text
~/.config/smux/projects/myapp.toml   ->   appears in the picker as "myapp"
```

They're plain TOML, so you can edit them by hand — but the easiest way to make
one is to capture a session you've already built.

## Capture your first project

1. Open and arrange a session however you like:

   ```bash
   smux connect ~/code/myapp     # then add windows/panes, start your tools
   ```

2. Capture it into a project file:

   ```bash
   smux save-project myapp       # explicit name
   smux save-project             # name defaults to the current session
   ```

   You can also do this from the picker: select a running session and press
   `Alt-S`.

3. It now shows up in `smux select` as `myapp`, and `smux list-projects` lists
   it.

Preview the captured definition without writing a file:

```bash
smux save-project myapp --stdout
```

**What gets captured:** a version-matched `#:schema` directive, `path`,
`session_name`, `startup_window`, `startup_pane`, each window and its pane
`cwd`s, and the best-effort pane split direction. **What doesn't:** shell
history or the live commands running in panes — capture records the *shape* of
the session, not its runtime state.

You can of course also write a project by hand. A minimal one:

```toml
path = "~/code/myapp"
session_name = "myapp"
template = "rust"
```

## Project detection: open a directory, get the project

This is what makes saved projects feel automatic. When you open or `connect` a
directory whose `path` matches a project, smux uses that project — its layout
and session name — instead of treating the folder as a plain directory.

Matching is on the **normalized absolute path** (both sides are expanded), so
`~/code/myapp`, `$HOME/code/myapp`, and a relative path all resolve to the same
project. And if a session for that project is already running, smux simply
switches to it rather than rebuilding it.

To open a matched directory as a plain directory instead, run
`smux select --no-project-detect`.

## Reference a template, or inline your own layout

A project can get its layout three ways:

- **Reference a template** — `template = "rust"` reuses a shared shape. Good
  when several projects share a structure.
- **Inline its own windows** — define `windows = [...]` directly for a
  self-contained definition (this is what `save-project` always writes).
- **Use a template as a base and override session details** — point at a
  `template` while setting project-specific `path`/`session_name`.

```toml
path = "~/code/myapp"
session_name = "myapp"
startup_window = "editor"
windows = [
  { name = "editor", command = "nvim" },
  { name = "run", layout = "main-horizontal", panes = [
      { command = "cargo run" },
      { layout = "right 40%", command = "cargo test" },
    ] },
]
```

One rule to know: when a project defines its own `windows`, they **replace** the
template's windows entirely — there is no window-by-window merge.

## Reopen your editor session

Because a pane `command` is just a shell command, a project can restore your
editor exactly where you left it. With [`folke/persistence.nvim`](https://github.com/folke/persistence.nvim),
have the editor window reload the last saved Neovim session on launch:

```toml
windows = [
  { name = "editor", command = "nvim -c 'lua require(\"persistence\").load()'" },
]
```

`load()` restores the session saved for the project's directory; use
`load({ last = true })` to reopen the most recent session regardless of
directory. Combined with project detection, opening the folder now drops you
straight back into your buffers, splits, and cursor positions.

## Manage projects

- **List**: `smux list-projects`
- **Apply**: select it in the picker and press Enter, or open its directory and
  let detection do the rest
- **Update in place**: re-run `smux save-project <name> --force` after adding
  windows or panes, or press `Alt-S` on the session in the picker — both
  overwrite the existing file
- **Edit**: press `Ctrl-E` on the project in the picker to open its `.toml` in
  `$EDITOR`; when you quit the editor the picker returns. This works on broken
  projects too, so it's the quickest way to fix one
- **Rename**: rename the `.toml` file (the file name *is* the project name).
  Note that the picker's `Ctrl-R` renames a tmux *session*, not a project file
- **Delete**: press `Ctrl-X` on the project in the picker, or delete the file
- **Validate**: `smux doctor` reports config and project validity;
  `smux doctor --fix` refreshes schema directives after an upgrade

A broken project file (bad path, unknown template reference) stays **visible but
inactive** in the picker, so you can spot it and fix it (`Ctrl-E`) instead of
having it silently disappear.

## Field summary

- `path` — required; expanded and normalized before matching
- `session_name` — optional
- `template` — optional; must refer to an existing template
- `root`, `startup_window`, `startup_pane` — optional
- `windows` — optional; an inline array, same structure as a template's windows

For the full field reference and behavior notes, see
[docs/configuration.md](./configuration.md#project-definitions) — and
[Projects vs Templates](../README.md#projects-vs-templates) for when to use
which.

## See also

- [docs/templates.md](./templates.md) — reusable layouts and smart selection
- [docs/configuration.md](./configuration.md#project-definitions) — full project
  field reference
- `smux-config(5)` — the configuration reference as a man page
