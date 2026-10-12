//! Search, filtering, and picker extras — all in one modal.
//!
//! Run with:
//! ```sh
//! cargo run --example search
//! cargo run --example search -- --substring --case-sensitive --multi
//! ```
//!
//! Flags:
//! - `--substring`: contiguous substring match (default is fuzzy subsequence,
//!   ranked best-first, so `ffx` finds Firefox)
//! - `--case-sensitive`: `fire` no longer matches `Firefox`
//! - `--multi`: Space toggles rows, Enter confirms the set (`run_multi()`)
//!
//! Also demonstrated: `keyword`/`shortcut`/`separator`/`disabled` option
//! states, `default_selected`, `footer_hint`, `empty_message`,
//! `max_results`, `allow_custom_input` (Enter with no match returns the
//! typed text with `custom_input == true`), and `history_file` (pick counts
//! persist, most-used sorts first).

use r_modals::{Modal, ModalOption};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let multi = args.iter().any(|a| a == "--multi");
    let substring = args.iter().any(|a| a == "--substring");
    let case_sensitive = args.iter().any(|a| a == "--case-sensitive");

    let history = std::env::temp_dir()
        .join("r-modals-search-history.txt")
        .to_string_lossy()
        .into_owned();

    let modal = Modal::new("Search demo")
        .placeholder("Try 'ffx', 'web', or 'fire' vs 'Fire'…")
        .fuzzy(!substring)
        .case_sensitive(case_sensitive)
        .option(
            ModalOption::new("ff", "Firefox")
                .description("Web browser")
                .keyword("web"),
        )
        .option(ModalOption::new("term", "Terminal").description("Shell"))
        .option(
            ModalOption::new("files", "Files")
                .description("File manager")
                .shortcut("Ctrl+O"),
        )
        .option(ModalOption::separator("sep", "System"))
        .option(ModalOption::new("settings", "Settings").disabled(true))
        .default_selected("term")
        .footer_hint("Enter pick · Space toggle (multi) · Esc cancel")
        .empty_message("Nothing found — Enter keeps your text")
        .allow_custom_input(true)
        .max_results(50)
        .history_file(history);

    if multi {
        let picks = modal.run_multi()?;
        if picks.is_empty() {
            println!("cancelled");
        }
        for s in &picks {
            println!("picked {} ({}) custom={}", s.label, s.id, s.custom_input);
        }
    } else {
        match modal.run()? {
            Some(sel) => println!(
                "picked {} ({}) custom={}",
                sel.label, sel.id, sel.custom_input
            ),
            None => println!("cancelled"),
        }
    }
    Ok(())
}
