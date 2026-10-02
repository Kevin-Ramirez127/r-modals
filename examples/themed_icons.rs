use egui::Color32;
use quick_modals::{Modal, ModalOption, Theme};

fn main() -> Result<(), String> {
    // Icons: file path, embedded bytes, or raw RGBA pixels.
    // Missing/undecodable images fall back to no icon (never crashes).
    let choice = Modal::new("Themed + icons")
        .placeholder("Search apps…")
        .case_sensitive(false)
        .option(
            ModalOption::new("ff", "Firefox")
                .description("Web browser")
                .icon("assets/firefox.png"), // missing file -> graceful fallback
        )
        .option(
            ModalOption::new("term", "Terminal")
                .description("Alacritty")
                .icon(include_bytes!("../README.md").to_vec()), // invalid image -> fallback
        )
        .option(ModalOption::new("edit", "Editor").description("No icon here"))
        .theme(
            Theme::transparent_dark()
                .rounded(16.0)
                .width(600.0)
                .accent(Color32::from_rgb(120, 90, 220)),
        )
        .run()?;

    println!("{choice:?}");
    Ok(())
}
