# r-modals

Instant modals for Rust — one call pops a centered, borderless, searchable option picker (`eframe`/`egui`, X11 + Wayland).

## Features

- **Blocking picker** — `Modal::new("Title").run()` returns the pick or `None` on cancel
- **Options** — label, description, and icons (file path, bytes, or raw RGBA)
- **Submenus** — nest options with `.sub_option(...)` for drill-down navigation
- **Actions** — run a command, shell one-liner, or Rust closure on select
- **Search** — substring or fuzzy (`Modal::fuzzy(true)`, ranked best-first) over label, id, description, and keywords, with case-sensitivity option
- **Option states** — `.disabled(true)`, `ModalOption::separator(..)`, `.shortcut("Ctrl+O")`, `.keyword(..)`
- **Picker extras** — `.default_selected(..)`, `.allow_custom_input(true)` (free text), `.empty_message(..)`, `.footer_hint(..)`, `.max_results(n)`, `.multi_select(true)` + `run_multi()`, `.history_file(..)` MRU sorting, `.font(..)` custom fonts
- **Dialogs** — `Confirm::new(..).run() -> bool`, `Prompt::new(..).run() -> Option<String>`
- **Layouts** — `List` rows or `Grid` image tiles
- **CSS themes** — style with real CSS via `.stylesheet(...)`; built-ins: `Theme::dark()`, `Theme::light()`, `Theme::transparent_dark()`, `Theme::auto()`
- **dmenu mode** — pipe stdin to `quick-modals --dmenu`, pick goes to stdout (exit 0 pick · 1 cancel · 2 error)

## Install

build from source:

```sh
cargo install --git https://github.com/Kevin-Ramirez127/r-modals quick-modals
```

To use it as a library, add it to your `Cargo.toml`:

```toml
[dependencies]
quick-modals = { git = "https://github.com/Kevin-Ramirez127/r-modals" }
```

## Usage

```rust
use quick_modals::{Modal, ModalOption};

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
Extras: `Home`/`End`, `PgUp`/`PgDn`, `Ctrl+N`/`Ctrl+P` move; `Ctrl+U` clears the query; `Space` toggles in `run_multi()`.

## Dialogs

```rust
use quick_modals::{Confirm, Prompt};

let name = Prompt::new("Your name?").placeholder("Jane").run().unwrap();
let ok = Confirm::new("Greet?").message("Say hello?").run().unwrap();
```

## dmenu mode

```sh
ls ~/bin | quick-modals --dmenu --prompt "Run:" --fuzzy
printf 'a\nb\nc' | quick-modals --dmenu --multi
quick-modals --help
```

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

See `assets/theme.css` and `assets/themes/` for starting points.

## Examples

```sh
cargo run --example basic    # minimal picker
cargo run --example search   # filtering + picker extras (--fuzzy/--multi flags)
cargo run --example menus    # submenus + actions (command/shell/closure)
cargo run --example gallery  # icons + List/Grid layouts + CSS themes (--grid)
cargo run --example dialogs  # Confirm + Prompt
cargo run --bin quick-modals -- ~/Pictures/wallpapers
ls ~/bin | cargo run --bin quick-modals -- --dmenu --prompt "Run:"
```

## Notes

- On tiling compositors (sway/Hyprland), float the window via the `quick-modals` app id, e.g. `for_window [app_id="quick-modals"] floating enable`.

## License

MIT OR Apache-2.0
