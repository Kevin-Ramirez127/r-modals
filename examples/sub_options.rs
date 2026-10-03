use quick_modals::{Modal, ModalOption};

fn main() -> Result<(), String> {
    // Sub-options: an option with children is a submenu. Enter (or click)
    // on it drills one level deeper instead of picking it; Esc, Backspace
    // (with an empty search field), or the ‹ breadcrumb goes back up.
    // Only childless (leaf) options can be picked — `sel.path` holds the
    // index path from the top level to the picked leaf.
    let choice = Modal::new("Settings")
        .placeholder("Type to filter…")
        .option(
            ModalOption::new("display", "Display")
                .description("Monitor, brightness, wallpaper")
                .sub_options([
                    ModalOption::new("resolution", "Resolution").sub_options([
                        ModalOption::new("1080p", "1920×1080"),
                        ModalOption::new("1440p", "2560×1440"),
                        ModalOption::new("4k", "3840×2160"),
                    ]),
                    ModalOption::new("brightness", "Brightness").sub_options([
                        ModalOption::new("bright-low", "Low"),
                        ModalOption::new("bright-medium", "Medium"),
                        ModalOption::new("bright-high", "High"),
                    ]),
                    ModalOption::new("wallpaper", "Wallpaper"),
                ]),
        )
        .option(
            ModalOption::new("sound", "Sound")
                .description("Volume and output device")
                .sub_option(ModalOption::new("volume", "Volume"))
                .sub_option(
                    ModalOption::new("output", "Output").sub_options([
                        ModalOption::new("speakers", "Speakers"),
                        ModalOption::new("headphones", "Headphones"),
                    ]),
                ),
        )
        .option(ModalOption::new("about", "About").description("No submenu here"))
        .run()?;

    match choice {
        Some(sel) => println!(
            "picked {} ({}) at path {:?}",
            sel.label, sel.id, sel.path
        ),
        None => println!("cancelled"),
    }
    Ok(())
}
