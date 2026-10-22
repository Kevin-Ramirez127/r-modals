# r-modals

Instant modals for Rust — one call pops a centered, borderless, searchable option picker (`eframe`/`egui`, X11 + Wayland).

## Features

- **Blocking picker** — `Modal::new("Title").run()` returns the pick or `None` on cancel
- **Options** — label, description, and icons (file path, bytes, or raw RGBA)
- **Submenus** — nest options with `.sub_option(...)` for drill-down navigation
- **Actions** — run a command, shell one-liner, or Rust closure on select
- **Shortcuts** — bind keys to an option id or a terminal command/shell from a TOML file (`Modal::shortcuts_file`, `Modal::shortcuts_toml`, `Modal::shortcut`); bound keys are consumed before the built-in ones and show up as row hints
- **Prefixes** — bind the start of the query to an action from a TOML file (`Modal::prefixes_file`, `Modal::prefixes_toml`, `Modal::prefix`); type the prefix and press Enter to run its command/shell with the rest of the query, or pass a parameter explicitly with `>search=Hello World`. While typing, the closest matching prefix is hinted and `Tab` completes it; with no prefix to complete, `Tab` completes the focused row's label
- **Search** — substring or fuzzy (`Modal::fuzzy(true)`, ranked best-first) over label, id, description, and keywords, with case-sensitivity option
- **Option states** — `.disabled(true)`, `ModalOption::separator(..)`, `.shortcut("Ctrl+O")`, `.keyword(..)`
- **Picker extras** — `.default_selected(..)`, `.allow_custom_input(true)` (free text), `.empty_message(..)`, `.footer_hint(..)`, `.max_results(n)`, `.min_match_chars(n)` (hide the full list until the query is long enough), `.multi_select(true)` + `run_multi()`, `.history_file(..)` MRU sorting, `.font(..)` custom fonts
- **Dialogs** — `Confirm::new(..).run() -> bool`, `Prompt::new(..).run() -> Option<String>`; both render a visible title (`.show_title(false)` to hide) and accept `.stylesheet(..)`/`.position(..)`
- **Spawnable dialogs** — `Dialog::menu()/ask()/confirm()/error()` start the current executable as a dialog child (`dialog_main!()`), stream typed options in, and return a typed `DialogOutput`; `--menu`, `--ask`, `--confirm`, and `--error` give shell scripts the same contract
- **Unified config** — `ConfigFile` creates `config.toml`, appends built-ins once, migrates the old split files, and expands `{exe}` to the running executable
- **Layouts** — `List` rows or `Grid` image tiles
- **CSS themes** — style with real CSS via `.stylesheet(...)`; built-ins: `Theme::dark()`, `Theme::light()`, `Theme::transparent_dark()`, `Theme::auto()`
- **Menu mode** — pipe stdin to `r-modals --menu` (`--dmenu` is an alias), pick goes to stdout (exit 0 pick · 1 cancel · 2 error)

## Install

build from source:

```sh
cargo install --git https://github.com/Kevin-Ramirez127/r-modals r-modals
```

To use it as a library, add it to your `Cargo.toml`:

```toml
[dependencies]
r-modals = { git = "https://github.com/Kevin-Ramirez127/r-modals" }
```

## Usage

```rust
use r_modals::{Modal, ModalOption};

let choice = Modal::new("Launcher")
    .option(ModalOption::new("ff", "Firefox").description("Web browser"))
    .option(ModalOption::new("term", "Terminal").icon("icons/term.png"))
    .run()
    .unwrap();

if let Some(sel) = choice {
    println!("picked {} ({})", sel.label, sel.id);
}
```

Keys: type to filter, ↑/↓ navigate, Enter pick, Esc cancel. Mouse hover + click works too.
While the search field has text, `←`/`→`/`Home`/`End` edit its caret; with an empty query they navigate rows (`←`/`→` also back out of/drill into submenus). `PgUp`/`PgDn`, `Ctrl+N`/`Ctrl+P` move rows; `Ctrl+U` clears the query; `Space` toggles in `run_multi()`.

