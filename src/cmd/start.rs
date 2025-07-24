use super::common::PortForwardRule;
use crate::cmd::common::load_rules;
use crate::interaction::select_rules;
use crate::ssh::portforward;
use anyhow::{Ok, Result};
use dialoguer::Select;
use std::collections::HashMap;

async fn start_all(rules: HashMap<String, PortForwardRule>) -> Result<()> {
    
    let options = vec!["Yes", "No"];
    let selection = Select::new()
        .with_prompt(format!(
            "Are you sure you want to start all rules? Running rules will not be affected."
        ))
        .items(&options)
        .default(1)
        .interact()?;

    if selection == 0 {
        for (name, rule) in rules.into_iter() {
            portforward(name.to_string(), &rule).await?;
        }
        println!("All rules started.");
    } else {
        println!("The operation was cancelled.");
    };
    Ok(())
}

async fn start_selected(rules: HashMap<String, PortForwardRule>) -> Result<()>{
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
        for name in names {
                start_forward_force(name, rules.clone()).await?;
            }
    } else {
        println!("The operation was cancelled.");
    };


  Ok(())

    
}

async fn start_input(names: Vec<String>,rules: HashMap<String, PortForwardRule>) -> Result<()>{
    for name in &names {
        let _ = match rules.get(name) {
            Some(rule) => {
                portforward(name.to_string(), &rule).await?;
            }
            None => {
                println!("Rule {} not found.", name);
            }
        };
    };

    Ok(())
}

pub async fn start_forward_force(
    name: String,
    rules: HashMap<String, PortForwardRule>,
) -> Result<()> {
    if let Some(rule) = rules.get(&name) {
        portforward(name.to_string(), &rule).await?;
    }
    Ok(())
}

pub async fn start_forward(names: Vec<String>) -> Result<()> {
    let rules = load_rules()?;

    if names.is_empty() {
        start_selected(rules).await?;
        return Ok(());
    }
    if names == vec!["all"] {

        start_all(rules).await?;
        return Ok(());

    }

    start_input(names, rules).await?;

    Ok(())
}
