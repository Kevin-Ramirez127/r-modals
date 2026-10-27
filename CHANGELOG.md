# Changelog

## 0.2.0 — unreleased

### Security hardening
- Windows: dynamic `shell` interpolation (`{query}` / appended remainder) now
  fails closed instead of being quoted for `cmd.exe`, which cannot escape
  `%VAR%` expansion safely. Use `command` + `args = ["{query}"]`; static shell
  commands and Unix quoting are unchanged. Unix shell actions use `/bin/sh`.
- Config, pid, and history files are written atomically through a `0600`
  `create_new` sibling; symlinks/non-regular targets are refused and history
  reads are limited to regular files (1 MiB / 4096 entries).
- `ConfigFile::var` values are TOML-escaped and substituted once, preventing
  TOML injection and recursive `{token}` expansion.
- `Dialog` passes labels/titles/messages after `--`; the child parser supports
  `--` to stop option parsing (no more argument injection through titles).
- Stdin option streams are bounded (64 KiB/line, 100k options, 64 MiB total);
  control characters are stripped from option input and printed selections.
- Icons enforce a 4096×4096 / 64 MiB decode limit with checked size
  arithmetic for raw RGBA pixels.
- Auto-loaded XDG config files are ignored when group/world-writable. See
  `SECURITY.md`.

### Spawnable dialog backend
- `Dialog::menu()/ask()/confirm()/error()` + `DialogSession` spawn the current
  executable (or `.program(path)`), stream `id<TAB>label` options into a menu,
  and return a typed `DialogOutput` (`Picked`, `PickedMany`, `Text`,
  `Confirmed`, `Shown`, `Cancelled`, `Raw`)
- `dialog_main!()` dispatches `--menu`/`--dmenu`/`--stdin`, `--ask`,
  `--confirm`, and `--error` at the top of a consumer's `main`
- The same modes are available from the `r-modals` binary: stdin options,
  `--format label|id|tag|json`, `--print-id`, `--no-stream`,
  `--loading-message`, `--pid-file`, and the existing picker flags
- `DialogSession` writes/removes the pid file, maps exit `0`/`1`/`2` to
  pick/cancel/error, and parses JSON back into `ModalSelection`
- `ModalSelection` now derives `Serialize`/`Deserialize`

### Unified config lifecycle
- `ConfigFile::at(path).template(..).var(..).migrate_from(..).ensure()` creates a
  single `config.toml`, folds in the legacy split files once, and appends
  built-in entries added by a newer version without duplicating existing names
- Malformed existing files are deliberately left untouched; writes are atomic
- `{exe}` in prefix/shortcut actions resolves at spec compile time to the
  running consumer executable: raw in `command`/`args`, shell-quoted in `shell`

### Dialog titles and window runner
- `Confirm` and `Prompt` now render their title inside the borderless window;
  `.show_title(false)` hides it, and both gain `.stylesheet(..)`/`.position(..)`
- New shared `window::run_app`/`WindowSpec` backs the picker, confirm, and
  prompt, so viewport options stay consistent
- `Prompt` input uses the themed search field styling

### Picker UX
- `Tab` completes a partial registered prefix first; when there is no prefix
  completion, it completes the search field with the focused row's label
- `Left`/`Right`/`Home`/`End` move the query caret while the search field has
  text; with an empty query they still navigate rows and submenus
- `Modal::min_match_chars(n)` hides the full list until the query reaches `n`
  characters; shorter queries show only exact label/id matches (empty query:
  nothing), so sensitive lists stay out of view
- The empty-state message (".empty_message(..)") is only shown once the query
  is non-empty; an untouched search field stays blank
- Fuzzy subsequence match (`Modal::fuzzy`, ranked best-first) alongside substring search
- `ModalOption::disabled`, `ModalOption::separator`, `ModalOption::shortcut`,
  `ModalOption::keywords`/`keyword` (matched in both search modes)
- `Modal::default_selected`, `Modal::allow_custom_input` (command-palette free text),
  `Modal::empty_message`, `Modal::footer_hint`, `Modal::max_results`
- Hint arrows (`↵` run, `⇥` complete) now fall back to egui's bundled Hack
  font instead of rendering as tofu boxes
- Extended keys: `Home`/`End`, `PgUp`/`PgDn`, `Ctrl+N`/`Ctrl+P`, `Ctrl+U` clears query
- First-query-occurrence highlight (underline, no layout shift)

### Workspace placement
- `Modal::workspace(..)`, `Confirm::workspace(..)`, `Prompt::workspace(..)`, and
  `Dialog::workspace(..)` open the window on a compositor workspace; the
  builder accepts a workspace name/number or a `Workspace`
- Supported backends: Hyprland (`hyprctl`), Sway (`swaymsg`), i3 (`i3-msg`),
  niri (`niri msg`), KDE Plasma (`kdotool`, X11 fallback `wmctrl`), and
  generic X11 (`wmctrl`); detection is automatic and can be overridden with
  `Workspace::compositor(..)`
