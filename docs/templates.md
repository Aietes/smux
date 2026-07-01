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
one — templates are authored by hand. `smux init` gives you a head start,
dropping in a `default` template plus one for each auto-detected project type
(`rust`, `node`, `go`, `python`, `ruby`, `java`), so opening a recognized folder
applies the right layout immediately. Edit them to taste or delete the ones you
don't use.

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
4. **smart auto-detection** from each template's own `match` patterns
5. the built-in fallback (a single plain window running your shell)

The interesting parts are 4 and 5.

## Smart selection

This is what makes templates feel effortless: most of the time you never pick
one by hand.

### Auto-detection by project type

Each template can declare a `match` list — the marker files that identify its
project type. When you open a folder, smux applies the template whose markers are
present:

```toml
# templates/rust.toml
match = ["Cargo.toml"]
startup_window = "editor"
windows = [{ name = "editor", command = "nvim" }]
```

Patterns are exact filenames or simple globs (`*`, `?`), so one entry can cover a
family of config files:

```toml
# templates/nuxt.toml — these two lines are equivalent:
match = ["nuxt.config.ts", "nuxt.config.js", "nuxt.config.mjs"]
match = ["nuxt.config.*"]
```

Some project types have no distinctive config file — React and Vite-based Vue
live in `package.json` dependencies rather than a marker file. For those, a
template can also declare `match_dependencies`, and smux applies it when the
folder's `package.json` lists any of them:

```toml
# templates/react.toml
match_dependencies = ["react"]
priority = 10
```

There is **no built-in marker list** — detection is entirely driven by your
templates, so adding a `match` or `match_dependencies` to a template *is* how you
extend it. No code change, no flag, no prompt.

### When several templates match

Overlap is common: a Nuxt repo has `nuxt.config.ts` *and* `package.json`; a
Next.js repo depends on both `next` and `react`. smux resolves it in order:

1. highest **`priority`** (an integer on the template, default `0`) — this is how
   a meta-framework beats its base (`next` over `react`, `nuxt` over `vue`);
2. then the **most specific** (longest) matched pattern — so `nuxt.config.*`
   beats the generic `package.json`;
3. then the alphabetically first template name, so the result is deterministic.

### What `smux init` ships

`smux init` writes one template per common language, each with its marker already
set, so detection works out of the box:

| Template | matches on                           |
| -------- | ------------------------------------ |
| `rust`   | `Cargo.toml`                         |
| `node`   | `package.json`                       |
| `go`     | `go.mod`                             |
| `python` | `pyproject.toml`, `requirements.txt` |
| `ruby`   | `Gemfile`                            |
| `java`   | `pom.xml`, `build.gradle`            |

JavaScript framework templates are **not** scaffolded — the base `node` template
already runs `npm run dev`/`npm test`, which works for most of them. When you
want a framework-specific layout, copy one from the gallery below; it will
auto-detect as soon as the file exists. The matcher is file- and
dependency-based, reading a folder's `package.json` once with no deeper analysis.

### Framework templates to copy

Drop any of these into `~/.config/smux/templates/<name>.toml` and edit to taste.
The meta-frameworks (`next`, `nuxt`) use `priority = 20` so they win over their
base (`react`, `vue`) when both match.

```toml
# templates/react.toml
match_dependencies = ["react"]
priority = 10
startup_window = "editor"
windows = [
  { name = "editor", command = "nvim" },
  { name = "dev", layout = "main-horizontal", panes = [
      { command = "npm run dev" },
      { layout = "right 40%", command = "npm test" },
    ] },
]
```

```toml
# templates/vue.toml
match = ["vue.config.js"]
match_dependencies = ["vue"]
priority = 10
startup_window = "editor"
windows = [{ name = "editor", command = "nvim" }, { name = "dev", command = "npm run dev" }]
```

```toml
# templates/svelte.toml
match = ["svelte.config.js", "svelte.config.ts"]
match_dependencies = ["svelte", "@sveltejs/kit"]
priority = 10
startup_window = "editor"
windows = [{ name = "editor", command = "nvim" }, { name = "dev", command = "npm run dev" }]
```

```toml
# templates/angular.toml
match = ["angular.json"]
match_dependencies = ["@angular/core"]
priority = 10
startup_window = "editor"
windows = [{ name = "editor", command = "nvim" }, { name = "dev", command = "npm start" }]
```

```toml
# templates/astro.toml
match = ["astro.config.mjs", "astro.config.ts"]
match_dependencies = ["astro"]
priority = 10
startup_window = "editor"
windows = [{ name = "editor", command = "nvim" }, { name = "dev", command = "npm run dev" }]
```

```toml
# templates/next.toml
match = ["next.config.js", "next.config.ts", "next.config.mjs"]
match_dependencies = ["next"]
priority = 20
startup_window = "editor"
windows = [{ name = "editor", command = "nvim" }, { name = "dev", command = "npm run dev" }]
```

```toml
# templates/nuxt.toml
match = ["nuxt.config.ts", "nuxt.config.js", "nuxt.config.mjs"]
match_dependencies = ["nuxt"]
priority = 20
startup_window = "editor"
windows = [{ name = "editor", command = "nvim" }, { name = "dev", command = "npm run dev" }]
```

### Ask only when it helps

If none of steps 1–4 resolve a template but you have **two or more** templates
defined, smux opens a quick template chooser instead of dropping you into a bare
session — you decide in the moment, with the folder already in hand. With one or
no templates, it just opens.

You can also reach for the chooser deliberately, even when a template *would*
auto-detect:

- press **`Ctrl-T`** on a folder in the picker to open it and pick the template
  by hand (configurable via `[settings.picker.bindings] choose_template`);
- run `smux select --choose-template` to force the chooser on every folder you
  open that session.

### Getting the most out of it

The single most useful tip: **leave `default_template` unset.** A default always
wins at step 3, so setting one suppresses both auto-detection and the chooser —
every folder gets the same layout. Instead:

- give each per-type template a `match` list for the project types you work in;
- let smux route folders by type automatically;
- fall back to the chooser for the folders it can't classify.

Teach your templates what to match, and folders open themselves.

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
