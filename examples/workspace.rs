//! Open a picker on a specific compositor workspace.
//!
//! Run with: `cargo run --example workspace -- 2`
//!
//! The argument is a workspace name or number (default `2`). Placement is
//! best-effort: Hyprland, Sway, i3, niri, KDE Plasma, and generic X11 are
//! supported; unsupported compositors leave the window on the current one.

use r_modals::{Modal, ModalOption, Workspace};

fn main() -> Result<(), String> {
    let workspace = std::env::args().nth(1).unwrap_or_else(|| "2".to_string());
    let choice = Modal::new("Workspace demo")
        .placeholder("Pick a target…")
        .option(ModalOption::new("one", "First"))
        .option(ModalOption::new("two", "Second"))
        .workspace(Workspace::new(workspace))
        .run()?;

    match choice {
        Some(sel) => println!("picked {} ({})", sel.label, sel.id),
        None => println!("cancelled"),
    }
    Ok(())
}