- `Workspace::special(..)` targets a Hyprland special workspace;
  `Workspace::focus(false)` moves without explicitly switching workspaces
- Placement is best-effort after the window maps; `Workspace::move_window`
  exposes the same logic for an existing window
- CLI: `--workspace NAME`, `--workspace-silent`, `--workspace-compositor NAME`

### Streaming options
- `Modal::run_streaming(rx)` opens the window immediately and fills it from a
  `Receiver<Vec<ModalOption>>`; `Modal::loading_message(text)` sets the themed
  spinner/message shown until the first batch arrives (Esc cancels)

### Configurable shortcuts
- File-driven key bindings: `Modal::shortcuts_file(path)`, `Modal::shortcuts_toml(text)`,
  `Modal::shortcut(spec)`; TOML `[[shortcut]]` entries bind a key to an option id, a
  `command` (+ `args`), or a `shell` one-liner — commands spawn detached on press
- Shortcut keys are consumed before the built-in keys (can rebind `Enter`); the
  most specific combo wins (`Ctrl+Shift+S` before `Ctrl+S`)
- Bound `option` shortcuts auto-fill the row's right-aligned hint
- `pick = "<tag>"` binds a key to picking the highlighted row while tagging the
  selection (`ModalSelection::shortcut`), so the same pick can mean something
  else (e.g. `Shift+Enter` copies the username instead of the password)
- Auto-loads `$XDG_CONFIG_HOME/r-modals/shortcuts.toml` (or
  `~/.config/r-modals/shortcuts.toml`) when present; disable with
  `Modal::auto_shortcuts(false)`
- `ShortcutSpec`, `parse_shortcuts`, `load_shortcuts_file`, `default_shortcuts_path`;
  `ModalError::Shortcuts` for parse/validation failures; reference in `examples/assets/shortcuts.toml`
- CLI: `r-modals --menu --shortcuts FILE` / `--no-shortcuts`

### New modal types
- `Confirm::new(...).message(...).run() -> bool`
- `Prompt::new(...).placeholder(...).password(...).run() -> Option<String>`
- `Modal::run_multi()` / `multi::run_multi` — Space-toggle multi-select

### Search prefixes
- Bind the start of the query to an action: `Modal::prefixes_file(path)`,
  `Modal::prefixes_toml(text)`, `Modal::prefix(spec)`; TOML `[[prefix]]`
  entries bind a prefix to a `command` (+ `args`) or a `shell` one-liner
- `Modal::prefix_action(prefix, SelectAction)` binds a prebuilt action,
  including a Rust closure that reads the remainder from `selection.query`
- Typing a registered prefix and pressing `Enter` runs its action with the
  text after the prefix (the remainder) and closes the modal without a pick —
  the same contract as a standalone shortcut action. An `=` right after the
  prefix (spaces allowed) delimits an explicit parameter, so
  `>search=Hello World` passes `Hello World` without the `=`
- Typing a partial prefix shows the closest registered prefix as a hint
  (`starts_with`, shortest completion wins); `Tab` completes the search field
  to it and leaves the cursor at the end
- `{query}` in `args`/`shell` is replaced by the remainder (shell-quoted in
  `shell`); without a placeholder the remainder is appended (a trailing
  argument for `command`, a single quoted word for `shell`). Longest matching
  prefix wins, and a bare prefix (empty remainder) still runs
- Auto-loads `$XDG_CONFIG_HOME/r-modals/prefixes.toml` (or
  `~/.config/r-modals/prefixes.toml`) when present; disable with
  `Modal::auto_prefixes(false)`
- `PrefixSpec`, `parse_prefixes`, `load_prefixes_file`, `default_prefixes_path`;
  `ModalError::Prefixes` for parse/validation failures; reference in
  `examples/assets/prefixes.toml`

### CLI (`r-modals --menu`)
- stdin-lines → picker → stdout, `--prompt/--layout/--theme/--multi/--fuzzy/`
  `--case-sensitive/--allow-custom/--history-file/--empty-message/`
  `--max-results/--print-id`; exit codes 0 pick · 1 cancel · 2 error
- `--shortcuts FILE` / `--no-shortcuts` and `--prefixes FILE` / `--no-prefixes`
  for file-configured key bindings and query prefixes

### API / robustness
- Modals are always borderless: `Theme::border`/`border_width` and
  `search_border`/`search_border_width` are removed, and
  `border`/`border-width`/`border-color` CSS is rejected
  (`border-radius` still rounds corners)
- Transparent framebuffer clear in all windows: overrides eframe's default
  `(12,12,12,180)` clear, which left a dark veil across the transparent
  margin ring and rounded corners
- Typed `ModalError::{NoOptions, Css, Backend}` (`Display + Error`;
  `From<ModalError> for String` keeps legacy `?` working)
- `Modal::spawn(callback)` non-blocking variant
- `Modal::font(name, bytes)` custom fonts; `Modal::history_file(path)`
  most-used-first sorting with pick-count persistence; `Theme::auto()`
  light/dark sniffing