## Shortcuts

Bind keys to options or terminal commands from a TOML file:

```toml
# ~/.config/r-modals/shortcuts.toml  (auto-loaded when present)
[[shortcut]]
key = "Ctrl+O"
option = "ff"                 # activate this option (pick + fire its action)

[[shortcut]]
key = "Ctrl+Shift+T"
command = "alacritty"         # spawn a detached program…
args = ["-e", "htop"]

[[shortcut]]
key = "Ctrl+N"
shell = "notify-send hi"      # …or a shell one-liner
```

```rust
let sel = Modal::new("Launcher")
    .option(ModalOption::new("ff", "Firefox").command("firefox"))
    .shortcuts_file("~/.config/r-modals/shortcuts.toml") // or .shortcuts_toml("…")
    .run();
```

Shortcut keys are consumed before the built-in keys (so they can rebind `Enter`), and `option` shortcuts render as the row hint automatically. A missing/invalid file makes `run()` return `ModalError::Shortcuts`. CLI: `r-modals --menu --shortcuts FILE` (or `--no-shortcuts` to skip the default file). See `examples/assets/shortcuts.toml` for the full reference.

## Prefixes

Bind the start of the query to an action. Type the prefix and press `Enter`: the action runs with the rest of the query (the *remainder*) and the modal closes without a pick.

```toml
# ~/.config/r-modals/prefixes.toml  (auto-loaded when present)
[[prefix]]
prefix = ">"
command = "notify-send"        # spawn a detached program…
args = ["r-modals", "{query}"]

[[prefix]]
prefix = "?"
shell = "xdg-open https://duckduckgo.com/?q={query}"  # …or a shell one-liner
hint = "Web search"
```

```rust
let sel = Modal::new("Launcher")
    .option(ModalOption::new("ff", "Firefox").command("firefox"))
    .prefixes_file("~/.config/r-modals/prefixes.toml") // or .prefixes_toml("…")
    .run();
```

A prefix can also run a Rust closure via `Modal::prefix_action(prefix, SelectAction::Callback(..))`; the closure reads the remainder from `selection.query`.

While you type, the picker hints the closest registered prefix that starts with
the query (letter by letter, like `str::starts_with`). Press `Tab` to complete
the search field to it; the cursor moves to the end so you can keep typing the
remainder. The shortest candidate wins, so `>sh` completes to `>show items`
while `>s` completes to `>sync`. An exact prefix is never a completion, but if
it is also the start of a longer one both hints are shown (`Enter` runs the
exact prefix, `Tab` extends it). When the query does not start a partial prefix,
`Tab` completes the search field with the focused row's label instead, so
`Al` + `Tab` becomes `Alpha` while a matching prefix still takes priority.

