use super::common::PortForwardRule;
use crate::interaction::select_rules;
use crate::services;
use crate::utils::check_exist;
use crate::cmd::common::load_rules;
use anyhow::Result;
use dialoguer::Select;
use std::collections::HashMap;

async fn start_selected(rules: &mut HashMap<String, PortForwardRule>) -> Result<()> {
    let names = select_rules().unwrap_or_default();
    if names.is_empty() {
        return Ok(());
    }
    let options = vec!["Yes", "No"];
    let selection = Select::new()
        .with_prompt(format!(
            "Are you sure you want to start {} rule{}: {}? Running rules will not be affected",
            if names.len() == 1 { "this" } else { "these" },
            if names.len() == 1 { "" } else { "s" },
            names.join(", ")
        ))
        .items(&options)
        .default(1)
        .interact()?;
    if selection == 0 {
        start_forward_force(&names, rules).await?;
    } else {
        println!("The operation was cancelled.");
    };
    Ok(())
}

async fn start_input(
    names: Vec<String>,
    rules: &mut HashMap<String, PortForwardRule>,
) -> Result<()> {
    let names = check_exist(names)?;
    start_forward_force(&names, rules).await?;
    Ok(())
}

pub async fn start_forward_force(
    names: &Vec<String>,
    _rules: &mut HashMap<String, PortForwardRule>,
) -> Result<()> {
    services::start_rules(names)?;
    for name in names {
        println!("Rule '{}' started.", name);
    }
    Ok(())
}

pub async fn start_forward(names: Vec<String>) -> Result<()> {
    let mut rules = load_rules()?;

    if names.is_empty() {
        start_selected(&mut rules).await?;
        return Ok(());
    }

    start_input(names, &mut rules).await?;

    Ok(())
}
