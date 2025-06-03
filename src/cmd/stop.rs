use super::common::PortForwardRule;
use crate::cmd::common::{load_rules, save_rules};
use crate::interaction::select_rules;
use crate::ssh::get_pid;
use anyhow::{Context, Result};
use dialoguer::Select;
use std::collections::HashMap;

use std::process::Command;

pub async fn stop_all(rules: HashMap<String, PortForwardRule>) -> Result<()> {
    let options = vec!["Yes", "No"];
    let selection = Select::new()
        .with_prompt("Are you sure you want to stop all rules?")
        .items(&options)
        .default(1)
        .interact()?;

    if selection == 0 {
        let names = rules.keys().cloned().collect();
        stop_forward_force(names, rules).await?;

        println!("All rules stopped.");
    } else {
        println!("The operation was cancelled.");
    };
    Ok(())
}

pub async fn stop_selected(rules: HashMap<String, PortForwardRule>) -> Result<()> {
    let names = select_rules().unwrap_or_default();
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
        stop_forward_force(names.clone(), rules).await?;

        println!(
            "{} rule{} stopped.",
            if names.len() == 1 { "Rule" } else { "Rules" },
            if names.len() == 1 { "" } else { "s" }
        );
    } else {
        println!("The operation was cancelled.");
    };

    Ok(())
}
pub async fn stop_input(
    mut names: Vec<String>,
    rules: HashMap<String, PortForwardRule>,
) -> Result<()> {
    names.retain(|name| {
        if !rules.contains_key(name) {
            println!("Rule '{}' not found.", name);
            false
        } else {
            true
        }
    });
    stop_forward_force(names.clone(), rules).await?;
    println!(
        "{} rule{} stopped.",
        if names.len() == 1 { "Rule" } else { "Rules" },
        if names.len() == 1 { "" } else { "s" }
    );

    Ok(())
}

pub async fn stop_forward_force(
    names: Vec<String>,
    mut rules: HashMap<String, PortForwardRule>,
) -> Result<()> {
    for name in names {
        if let Some(rule) = rules.get_mut(&name) {
            let local_port = rule.local_port;
            if let Ok(pid) = get_pid(local_port) {
                let output = Command::new("kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .output()
                    .context("Failed to execute kill -9 command")?;
                if !output.status.success() {
                    let error = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!(
                        "Stoping SSH portforward process exited abnormally: {}",
                        error
                    );
                }
            };
            rule.pid = None;
            rule.status = false;
            save_rules(&rules)?;
        }
    }
    Ok(())
}

pub async fn stop_forward(names: Vec<String>) -> Result<()> {
    let rules = load_rules()?;
    if names.is_empty() {
        stop_selected(rules).await?;
        return Ok(());
    };

    if names == vec!["all"] {
        stop_all(rules).await?;
        return Ok(());
    };

    stop_input(names, rules).await?;

    Ok(())
}
