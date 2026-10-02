use quick_modals::{Modal, ModalOption};

fn main() -> Result<(), String> {
    let choice = Modal::new("Launcher")
        .placeholder("Type to filter…")
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
