use crate::cmd::common::{load_rules, load_ssh_config, PortForwardRule};

use dialoguer::Select;
use dialoguer::{Input, MultiSelect, theme::ColorfulTheme};

pub fn add_rule_form() -> anyhow::Result<(String, PortForwardRule)> {
    let hosts = load_ssh_config()?;
    if hosts.is_empty() {
        return Err(anyhow::anyhow!("No hosts found in ~/.ssh/config"));
    }
    let rules = load_rules()?;

    let name: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("RuleName:")
        .validate_with(|input: &String| {
            if rules.contains_key(input) {
                Err(format!("Name: '{}' already exists", input))
            } else {
                Ok(())
            }
        })
        .validate_with(|input: &String| {
            if input == "all" {
                Err(format!("'all' is keyword , please input another name."))
            } else {
                Ok(())
            }
        })
        .allow_empty(false)
        .interact_text()?;

    // let remote_host: String = Input::with_theme(&ColorfulTheme::default())
    //     .with_prompt("RemoteHost:") //Host in ~/.ssh/config
    //     .allow_empty(false)
    //     .interact_text()?;


    let section = Select::with_theme(&ColorfulTheme::default())
    .with_prompt("RemoteHost")
    .items(&hosts)
    .default(0)
    .interact()?;
    let remote_host = hosts[section].clone();

    // let remote_host: String = Select::with_theme(&ColorfulTheme::default())
    //     .with_prompt("RemoteHost:")
    //     .items(&hosts)
    //     .default(0)
    //     .interact()?;

    let local_port: u16 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("LocalPort:")
        .allow_empty(false)
        .interact_text()?;

    let remote_port: u16 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("RemotePort:")
        .allow_empty(false)
        .interact_text()?;

    let rule = PortForwardRule {
        local_port,
        remote_port,
        remote_host,
        status: false,
        pid: None,
    };
    Ok((name, rule))
}

pub fn update_rule_form(
    name: &String,
    rule: &PortForwardRule,
) -> anyhow::Result<(String, PortForwardRule)> {
    let rules = load_rules()?;
    let name: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("RuleName:")
        .with_initial_text(name)
        .validate_with(|input: &String| {
            if rules.contains_key(input) && input != name {
                Err(format!("Name: '{}' already exists", input))
            } else {
                Ok(())
            }
        })
        .allow_empty(false)
        .interact_text()?;

    let local_port: u16 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("LocalPort:")
        .with_initial_text(rule.local_port.to_string())
        .allow_empty(false)
        .interact_text()?;

    let remote_port: u16 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("RemotePort:")
        .with_initial_text(rule.remote_port.to_string())
        .allow_empty(false)
        .interact_text()?;

    let host: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("RemoteHost:")
        .with_initial_text(rule.remote_host.clone())
        .allow_empty(false)
        .interact_text()?;

    let rule = PortForwardRule {
        local_port,
        remote_port,
        remote_host: host,
        status: rule.status,
        pid: rule.pid,
    };
    Ok((name, rule))
}

pub fn select_rule() -> Option<String> {
    let names = match get_rules_names() {
        Ok(names) => names,
        Err(_) => {
            return None;
        }
    };

    match Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Please select a rule")
        .items(&names)
        .default(0)
        .interact_opt()
    {
        Ok(Some(index)) if index < names.len() - 1 => Some(names[index].clone()),
        _ => None,
    }
}

pub fn select_rules() -> Option<Vec<String>> {
    loop {
        let names = match get_rules_names() {
            Ok(names) => names,
            Err(_) => {
                return None;
            }
        };

        let selections = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Please select one or more rules")
            .items(&names)
            .interact();

        match selections {
            Ok(selected) if !selected.is_empty() => {
                return Some(
                    selected
                        .into_iter()
                        .map(|index| names[index].clone())
                        .collect(),
                );
            }
            Ok(_) => {
                let options = vec!["Yes, select again", "No, exit"];
                let selection = Select::new()
                    .with_prompt("No rules selected. Would you like to select again?")
                    .items(&options)
                    .default(0)
                    .interact();

                match selection {
                    Ok(0) => continue,
                    _ => return Some(vec![]),
                }
            }
            Err(_) => return Some(vec![]),
        }
    }
}

pub fn get_rules_names() -> anyhow::Result<Vec<String>> {
    let rules = load_rules()?;
    let keys: Vec<String> = rules.keys().cloned().collect();
    if keys.is_empty() {
        println!("No rules available.");
        return Ok(vec![]);
    }
    Ok(keys)
}


