use super::common::PortForwardRule;
use crate::cmd::common::load_rules;
use crate::interaction::select_rules;
use crate::services;
use crate::utils::check_exist;
use anyhow::Result;
use dialoguer::Select;
use std::collections::HashMap;

pub async fn stop_all(rules: &mut HashMap<String, PortForwardRule>) -> Result<()> {
    let options = vec!["Yes", "No"];

    let selection = Select::new()
        .with_prompt("Are you sure you want to stop all rules?")
        .items(&options)
        .default(1)
        .interact()?;

    if selection == 0 {
        let names = rules.keys().cloned().collect::<Vec<String>>();
        stop_forward_force(&names, rules).await?;
        println!("All rules stopped.");
    } else {
        println!("The operation was cancelled.");
    };
    Ok(())
}

pub async fn stop_selected(rules: &mut HashMap<String, PortForwardRule>) -> Result<()> {
    let names = select_rules().unwrap_or_default();
    if names.is_empty() {
        return Ok(());
    }
    let options = vec!["Yes", "No"];
    let selection = Select::new()
        .with_prompt(format!(
            "Are you sure you want to stop {} rule{}: {}?",
            if names.len() == 1 { "this" } else { "these" },
            if names.len() == 1 { "" } else { "s" },
            names.join(", ")
        ))
        .items(&options)
        .default(1)
        .interact()?;
    if selection == 0 {
        stop_forward_force(&names, rules).await?;
        println!(
            "{} rule{} stopped.",
            if names.len() == 1 { "Rule" } else { "Rules" }.to_string(),
            if names.len() == 1 { "" } else { "s" }
        );
    } else {
        println!("The operation was cancelled.");
    };

    Ok(())
}
pub async fn stop_input(
    names: Vec<String>,
    rules: &mut HashMap<String, PortForwardRule>,
) -> Result<()> {
    let names = check_exist(names)?;
    stop_forward_force(&names, rules).await?;
    Ok(())
}

pub async fn stop_forward_force(
    names: &Vec<String>,
    _rules: &mut HashMap<String, PortForwardRule>,
) -> Result<()> {
    services::stop_rules(names)?;
    for name in names {
        println!("Rule '{}' stopped.", name);
    }
    Ok(())
}

pub async fn stop_forward(names: Vec<String>) -> Result<()> {
    let mut rules = load_rules()?;
    if names.is_empty() {
        stop_selected(&mut rules).await?;
        return Ok(());
    };

    if names == vec!["all"] {
        stop_all(&mut rules).await?;
        return Ok(());
    };

    stop_input(names, &mut rules).await?;

    Ok(())
}
