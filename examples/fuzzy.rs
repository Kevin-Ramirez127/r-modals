use quick_modals::{Modal, ModalOption};

fn main() -> Result<(), String> {
    // Subsequence fuzzy match, best-first: "ffx" finds Firefox even though
    // it is not a contiguous substring.
    let choice = Modal::new("Fuzzy finder")
        .placeholder("Type initials, e.g. ffx…")
        .fuzzy(true)
        .footer_hint("Fuzzy match · Enter pick · Esc cancel")
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
        .run()?;

    match choice {
        Some(sel) => println!("picked {} ({})", sel.label, sel.id),
        None => println!("cancelled"),
    }
    Ok(())
}
