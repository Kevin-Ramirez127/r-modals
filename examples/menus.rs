//! Submenus + actions: drill-down navigation where picking fires something.
//!
//! Run with: `cargo run --example menus`
//!
//! - An option with `sub_option`/`sub_options` children is a submenu: Enter
//!   (or click) drills one level deeper instead of picking it. Esc,
//!   Backspace (with an empty query), or the ‹ breadcrumb goes back up.
//! - Only childless (leaf) options can be picked; `sel.path` holds the index
//!   path from the top level to the picked leaf.
//! - A pick can fire a `command` (+ `arg`/`args`), a `shell` one-liner, or an
//!   `on_select` Rust closure. The action runs once, after the window
//!   closes, on the calling thread — `run()` still returns the pick. A
//!   failing spawn fires `on_error` instead of erasing the pick.

use quick_modals::{Modal, ModalOption};

fn main() -> Result<(), String> {
    let choice = Modal::new("Settings")
        .placeholder("Type to filter…")
        .option(
            ModalOption::new("display", "Display")
                .description("Monitor, brightness, wallpaper")
                .sub_options([
                    ModalOption::new("resolution", "Resolution").sub_options([
                        ModalOption::new("1080p", "1920×1080"),
                        ModalOption::new("1440p", "2560×1440"),
                        ModalOption::new("4k", "3840×2160"),
                    ]),
                    ModalOption::new("brightness", "Brightness").sub_options([
                        ModalOption::new("bright-low", "Low"),
                        ModalOption::new("bright-medium", "Medium"),
                        ModalOption::new("bright-high", "High"),
                    ]),
                    ModalOption::new("wallpaper", "Wallpaper"),
                ]),
        )
        .option(
            ModalOption::new("term", "Terminal")
                .description("Spawned detached via shell")
                .shell("xterm &"),
        )
        .option(
            ModalOption::new("ff", "Firefox")
                .description("Spawned detached, no shell")
                .command("firefox")
                .arg("--new-window"),
        )
        .option(
            ModalOption::new("tools", "Tools").sub_options([
                ModalOption::new("hello", "Say hi")
                    .description("Rust closure, no process")
                    .on_select(|sel| println!("hello from {}!", sel.id)),
                ModalOption::new("date", "Print date")
                    .description("Shell one-liner")
                    .shell("date"),
                ModalOption::new("broken", "Broken launcher")
                    .description("Falls back to on_error")
                    .command("definitely-not-a-real-binary")
                    .on_error(|sel, err| println!("could not launch {}: {err}", sel.id)),
            ]),
        )
        .run()?;

    match choice {
        Some(sel) => println!(
            "picked {} ({}) at path {:?}",
            sel.label, sel.id, sel.path
        ),
        None => println!("cancelled"),
    }
    Ok(())
}
