use crate::cmd::common::{load_rules, save_rules};
use crate::interaction::{select_rule, update_rule_form};
use anyhow::Result;

pub fn edit_rule(mut name: String) -> Result<()> {
    if name.is_empty() {
        name = select_rule().unwrap_or_default();
        if name.is_empty() {
            println!("There is no rule to edit.");
            return Ok(());
        }
    };
    let mut rules = load_rules()?;

    if let Some(current_rule) = rules.get(&name).cloned() {
        if current_rule.status {
            println!("Rule is running, please stop it first.");
            return Ok(());
        }
        let (new_name, new_rule) = update_rule_form(&name, &current_rule)?;
        if new_name != name && rules.contains_key(&new_name) {
            println!(
                "A rule with the name '{}' already exists. Please choose a different name.",
                new_name
            );
            return Ok(());
        }
        if new_rule == current_rule && new_name == name {
            println!("No changes were made to the rule '{}'.", name);
            return Ok(());
        }
        rules.remove(&name);
        rules.insert(new_name.clone(), new_rule);
        save_rules(&rules)?;
        if new_name != name {
            println!("Rule '{}' has been updated to '{}'.", name, new_name);
        } else {
            println!("Rule '{}' has been updated.", name);
        }
    };
    Ok(())
}
