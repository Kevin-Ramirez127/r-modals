//! Search prefixes: type a prefix, press Enter, run its action.
//!
//! Run with: `cargo run --example prefixes`
//!
//! A prefix binds the start of the search query to an action. Type the prefix
//! (e.g. `>`) followed by optional text and press Enter: the action runs with
//! the text after the prefix (the "remainder") and the modal closes without a
//! pick — the same shape as a standalone shortcut action.
//!
//! The `{query}` token is replaced by the remainder. An `=` right after the
//! prefix (spaces allowed) delimits an explicit parameter, so
//! `>search=Hello World` passes `Hello World` (without the `=`). When an
//! action sets no `{query}`, the remainder is appended (an extra argument for
//! `command`, a single shell-quoted word for `shell`). For `shell`, `{query}`
//! is inserted shell-quoted, so don't wrap it in quotes yourself.
//!
//! Prefixes load from a TOML file (`Modal::prefixes_file`, auto-loaded from
//! `~/.config/r-modals/prefixes.toml` when present), inline TOML
//! (`Modal::prefixes_toml`), or in code (`Modal::prefix`).

use r_modals::{Modal, ModalOption, PrefixSpec};

const PREFIXES: &str = r#"
# ">" + text: notify with the typed text as one argument.
[[prefix]]
prefix = ">"
command = "notify-send"
args = ["r-modals", "{query}"]

# "?" + text: web-search the typed text (the value is shell-quoted for you).
[[prefix]]
prefix = "?"
shell = "xdg-open https://duckduckgo.com/?q={query}"
hint = "Web search"
"#;

fn main() -> Result<(), String> {
    let choice = Modal::new("Launcher")
        .placeholder("Type to filter, or \"> note\" / \"? rust closures\"…")
        .option(
            ModalOption::new("ff", "Firefox")
                .description("Web browser")
                .command("firefox"),
        )
        .option(ModalOption::new("term", "Terminal"))
        .prefixes_toml(PREFIXES)
        // One more prefix built in code (no TOML needed).
        .prefix(PrefixSpec::shell(
            ":",
            "notify-send r-modals 'colon prefix fired'",
        ))
        .footer_hint("Enter pick · \"> msg\" notify · \"? q\" search · Esc quit")
        .run()?;

    match choice {
        Some(sel) => println!("picked {} ({})", sel.label, sel.id),
        None => println!("closed"),
    }
    Ok(())
}