`{query}` is replaced by the remainder. In `shell` it is inserted shell-quoted (don't wrap it in quotes yourself); when an action sets no `{query}` the remainder is appended — a trailing argument for `command`, a single quoted word for `shell`. Put an `=` right after the prefix (spaces around it are fine) to pass an explicit parameter, so `>search=Hello World` hands `Hello World` to the action just like `>search Hello World`; the `=` itself is never part of it. A bare prefix (empty remainder) still runs its action, and the longest matching prefix wins (`>>` beats `>`). A missing/invalid file makes `run()` return `ModalError::Prefixes`. CLI: `r-modals --menu --prefixes FILE` (or `--no-prefixes` to skip the default file). See `examples/assets/prefixes.toml` for the full reference.

## Dialogs

```rust
use r_modals::{Confirm, Prompt};

let name = Prompt::new("Your name?").placeholder("Jane").run().unwrap();
let ok = Confirm::new("Greet?").message("Say hello?").run().unwrap();
```

Titles are rendered inside the borderless window (there is no OS title bar), so
`Prompt::new("Master password")` is visible without extra work. Use
`.show_title(false)` to hide it, and `.stylesheet(..)`/`.position(..)` to style or
place either dialog.

## Spawning dialogs

A consumer binary can reuse its own executable as the dialog backend:

```rust
use r_modals::{Dialog, DialogOutput};

fn main() {
    // Child modes (`--menu`, `--ask`, `--confirm`, `--error`) exit here.
    r_modals::dialog_main!();

    let mut picker = Dialog::menu()
        .app_id("my-app")
        .pid_file("/tmp/my-app/menu.pid")
        .loading_message("Loading…")
        .start()
        .unwrap();
    picker
        .send_options([("ff", "Firefox"), ("term", "Terminal")])
        .unwrap();

    match picker.wait() {
        Ok(DialogOutput::Picked(sel)) => println!("picked {}", sel.id),
        Ok(DialogOutput::Cancelled) => {}
        Ok(_) => {}
        Err(e) => eprintln!("dialog failed: {e}"),
    }
}
```

`Dialog::menu()` requests JSON and returns the full [`ModalSelection`] (`id`,
`label`, `index`, `path`, `query`, `custom_input`, `shortcut`). The child writes
one option per line (`label` or `id<TAB>label`) and exits `0` pick, `1` cancel,
`2` error. Use `.raw_format()` when stdout should stay a plain label, and
`.program(path)` to spawn an installed `r-modals` binary instead of the current
executable.

## Unified config

`ConfigFile` handles the lifecycle of one TOML file that holds both prefixes and
shortcuts:

```rust
use r_modals::ConfigFile;

ConfigFile::at("/home/me/.config/my-app/config.toml")
    .template(include_str!("config.toml"))
    .var("exe", "'/usr/bin/my-app'")
    .migrate_from(["/home/me/.config/my-app/prefixes.toml"])
    .ensure()
    .unwrap();
```

It creates the file from the template, folds in the legacy files once, and
appends built-in entries added by a newer version without duplicating existing
names. A malformed file is left untouched so the picker can report the parse
error instead of overwriting user edits. `{exe}` in a prefix/shortcut action is
resolved when the spec is compiled: raw in `command`/`args`, shell-quoted in
`shell`.

## Dialog modes

```sh
# Menu: stdin lines are labels or `id<TAB>label`; stdout is the pick.
printf 'ff\tFirefox\nterm\tTerminal\n' | r-modals --menu --format json
ls ~/bin | r-modals --menu --prompt "Run:" --fuzzy
printf 'a\nb\nc' | r-modals --menu --multi

# Prompt, confirm, and error dialogs.
r-modals --ask "Master password" --password
r-modals --confirm "Delete?" --message "This cannot be undone."
r-modals --error "Something went wrong"

r-modals --help
```

`--menu` opens immediately in its loading state and reads until stdin closes;
`--no-stream` reads everything first. `--format label|id|tag|json` controls the
output shape (tag prints the shortcut tag then the label). Exit codes are
`0` pick/value/confirmed/shown, `1` cancelled, `2` error.

## Theming

Styling is CSS-only:

```rust
let sel = Modal::new("Pick one")
    .stylesheet(include_str!("theme.css"))
    .run();
```

```css
modal { width: 560px; border-radius: 12px; }
search { background: #222228; color: #f0f0f5; }
```

See `examples/assets/theme.css` (an example stylesheet) and `assets/themes/` (the built-in themes) for starting points.

## Examples

```sh
cargo run --example basic    # minimal picker
cargo run --example search   # filtering + picker extras (--fuzzy/--multi flags)
cargo run --example menus    # submenus + actions (command/shell/closure)
cargo run --example shortcuts # file-configured keyboard shortcuts
cargo run --example prefixes  # query prefixes that run an action on Enter
cargo run --example gallery  # icons + List/Grid layouts + CSS themes (--grid)
cargo run --example dialogs  # Confirm + Prompt
cargo run --bin r-modals -- ~/Pictures/wallpapers
ls ~/bin | cargo run --bin r-modals -- --menu --prompt "Run:"
cargo run --bin r-modals -- --confirm "Continue?" --message "This is a demo."
```

## Notes

- On tiling compositors (sway/Hyprland), float the window via the `r-modals` app id, e.g. `for_window [app_id="r-modals"] floating enable`.

## License

GNU General Public License v3.0
