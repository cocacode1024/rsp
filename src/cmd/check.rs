use super::common::PortForwardRule;
use super::start::start_forward_force;
use super::stop::stop_forward_force;
use crate::cmd::common::load_rules;
use crate::interaction::select_rules;
use crate::ssh::get_pid;
use anyhow::Result;
use dialoguer::Select;
use std::collections::HashMap;

async fn check_running(names: Vec<String>, rules: HashMap<String, PortForwardRule>) -> Result<()> {
    let mut not_started_rules = vec![];
    let mut running_rules = vec![];
    let mut stoped_rules = vec![];

    for name in &names {
        if let Some(rule) = rules.get(name) {
            if rule.status {
                match get_pid(rule.local_port) {
                    Ok(_) => running_rules.push(name.to_string()),
                    Err(_) => stoped_rules.push(name.to_string()),
                }
            } else {
                if let Err(_) = get_pid(rule.local_port) {
                    not_started_rules.push(name.to_string());
                }
            }
        }
    }
    if !not_started_rules.is_empty() {
        println!(
            "{} rule{} {} not running: {}, you can start {} by 'rsp start {}'.",
            if not_started_rules.len() == 1 { "This" } else { "These" },
            if not_started_rules.len() == 1 { "" } else { "s" },
            if not_started_rules.len() == 1 { "is" } else { "are" },
            not_started_rules.join(", "),
            if not_started_rules.len() == 1 { "it" } else { "them" },
            not_started_rules.join(" ")
        );
    }
    if !running_rules.is_empty() {
        println!(
            "{} rule{} {} running: {}, you can stop {} by 'rsp stop {}'.",
            if running_rules.len() == 1 { "This" } else { "These" },
            if running_rules.len() == 1 { "" } else { "s" },
            if running_rules.len() == 1 { "is" } else { "are" },
            running_rules.join(", "),
            if running_rules.len() == 1 { "it" } else { "them" },
            running_rules.join(" ")
        );
    }
    if stoped_rules.is_empty() {
        return Ok(());
    }

    let options = vec!["Yes", "No"];
    let selection = Select::new()
        .with_prompt(format!(
            "{} rule{} {} terminated abnormally: {}. Do you want to restart?",
            if stoped_rules.len() == 1 { "This" } else { "These" },
            if stoped_rules.len() == 1 { "" } else { "s" },
            if stoped_rules.len() == 1 { "has" } else { "have" },
            stoped_rules.join(", ")
        ))
        .items(&options)
        .default(1)
        .interact()?;
    if selection == 0 {
        for name in &stoped_rules {
            if let Some(rule) = rules.get(name) {
                if rule.status {
                    start_forward_force(name.to_string(), rules.clone()).await?;
                }
            }
        }
    } else {
        stop_forward_force(stoped_rules, rules).await?
    }
    Ok(())
}

async fn check_all(rules: HashMap<String, PortForwardRule>) -> Result<()> {
    let names = rules.keys().cloned().collect::<Vec<String>>();
    check_running(names, rules).await?;
    Ok(())
}

async fn check_input(names: Vec<String>, rules: HashMap<String, PortForwardRule>) -> Result<()> {
    let mut not_found_rules = vec![];
    let mut found_rules = vec![];
    for name in &names {
        match rules.get(name) {
            Some(_) => found_rules.push(name.to_string()),
            None => not_found_rules.push(name.to_string()),
        }
    }
    if !not_found_rules.is_empty() {
        println!("Rules: {} is not found.", not_found_rules.join(","));
    }
    check_running(found_rules, rules).await?;
    Ok(())
}

async fn check_selected(rules: HashMap<String, PortForwardRule>) -> Result<()> {
    let names = select_rules().unwrap_or_default();
    if names.is_empty() {
        println!("There is no rule to check.");
        return Ok(());
    };
    check_running(names, rules).await?;

    Ok(())
}

pub async fn check_rules(names: Vec<String>) -> Result<()> {
    let rules = load_rules()?;
    if names.is_empty() {
        check_selected(rules).await?;
        return Ok(());
    };

    if names == vec!["all"] {
        check_all(rules).await?;
        return Ok(());
    };

    check_input(names, rules).await?;

    Ok(())
}
