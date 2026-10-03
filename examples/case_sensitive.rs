use quick_modals::{Modal, ModalOption};

fn main() -> Result<(), String> {
    // Case-sensitive search: "firefox" will NOT match "Firefox".
    let choice = Modal::new("Case-sensitive demo")
        .placeholder("Try 'fire' vs 'Fire'…")
        .case_sensitive(true)
        .options([
            ModalOption::new("1", "Firefox"),
            ModalOption::new("2", "firefox-beta"),
            ModalOption::new("3", "FIREFOX NIGHTLY"),
        ])
        .stylesheet("modal { width: 480px; }")
        .run()?;

    println!("{choice:?}");
    Ok(())
}
