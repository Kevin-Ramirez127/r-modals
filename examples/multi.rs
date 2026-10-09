use quick_modals::{Modal, ModalOption};

fn main() -> Result<(), String> {
    // Space toggles rows, Enter confirms the set.
    let picks = Modal::new("Pick toppings")
        .placeholder("Type to filter…")
        .option(ModalOption::new("cheese", "Cheese"))
        .option(ModalOption::new("pepperoni", "Pepperoni"))
        .option(ModalOption::new("mushroom", "Mushrooms"))
        .option(ModalOption::new("out", "Out of stock").disabled(true))
        .run_multi()?;

    if picks.is_empty() {
        println!("cancelled");
    } else {
        for s in &picks {
            println!("picked {} ({})", s.label, s.id);
        }
    }
    Ok(())
}
