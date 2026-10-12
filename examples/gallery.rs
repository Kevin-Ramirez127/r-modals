//! Icons, layouts, and themes — how the modal looks.
//!
//! Run with:
//! ```sh
//! cargo run --example gallery
//! cargo run --example gallery -- --grid   # image tiles instead of list rows
//! ```
//!
//! - Icons, all three sources: file path (`assets/firefox.png`), embedded
//!   bytes (`include_bytes!`), and raw RGBA pixels (`icon_pixels`).
//!   Missing/undecodable images fall back to no icon (never crashes).
//! - Layout: `List` rows (thumbnail + text) or `Grid` image tiles (the tile
//!   IS the image; the label becomes a hover tooltip, search still filters).
//! - Theme: built-ins are `Theme::dark()`, `Theme::light()`,
//!   `Theme::transparent_dark()`, and `Theme::auto()` (light/dark sniff).
//!   `.stylesheet(css)` layers CSS over the theme (wins where specified);
//!   see `Theme::from_css` for the supported selectors/properties. A bad
//!   stylesheet makes `run()` return `Err`.

use r_modals::{Layout, Modal, ModalOption, Theme};

/// Procedural 16x16 checkerboard icon (RGBA, no files needed).
fn checkerboard() -> (usize, usize, Vec<u8>) {
    let (w, h) = (16usize, 16usize);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let light = (x / 4 + y / 4) % 2 == 0;
            if light {
                rgba.extend_from_slice(&[120, 90, 220, 255]);
            } else {
                rgba.extend_from_slice(&[30, 30, 40, 255]);
            }
        }
    }
    (w, h, rgba)
}

fn main() -> Result<(), String> {
    let grid = std::env::args().any(|a| a == "--grid");
    let (pw, ph, px) = checkerboard();

    let choice = Modal::new("Gallery")
        .placeholder("Search apps…")
        .layout(if grid { Layout::Grid } else { Layout::List })
        .theme(Theme::transparent_dark())
        .stylesheet(include_str!("../assets/theme.css"))
        .stylesheet_override("modal { width: 600px; }")
        .option(
            ModalOption::new("ff", "Firefox")
                .description("Web browser — file icon")
                .icon("assets/firefox.png"),
        )
        .option(
            ModalOption::new("term", "Terminal")
                .description("Alacritty — embedded bytes")
                .icon(include_bytes!("../assets/terminal.png").to_vec()),
        )
        .option(
            ModalOption::new("edit", "Editor")
                .description("Raw pixels — checkerboard")
                .icon_pixels(pw, ph, px),
        )
        .run()?;

    println!("{choice:?}");
    Ok(())
}
