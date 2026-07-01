# Templates

A **template** is a reusable tmux layout — the windows, panes, splits, and
startup commands that make up a workspace. Define a shape once and smux can
apply it to any folder, so a new session always opens the way you work.

Each template is a **file in `~/.config/smux/templates/`**, one template per
file, where the file name (without `.toml`) is the template name. (Projects work
the same way, as individual files in `~/.config/smux/projects/`. A template is a
reusable *shape*; a project is a concrete workspace that already knows its path
and which template — or layout — it uses. See
[Projects vs Templates](../README.md#projects-vs-templates).)

## Create your first template

1. Create `~/.config/smux/templates/dev.toml` — the file name is the template
   name.
2. Give it a layout:

   ```toml
   startup_window = "editor"
   windows = [
     { name = "editor", command = "nvim" },
     { name = "shell" },
   ]
   ```

3. Confirm smux sees it and the config is valid:

   ```bash
   smux list-templates   # should print: dev
   smux doctor           # validates templates and reports problems
   ```

4. Apply it to a directory:

   ```bash
   smux connect --template dev ~/code/anything
   ```

That's the whole loop: add a file, list, apply. There is no command to scaffold
one — templates are authored by hand, though `smux init` drops in a couple of
starters (`default`, `rust`) you can copy from.

## Anatomy of a template

A slightly fuller example — `~/.config/smux/templates/rust.toml`, with a split
window:

```toml
startup_window = "editor"
startup_pane = 0
windows = [
  { name = "editor", cwd = "~/code/example", command = "nvim" },
  { name = "run", layout = "main-horizontal", panes = [
      { command = "cargo run" },
      { layout = "right 40%", command = "cargo test", zoom = true },
    ] },
]
```

- **Template-level**: `startup_window` (which window to focus), `startup_pane`
  (zero-based pane index within it), and the required `windows` array.
- **Window**: `name` (required), plus optional `cwd`, `command`, `pre_command`
  (runs first in each pane), `layout` (a tmux layout name), `synchronize`
  (mirror typing across panes), `zoom`, and a nested `panes` array.
- **Pane**: optional `layout` (`<position>` or `<position> <size>`, where
  position is `right` / `left` / `top` / `bottom`), `cwd`, and `command`.

A few rules worth remembering (smux validates these on load):

- a window has **either** a `command` **or** `panes`, never both;
- `startup_window` must name a real window in the template;
- at most one pane per window may set `zoom = true`.

For the exhaustive field-by-field reference, the pane-vs-window layout
interaction, and ready-made layout recipes (2×2 grid, sidebar, vertical stack,
…), see [docs/configuration.md](./configuration.md#template-files) — the same
content ships in the `smux-config(5)` man page.

## How smux picks a template

When you open a directory, smux resolves a template in this order:

1. an explicit `--template <name>`
2. a matching saved project's template
3. `default_template` from `[settings]`
4. **smart auto-detection** from the folder's marker files
5. the built-in fallback (a single plain window running your shell)

The interesting parts are 4 and 5.

## Smart selection

This is what makes templates feel effortless: most of the time you never pick
one by hand.

### Auto-detection by project type

If a folder looks like a known kind of project and you have a template *named to
match*, smux applies it automatically. Detection keys off marker files:

| Marker file in the folder        | Template name smux looks for |
| -------------------------------- | ---------------------------- |
| `Cargo.toml`                     | `rust`                       |
| `package.json`                   | `node`                       |
| `go.mod`                         | `go`                         |
| `pyproject.toml`                 | `python`                     |
| `requirements.txt`               | `python`                     |
| `Gemfile`                        | `ruby`                       |
| `pom.xml`                        | `java`                       |
| `build.gradle`                   | `java`                       |

Detection only fires when a template with that name actually exists. Create
`templates/rust.toml`, open any folder containing a `Cargo.toml`, and you land in
your Rust workspace — no flag, no prompt.

### Ask only when it helps

If none of steps 1–4 resolve a template but you have **two or more** templates
defined, smux opens a quick template chooser instead of dropping you into a bare
session — you decide in the moment, with the folder already in hand. With one or
no templates, it just opens. `smux select --choose-template` forces the chooser
every time.

### Getting the most out of it

The single most useful tip: **leave `default_template` unset.** A default always
wins at step 3, so setting one suppresses both auto-detection and the chooser —
every folder gets the same layout. Instead:

- define per-type templates named after the markers you work with (`rust`,
  `node`, `go`, `python`, …);
- let smux route folders by type automatically;
- fall back to the chooser for the folders it can't classify.

Name your templates after how you work, and folders open themselves.

## Managing templates

- **List** them: `smux list-templates`
- **Set a default**: `default_template = "dev"` under `[settings]` — but note it
  turns off smart selection (see above)
- **Change or remove** one: edit or delete its `templates/<name>.toml` file
- **Validate**: `smux doctor` checks every template; `smux doctor --fix`
  refreshes schema directives after an upgrade

There is no "save as template" command. `smux save-project` captures a *running*
session, but into a **project** file, not a template. If you've arranged a
layout live and want to turn it into a reusable template, a handy shortcut is to
preview that capture and adapt it by hand:

```bash
smux save-project scratch --stdout   # prints the captured windows/panes as TOML
```

The `windows`/`panes` structure it prints is the same shape a template uses, so
you can save it as `templates/<name>.toml` and generalize the paths.

## The built-in fallback

When nothing resolves and there's nothing to choose, smux uses a built-in
template: one window named `main`, running your shell, in the target directory.
In the template chooser it shows up as `<builtin>`, so you can always pick a
plain session on purpose.

## See also

- [docs/configuration.md](./configuration.md#template-files) — full field
  reference, layout interaction, and recipes
- `smux-config(5)` — the same reference as a man page
- [Projects vs Templates](../README.md#projects-vs-templates) — when to use which
