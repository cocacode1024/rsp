use crate::cmd::common::load_rules;
use anyhow::{Context, Ok, Result};
use std::process::Command;

pub fn check_exist(mut names: Vec<String>) -> Result<Vec<String>> {
    let rules = load_rules()?;
    names.retain(|name| {
        if !rules.contains_key(name) {
            println!("Rule '{}' not found.", name);
            false
        } else {
            true
        }
    });
    Ok(names)
}

pub fn get_pid(port: u16) -> Result<u32> {
    let lsof = Command::new("lsof")
        .arg(format!("-i:{}", port))
        .output()
        .context("Failed to execute lsof command")?;

    let lsof_output = String::from_utf8_lossy(&lsof.stdout);

    if let Some(line) = lsof_output.lines().nth(1) {
        if let Some(pid) = line.split_whitespace().nth(1) {
            let pid = pid.parse::<u32>()?;
            return Ok(pid);
        }
    }
    anyhow::bail!("Failed to get portforward process PID");
}
