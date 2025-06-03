use super::common::PortForwardRule;
use crate::cmd::common::{load_rules, save_rules};
use crate::interaction::select_rules;
use anyhow::{Ok, Result};
use dialoguer::Select;
use std::collections::HashMap;

fn remove_all(mut rules: HashMap<String, PortForwardRule>) -> Result<()> {
    if rules.is_empty() {
        println!("There is no rule to remove.");
        return Ok(());
    };
    let mut to_remove = vec![];
    for (name, rule) in rules.clone().into_iter() {
        if rule.status {
            println!("Rule {} is running, please stop it first.", name);
        } else {
            to_remove.push(name);
        }
    }
    if to_remove.is_empty() {
        println!("There is no rule to remove.");
        return Ok(());
    }
    let options = vec!["Yes", "No"];
    let selection = Select::new()
        .with_prompt(format!(
            "Are you sure you want to remove {} rule{}: {} ? This action cannot be undone",
            if to_remove.len() == 1 { "this" } else { "these" },
            if to_remove.len() == 1 { "" } else { "s" },
            to_remove.join(", ")
        ))
        .items(&options)
        .default(1)
        .interact()?;
    if selection == 0 {
        for name in to_remove {
            rules.remove(&name);
        }
        save_rules(&rules)?;
        println!("Rule(s) removed.");
    } else {
        println!("Rule(s) removal canceled.");
    }
    Ok(())
}

fn remove_selected(mut rules: HashMap<String, PortForwardRule>) -> Result<()> {
    let names = select_rules().unwrap_or_default();
    if names.is_empty() {
        println!("There is no rule to remove.");
        return Ok(());
    }
    let mut to_remove = vec![];
    for name in &names {
        if let Some(selected_rule) = rules.get(name) {
            if selected_rule.status {
                println!("Rule: {} is running, please stop it first.",name);
            }
            else {
                to_remove.push(name.to_string());
            }
        }
    
    }
    if to_remove.is_empty() {
        println!("There is no rule to remove.");
        return Ok(());
    }
    let options = vec!["Yes", "No"];
    let selection = Select::new()
        .with_prompt(format!(
            "Are you sure you want to remove {} rule{}: {}? This action cannot be undone",
            if to_remove.len() == 1 { "this" } else { "these" },
            if to_remove.len() == 1 { "" } else { "s" },
            to_remove.join(", ")
        ))
        .items(&options)
        .default(1)
        .interact()?;
    if selection == 0 {
        for name in to_remove {
            rules.remove(&name);
        }
        save_rules(&rules)?;
        println!("Rule(s) removed.");
    } else {
        println!("Rule(s) removal canceled.");
    }
    Ok(())
}
fn remove_input(names: Vec<String>, mut rules: HashMap<String, PortForwardRule>) -> Result<()> {
    let mut to_remove = vec![];
    for name in names {
        let _ = match rules.get(&name) {
            Some(rule) => {
                if rule.status {
                    println!("Rule is running, please stop it first.");
                } else {
                    to_remove.push(name);
                }
            }
            None => {
                println!("Rule {} not found.", name);
            }
        };
    }
    if to_remove.is_empty() {
        println!("There is no rule to remove.");
        return Ok(());
    }
    let options = vec!["Yes", "No"];
    let selection = Select::new()
        .with_prompt(format!(
            "Are you sure you want to remove {} rule{}: {}? This action cannot be undone",
            if to_remove.len() == 1 { "this" } else { "these" },
            if to_remove.len() == 1 { "" } else { "s" },
            to_remove.join(", ")
        ))
        .items(&options)
        .default(1)
        .interact()?;
    if selection == 0 {
        for name in to_remove {
            rules.remove(&name);
        }
        save_rules(&rules)?;
        println!("Rule(s) removed.");
    } else {
        println!("Rule(s) removal canceled.");
    }
    Ok(())
}

pub fn remove_rules(names: Vec<String>) -> Result<()> {
    let rules = load_rules()?;
    if names.is_empty() {
        remove_selected(rules)?;
        return Ok(());
    };
    if names == vec!["all"] {
        remove_all(rules)?;
        return Ok(());
    };
    remove_input(names, rules)?;
    Ok(())
}
