use quick_modals::{Modal, ModalOption};

fn main() -> Result<(), String> {
    // Actions: picking an option can fire a shell command, a shell one-liner,
    // or a Rust closure. The action runs once, after the window closes, on
    // the calling thread — `run()` still returns the pick as usual.
    //
    // ```sh
    // cargo run --example actions
    // ```
    let choice = Modal::new("Launcher")
        .placeholder("Type to filter…")
        .option(
            ModalOption::new("term", "Terminal")
                .description("Spawned detached via shell")
                .shell("xterm &"),
        )
        .option(
            ModalOption::new("ff", "Firefox")
                .description("Spawned detached, no shell")
                .command("firefox")
                .arg("--new-window"),
        )
        .option(
            ModalOption::new("tools", "Tools").sub_options([
                ModalOption::new("hello", "Say hi")
                    .description("Rust closure, no process")
                    .on_select(|sel| println!("hello from {}!", sel.id)),
                ModalOption::new("date", "Print date")
                    .description("Shell one-liner")
                    .shell("date"),
            ]),
        )
        .run()?;

    match choice {
        Some(sel) => println!("picked {} ({})", sel.label, sel.id),
        None => println!("cancelled"),
    }
    Ok(())
}
