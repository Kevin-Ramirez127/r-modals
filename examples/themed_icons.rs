use quick_modals::{Modal, ModalOption, Theme};

/// Procedural 16x16 checkerboard icon (RGBA, no files needed).
fn checkerboard() -> (usize, usize, Vec<u8>) {
    let (w, h) = (16usize, 16usize);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let light = (x / 4 + y / 4) % 2 == 0;
            if light {
                rgba.extend_from_slice(&[120, 90, 220, 255]);
            } else {
                rgba.extend_from_slice(&[30, 30, 40, 255]);
            }
        }
    }
    (w, h, rgba)
}

fn main() -> Result<(), String> {
    // All three icon sources, each with a real image:
    // - File path (assets/firefox.png ships with the repo)
    // - Embedded bytes via include_bytes! (assets/terminal.png)
    // - Raw RGBA pixels (procedural checkerboard)
    // Missing/undecodable images fall back to no icon (never crashes).
    let (pw, ph, px) = checkerboard();
    let choice = Modal::new("Themed + icons")
        .placeholder("Search apps…")
        .case_sensitive(false)
        .option(
            ModalOption::new("ff", "Firefox")
                .description("Web browser — file icon")
                .icon("assets/firefox.png"),
        )
        .option(
            ModalOption::new("term", "Terminal")
                .description("Alacritty — embedded bytes")
                .icon(include_bytes!("../assets/terminal.png").to_vec()),
        )
        .option(
            ModalOption::new("edit", "Editor")
                .description("Raw pixels — checkerboard")
                .icon_pixels(pw, ph, px),
        )
        .theme(Theme::transparent_dark())
        .stylesheet("modal { width: 600px; border-radius: 16px; accent-color: rgb(120, 90, 220); }")
        .run()?;

    println!("{choice:?}");
    Ok(())
}
