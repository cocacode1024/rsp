use crate::cmd::common::{PortForwardRule, load_rules, load_ssh_config, save_rules};
use crate::utils::{get_pid, get_ssh_pids_for_rule, process_matches_rule};
use anyhow::{Context, Result, bail};
use std::process::Command;
use std::thread;
use std::time::Duration;

pub fn load_dashboard() -> Result<(Vec<(String, PortForwardRule)>, Vec<String>)> {
    let rules = refresh_status()?;
    let hosts = ssh_hosts()?;
    Ok((rules, hosts))
}

pub fn ssh_hosts() -> Result<Vec<String>> {
    let mut hosts = load_ssh_config()?;
    hosts.sort();
    hosts.dedup();
    Ok(hosts)
}

pub fn refresh_status() -> Result<Vec<(String, PortForwardRule)>> {
    let mut rules = load_rules()?;
    let mut changed = false;

    for rule in rules.values_mut() {
        let current_pid = match rule.pid {
            Some(pid) if process_matches_rule(pid, rule.local_port, rule.remote_port, &rule.remote_host).unwrap_or(false) => Some(pid),
            _ => get_pid(rule.local_port).ok(),
        };

        match current_pid {
            Some(pid) => {
                if rule.pid != Some(pid) || !rule.status {
                    rule.pid = Some(pid);
                    rule.status = true;
                    changed = true;
                }
            }
            None => {
                if rule.pid.is_some() || rule.status {
                    rule.pid = None;
                    rule.status = false;
                    changed = true;
                }
            }
        }
    }

    if changed {
        save_rules(&rules)?;
    }

    let mut list: Vec<_> = rules.into_iter().collect();
    list.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(list)
}

pub fn add_rule(name: String, rule: PortForwardRule) -> Result<()> {
    let mut rules = load_rules()?;
    validate_rule(&rules, &name, &rule, None)?;
    rules.insert(name, rule);
    save_rules(&rules)
}

pub fn update_rule(old_name: &str, new_name: String, rule: PortForwardRule) -> Result<()> {
    let mut rules = load_rules()?;
    let current = rules
        .get(old_name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Rule '{old_name}' not found"))?;

    if current.status {
        bail!("Rule '{old_name}' is running. Stop it before editing");
    }
    validate_rule(&rules, &new_name, &rule, Some(old_name))?;

    rules.remove(old_name);
    rules.insert(new_name, rule);
    save_rules(&rules)
}

pub fn remove_rule(name: &str) -> Result<()> {
    let mut rules = load_rules()?;
    let rule = rules
        .get(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Rule '{name}' not found"))?;
    if rule.status {
        bail!("Rule '{name}' is running. Stop it before deleting");
    }
    rules.remove(name);
    save_rules(&rules)
}

pub fn start_rules(names: &[String]) -> Result<()> {
    let mut rules = load_rules()?;
    for name in names {
        let rule = rules
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Rule '{name}' not found"))?;

        let existing_pids =
            get_ssh_pids_for_rule(rule.local_port, rule.remote_port, &rule.remote_host)
                .unwrap_or_default();
        if !existing_pids.is_empty() {
            for pid in existing_pids {
                kill_pid(pid).with_context(|| {
                    format!("Failed to replace existing SSH process for rule '{name}'")
                })?;
            }
            thread::sleep(Duration::from_millis(300));
        }
        check_host_forward_conflict(&rule)?;
        if get_pid(rule.local_port).is_ok() {
            bail!("Local port {} is already in use by another process", rule.local_port);
        }

        let output = Command::new("ssh")
            .arg("-f")
            .arg("-N")
            .arg("-C")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ExitOnForwardFailure=yes")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg("-L")
            .arg(format!(
                "127.0.0.1:{}:localhost:{}",
                rule.local_port, rule.remote_port
            ))
            .arg(&rule.remote_host)
            .output()
            .context("Failed to execute ssh command")?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to start '{name}': {}", error.trim());
        }

        let pid = wait_for_rule_ready(rule.local_port)
            .with_context(|| format!("Failed to start '{name}'"))?;
        if let Some(target) = rules.get_mut(name) {
            target.pid = Some(pid);
            target.status = true;
        }
    }
    save_rules(&rules)
}

pub fn stop_rules(names: &[String]) -> Result<()> {
    let mut rules = load_rules()?;
    for name in names {
        let rule = rules
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Rule '{name}' not found"))?;

        let pids = get_ssh_pids_for_rule(rule.local_port, rule.remote_port, &rule.remote_host)
            .unwrap_or_default();
        for pid in pids {
            kill_pid(pid).with_context(|| format!("Failed to stop '{name}'"))?;
        }

        if let Some(target) = rules.get_mut(name) {
            target.pid = None;
            target.status = false;
        }
    }
    save_rules(&rules)
}

fn kill_pid(pid: u32) -> Result<()> {
    let output = Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .output()
        .context("Failed to execute kill command")?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        bail!("{}", error.trim());
    }
    Ok(())
}

