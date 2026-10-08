# r-modals

Instant modals for Rust — one call pops a centered, borderless, searchable option picker (`eframe`/`egui`, X11 + Wayland).

## Features

- **Blocking picker** — `Modal::new("Title").run()` returns the pick or `None` on cancel
- **Options** — label, description, and icons (file path, bytes, or raw RGBA)
- **Submenus** — nest options with `.sub_option(...)` for drill-down navigation
- **Actions** — run a command, shell one-liner, or Rust closure on select
- **Search** — filters over label, id, and description, with case-sensitivity option
- **Layouts** — `List` rows or `Grid` image tiles
- **CSS themes** — style with real CSS via `.stylesheet(...)`; built-ins: `Theme::dark()`, `Theme::light()`, `Theme::transparent_dark()`

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
cargo run --example basic
cargo run --example sub_options
cargo run --example actions
cargo run --example themed_icons
cargo run --example css_themed
cargo run --bin quick-modals -- ~/Pictures/wallpapers
```

## Notes

- On tiling compositors (sway/Hyprland), float the window via the `quick-modals` app id, e.g. `for_window [app_id="quick-modals"] floating enable`.

## License

MIT OR Apache-2.0
