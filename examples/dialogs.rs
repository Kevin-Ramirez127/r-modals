//! One-shot dialogs on the same backend: text input + yes/no.
//!
//! Run with: `cargo run --example dialogs`
//!
//! - `Prompt` returns `Ok(None)` on Esc / window close. Beyond
//!   `message`/`placeholder`, it supports `initial` (pre-filled value),
//!   `password(true)` (masked input), and `confirm_label`.
//! - `Confirm` returns `Ok(true)` on confirm, `Ok(false)` on cancel / Esc /
//!   close. Button texts come from `confirm_label`/`cancel_label`.
//! - Both take `theme(..)` and `app_id(..)` like `Modal`.

use quick_modals::{Confirm, Prompt};

fn main() -> Result<(), String> {
    let name = Prompt::new("Your name?")
        .message("Shown above the input field.")
        .placeholder("Jane")
        .initial("")
        .run()?;
    let Some(name) = name else {
        println!("cancelled");
        return Ok(());
    };
    let ok = Confirm::new("Greet?")
        .message(format!("Say hello to {name}?"))
        .confirm_label("Greet")
        .cancel_label("Cancel")
        .run()?;
    if ok {
        println!("hello, {name}!");
    } else {
        println!("cancelled");
    }
    Ok(())
}