pub fn all_rule_names(rules: &[(String, PortForwardRule)]) -> Vec<String> {
    rules.iter().map(|(name, _)| name.clone()).collect()
}

pub fn make_rule(
    local_port: u16,
    remote_port: u16,
    remote_host: String,
    status: bool,
    pid: Option<u32>,
) -> PortForwardRule {
    PortForwardRule {
        local_port,
        remote_port,
        remote_host,
        status,
        pid,
    }
}

fn validate_rule(
    rules: &std::collections::HashMap<String, PortForwardRule>,
    name: &str,
    rule: &PortForwardRule,
    current_name: Option<&str>,
) -> Result<()> {
    if name.trim().is_empty() {
        bail!("Rule name cannot be empty");
    }
    if name == "all" {
        bail!("'all' is a reserved keyword");
    }
    if current_name != Some(name) && rules.contains_key(name) {
        bail!("Rule '{name}' already exists");
    }
    if rule.remote_host.trim().is_empty() {
        bail!("Remote host cannot be empty");
    }

    for (existing_name, existing_rule) in rules {
        if current_name == Some(existing_name.as_str()) {
            continue;
        }
        if existing_rule.local_port == rule.local_port {
            bail!(
                "Local port {} is already used by rule '{}'",
                rule.local_port,
                existing_name
            );
        }
    }
    Ok(())
}

fn wait_for_rule_ready(local_port: u16) -> Result<u32> {
    for _ in 0..40 {
        if let Ok(pid) = get_pid(local_port) {
            return Ok(pid);
        }
        thread::sleep(Duration::from_millis(250));
    }

    bail!(
        "ssh process started but local listener 127.0.0.1:{} was not established",
        local_port,
    )
}

fn check_host_forward_conflict(rule: &PortForwardRule) -> Result<()> {
    let config = std::fs::read_to_string(shellexpand::tilde("~/.ssh/config").into_owned())
        .unwrap_or_default();
    let mut in_target_host = false;

    for raw_line in config.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let lower = line.to_ascii_lowercase();
        if lower.starts_with("host ") {
            in_target_host = line[5..]
                .split_whitespace()
                .any(|name| name == rule.remote_host);
            continue;
        }

        if !in_target_host || !lower.starts_with("localforward ") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let local_port = parts[1]
            .rsplit(':')
            .next()
            .and_then(|value| value.parse::<u16>().ok());
        let remote_port = parts[2]
            .rsplit(':')
            .next()
            .and_then(|value| value.parse::<u16>().ok());

        if local_port == Some(rule.local_port) && remote_port != Some(rule.remote_port) {
            bail!(
                "SSH config host '{}' already defines LocalForward {} -> {}. This conflicts with rule '{}' ({} -> {}). Remove or change that LocalForward in ~/.ssh/config.",
                rule.remote_host,
                rule.local_port,
                parts[2],
                rule.remote_host,
                rule.local_port,
                rule.remote_port
            );
        }
    }

    Ok(())
}
