# Changelog

## 0.2.0 — unreleased

### Picker UX
- `Modal::min_match_chars(n)` hides the full list until the query reaches `n`
  characters; shorter queries show only exact label/id matches (empty query:
  nothing), so sensitive lists stay out of view
- Fuzzy subsequence match (`Modal::fuzzy`, ranked best-first) alongside substring search
- `ModalOption::disabled`, `ModalOption::separator`, `ModalOption::shortcut`,
  `ModalOption::keywords`/`keyword` (matched in both search modes)
- `Modal::default_selected`, `Modal::allow_custom_input` (command-palette free text),
  `Modal::empty_message`, `Modal::footer_hint`, `Modal::max_results`
- Extended keys: `Home`/`End`, `PgUp`/`PgDn`, `Ctrl+N`/`Ctrl+P`, `Ctrl+U` clears query
- First-query-occurrence highlight (underline, no layout shift)

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
- Auto-loads `$XDG_CONFIG_HOME/r-modals/shortcuts.toml` (or
  `~/.config/r-modals/shortcuts.toml`) when present; disable with
  `Modal::auto_shortcuts(false)`
- `ShortcutSpec`, `parse_shortcuts`, `load_shortcuts_file`, `default_shortcuts_path`;
  `ModalError::Shortcuts` for parse/validation failures; reference in `examples/assets/shortcuts.toml`
- CLI: `r-modals --dmenu --shortcuts FILE` / `--no-shortcuts`

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
  the same contract as a standalone shortcut action
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

### CLI (`r-modals --dmenu`)
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
