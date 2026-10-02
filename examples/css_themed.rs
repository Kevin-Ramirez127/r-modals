use quick_modals::{Modal, ModalOption};

fn main() -> Result<(), String> {
    // Style the modal from a CSS file (parsed with lightningcss).
    // Only the documented subset applies — see `Theme::from_css`.
    // A bad stylesheet makes `run()` return `Err`.
    let choice = Modal::new("CSS themed")
        .placeholder("Search…")
        .stylesheet(include_str!("../assets/theme.css"))
        .option(ModalOption::new("ff", "Firefox").description("Web browser"))
        .option(ModalOption::new("term", "Terminal").description("Shell"))
        .option(ModalOption::new("files", "Files").description("File manager"))
        .run()?;

    match choice {
        Some(sel) => println!("picked {} ({})", sel.label, sel.id),
        None => println!("cancelled"),
    }
    Ok(())
}
