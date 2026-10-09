use quick_modals::{Confirm, Prompt};

fn main() -> Result<(), String> {
    let name = Prompt::new("Your name?")
        .message("Shown above the input field.")
        .placeholder("Jane")
        .run()?;
    let Some(name) = name else {
        println!("cancelled");
        return Ok(());
    };
    let ok = Confirm::new("Greet?")
        .message(format!("Say hello to {name}?"))
        .confirm_label("Greet")
        .run()?;
    if ok {
        println!("hello, {name}!");
    } else {
        println!("cancelled");
    }
    Ok(())
}
