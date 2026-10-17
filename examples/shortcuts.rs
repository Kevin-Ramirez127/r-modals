//! File-configured keyboard shortcuts.
//!
//! Run with: `cargo run --example shortcuts`
//!
//! Shortcuts are declared in TOML and loaded with `Modal::shortcuts_file(path)`
//! or, as here, inline with `Modal::shortcuts_toml(text)`. Each `[[shortcut]]`
//! entry binds one key combo to either an option id or a terminal command:
//!
//! - `option = "<id>"`   → activate that option (drill into a submenu, or pick
//!   a leaf and fire its action), exactly as if the user pressed Enter on it;
//! - `command = "<prog>"` (+ optional `args = [..]`) → spawn `prog` detached;
//! - `shell = "<one-liner>"` → run a shell command detached (`sh -c`).
//!
//! A file may mix all three. Shortcut keys are consumed before the built-in
//! keys, so they can rebind even `Enter`. Bound `option` shortcuts also show up
//! as the right-aligned hint on their row (unless the row sets its own).
//!
//! Command shortcuts run the action and close the modal without a pick (there
//! is no row to return). Option shortcuts return the normal selection.

use r_modals::{Modal, ModalOption};

const SHORTCUTS: &str = r#"
# Ctrl+O activates the "ff" option below (same as picking it).
[[shortcut]]
key = "Ctrl+O"
option = "ff"

# Ctrl+Shift+T spawns a detached program from the file itself.
[[shortcut]]
key = "Ctrl+Shift+T"
command = "xterm"
args = ["-e", "htop"]

# Ctrl+N runs a shell one-liner from the file itself.
[[shortcut]]
key = "Ctrl+N"
shell = "notify-send r-modals 'shortcut fired'"
"#;

fn main() -> Result<(), String> {
    let choice = Modal::new("Launcher")
        .placeholder("Type to filter…")
        .option(
            ModalOption::new("ff", "Firefox")
                .description("Opens a browser")
                .command("firefox"),
        )
        .option(ModalOption::new("term", "Terminal").description("Try Ctrl+Shift+T for htop"))
        .shortcuts_toml(SHORTCUTS)
        .footer_hint("Ctrl+O firefox · Ctrl+Shift+T htop · Ctrl+N notify · Esc quit")
        .run()?;

    match choice {
        Some(sel) => println!("picked {} ({})", sel.label, sel.id),
        None => println!("closed"),
    }
    Ok(())
}
