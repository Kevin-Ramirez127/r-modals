# r-modals

Instant modals made with Rust.

One call pops a centered, borderless, searchable option picker (eframe/egui backend, works on X11 + Wayland).

## Features

- **Modals** — `Modal::new("Title").run()` blocks until pick / cancel
- **Options** — `ModalOption::new(id, label).description(...)`
- **Search** — substring filter over label + id + description, `case_sensitive(true/false)`, placeholder, pre-filled query
- **Images** — per-option icon from file path, bytes (`include_bytes!`), or raw RGBA; failures degrade gracefully
- **Layouts** — `Layout::List` rows (thumbnail + text) or `Layout::Grid` squares where the tile *is* the image; keyboard navigation centers the selected row/tile, disable with `.center_on_navigate(false)`
- **Styling is CSS-only** — `Theme::from_css(include_str!("theme.css"))` or `.stylesheet(css)` parses real CSS (`lightningcss`); no builder functions. Selectors: `modal`, `tile` (+`width`/`height`, `tile icon`), `tile[selected]`, `tile description` (incl. `display: none`); px-only subset, later rules win. Built-ins (`Theme::dark/light/transparent_dark`) are embedded stylesheets — copy `assets/themes/dark.css` to start your own.
- **Fit to content** — `.fit_width_to_content(true)` shrinks the window width to the measured content (clamped to 320px…theme width) instead of using the full theme width

## Usage

```rust
use quick_modals::{Modal, ModalOption};

let choice = Modal::new("Launcher")
    .option(ModalOption::new("ff", "Firefox").description("Web browser"))
    .option(ModalOption::new("term", "Terminal").icon("icons/term.png"))
    .stylesheet("modal { width: 560px; border-radius: 12px; }")
    .run()
    .unwrap();
```

`run()` returns `Ok(Some(ModalSelection { id, label, index, query }))` or `Ok(None)` on Esc/close.

Keys: type to filter, ↑/↓ navigate, Enter pick, Esc cancel. Mouse hover + click works too.

## Examples

```sh
cargo run --example basic
cargo run --example case_sensitive
cargo run --example themed_icons
cargo run --example css_themed                # styled from assets/theme.css
cargo run --example image_picker            # pick one of several images
cargo run --example image_picker -- ~/Pictures/wallpapers
```

Headless filtering (no window) is also available and unit-tested:

```rust
let m = Modal::new("x").option(ModalOption::new("a", "Alpha"));
assert_eq!(m.filtered_indices("alp"), vec![0]);
```

## Notes

- eframe centers the window; `Position::Custom { x, y }` sets an explicit position.
  True layer-shell exclusivity is compositor-specific and intentionally out of scope.
- Icons decode via the `image` crate at render time and are cached as egui textures.
- Tiling compositors (sway/Hyprland/…) tile the modal and ignore its size.
  Float it via the `quick-modals` app_id (override with `Modal::app_id`), e.g. sway:
  `for_window [app_id="quick-modals"] floating enable`.
