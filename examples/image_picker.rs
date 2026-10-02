use quick_modals::{Layout, Modal, ModalOption, Theme};
use std::path::PathBuf;

/// Image files we treat as pickable.
fn is_image_file(p: &std::path::Path) -> bool {
    matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp")
    )
}

/// Solid-color RGBA square (no files needed).
fn solid_icon(size: usize, rgb: [u8; 3]) -> (usize, usize, Vec<u8>) {
    let mut rgba = Vec::with_capacity(size * size * 4);
    for _ in 0..size * size {
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    (size, size, rgba)
}

fn main() -> Result<(), String> {
    // Image grid: every option is a square that IS the image (no text rows).
    // Hover a tile to see its name; type to filter; arrows move, Enter picks.
    //
    // ```sh
    // cargo run --example image_picker            # uses ./assets
    // cargo run --example image_picker -- ~/Pictures/wallpapers
    // ```
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets".to_string());

    let mut modal = Modal::new("Pick an image")
        .placeholder("Search images…")
        .layout(Layout::Grid)
        .theme(Theme::dark().rounded(12.0).width(560.0).tile_size(128.0))
        .fit_width_to_content(true);

    // 1) Every image file in the directory becomes a tile.
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| is_image_file(p))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files.truncate(30);
    if files.is_empty() {
        eprintln!("no images in {dir:?} — showing built-in choices only");
    }
    for path in &files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string();
        modal = modal
            .option(ModalOption::new(format!("file:{}", path.display()), stem).icon(path.clone()));
    }

    // 2) Embedded bytes (works even when the directory is missing).
    modal = modal.option(
        ModalOption::new("bytes:terminal", "Terminal (embedded)")
            .icon(include_bytes!("../assets/terminal.png").to_vec()),
    );

    // 3) Procedural RGBA pixels — solid swatches.
    for (name, rgb) in [
        ("red", [200, 60, 50]),
        ("green", [60, 170, 80]),
        ("blue", [80, 120, 220]),
    ] {
        let (w, h, px) = solid_icon(24, rgb);
        modal = modal.option(
            ModalOption::new(format!("swatch:{name}"), format!("{name} swatch"))
                .icon_pixels(w, h, px),
        );
    }

    match modal.run()? {
        Some(sel) => println!("picked {} ({}) [index {}]", sel.label, sel.id, sel.index),
        None => println!("cancelled"),
    }
    Ok(())
}
