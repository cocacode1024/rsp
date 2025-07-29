use super::common::PortForwardRule;
use crate::cmd::common::save_rules;
use crate::interaction::select_rules;
use crate::utils::check_exist;
use crate::{cmd::common::load_rules, utils::get_pid};
use anyhow::{Context, Ok, Result};
use dialoguer::Select;
use std::{collections::HashMap, process::Command};

async fn start_all(rules: &mut HashMap<String, PortForwardRule>) -> Result<()> {
    let options = vec!["Yes", "No"];
    let selection = Select::new()
        .with_prompt(format!(
            "Are you sure you want to start all rules? Running rules will not be affected."
        ))
        .items(&options)
        .default(1)
        .interact()?;

    if selection == 0 {
        let names = rules.keys().cloned().collect::<Vec<String>>();
        start_forward_force(&names, rules).await?;
        println!("All rules started.");
    } else {
        println!("The operation was cancelled.");
    };
    Ok(())
}

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
    rules: &mut HashMap<String, PortForwardRule>,
) -> Result<()> {
    for name in names {
        if let Some(rule) = rules.get_mut(name) {
            println!(
                "Rule '{}' starting, localhost:{} -> {}:{}",
                name, rule.local_port, rule.remote_host, rule.remote_port
            );

            let output = Command::new("ssh")
                .arg("-f")
                .arg("-N")
                .arg("-C")
                .arg("-g")
                .arg("-L")
                .arg(format!(
                    "{}:localhost:{}",
                    rule.local_port, rule.remote_port
                ))
                .arg(format!("{}", rule.remote_host))
                .output()
                .context("Failed to execute SSH command")?;

            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("SSH portforward process exited abnormally: {}", error);
            }
            let pid = get_pid(rule.local_port)?;
            let mut rules = load_rules()?;
            if let Some(rule) = rules.get_mut(name) {
                rule.pid = Some(pid);
                rule.status = true;
                save_rules(&rules)?;
            }
            println!("SSH port forward is running in background, PID: {}", pid);
        }
    }
    Ok(())
}

pub async fn start_forward(names: Vec<String>) -> Result<()> {
    let mut rules = load_rules()?;

    if names.is_empty() {
        start_selected(&mut rules).await?;
        return Ok(());
    }
    if names == vec!["all"] {
        start_all(&mut rules).await?;
        return Ok(());
    }

    start_input(names, &mut rules).await?;

    Ok(())
}
