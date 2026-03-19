use crate::cmd::common::{PortForwardRule, load_rules, load_ssh_config, save_rules};
use crate::utils::{get_listening_pids, get_pid, get_rule_process_pids, process_is_ssh, resolve_rule_pid};
use anyhow::{Context, Result, bail};
use std::process::{Command, Stdio};
use std::collections::BTreeSet;
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
        let current_pid = resolve_rule_pid(
            rule.pid,
            rule.local_port,
            rule.remote_port,
            &rule.remote_host,
        )
        .unwrap_or(None);

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

        let duplicate_rule_names: Vec<String> = rules
            .iter()
            .filter(|(other_name, other_rule)| {
                other_name.as_str() != name && other_rule.local_port == rule.local_port
            })
            .map(|(other_name, _)| other_name.clone())
            .collect();

        let mut running_duplicate_rules = Vec::new();
        let mut stale_duplicate_rules = Vec::new();
        for other_name in duplicate_rule_names {
            let Some(other_rule) = rules.get(&other_name).cloned() else {
                continue;
            };

            let duplicate_pid = resolve_rule_pid(
                other_rule.pid,
                other_rule.local_port,
                other_rule.remote_port,
                &other_rule.remote_host,
            )?;

            if duplicate_pid.is_some() {
                running_duplicate_rules.push(other_name);
            } else {
                stale_duplicate_rules.push(other_name);
            }
        }

        if !running_duplicate_rules.is_empty() {
            bail!(
                "Local port {} is already occupied by running rule(s): {}. Stop that rule first before starting '{}'.",
                rule.local_port,
                running_duplicate_rules.join(", "),
                name
            );
        }

        for stale_name in stale_duplicate_rules {
            if let Some(stale_rule) = rules.get_mut(&stale_name) {
                if stale_rule.status || stale_rule.pid.is_some() {
                    stale_rule.status = false;
                    stale_rule.pid = None;
                }
            }
        }

        let existing_pids =
            get_rule_process_pids(rule.local_port, rule.remote_port, &rule.remote_host)
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
        if let Ok(listeners) = get_listening_pids(rule.local_port) {
            let has_foreign_listener = listeners.into_iter().any(|pid| {
                !get_rule_process_pids(rule.local_port, rule.remote_port, &rule.remote_host)
                    .unwrap_or_default()
                    .contains(&pid)
            });
            if has_foreign_listener {
                bail!(
                    "Local port {} is already in use by another process",
                    rule.local_port
                );
            }
        }

        let status = Command::new("ssh")
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
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("Failed to execute ssh command")?;

        if !status.success() {
            bail!("Failed to start '{name}': ssh exited with status {}", status);
        }

        let pid = wait_for_rule_ready(rule.pid, rule.local_port, rule.remote_port, &rule.remote_host)
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

        let mut pids = BTreeSet::new();

        if let Some(pid) = rule.pid {
            pids.insert(pid);
        }

        for pid in get_rule_process_pids(rule.local_port, rule.remote_port, &rule.remote_host)
            .unwrap_or_default()
        {
            pids.insert(pid);
        }

        for pid in get_listening_pids(rule.local_port).unwrap_or_default() {
            if process_is_ssh(pid).unwrap_or(false) {
                pids.insert(pid);
            }
        }

        for pid in pids {
            match kill_pid(pid) {
                Ok(()) => {}
                Err(err) if is_missing_process_error(&err) => {}
                Err(err) => return Err(err).with_context(|| format!("Failed to stop '{name}'")),
            }
        }

        wait_for_rule_stopped(rule.local_port, rule.remote_port, &rule.remote_host)
            .with_context(|| format!("Failed to stop '{name}'"))?;

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

fn is_missing_process_error(err: &anyhow::Error) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("no such process")
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
    if current_name != Some(name) && rules.contains_key(name) {
        bail!("Rule '{name}' already exists");
    }
    if rule.remote_host.trim().is_empty() {
        bail!("Remote host cannot be empty");
    }
    Ok(())
}

fn wait_for_rule_ready(
    saved_pid: Option<u32>,
    local_port: u16,
    remote_port: u16,
    remote_host: &str,
) -> Result<u32> {
    let mut stable_pid = None;
    let mut stable_samples = 0;

    for _ in 0..50 {
        if let Some(pid) = resolve_rule_pid(saved_pid, local_port, remote_port, remote_host)? {
            if stable_pid == Some(pid) {
                stable_samples += 1;
            } else {
                stable_pid = Some(pid);
                stable_samples = 1;
            }

            if stable_samples >= 50 {
                return Ok(pid);
            }
        } else {
            stable_pid = None;
            stable_samples = 0;
        }
        thread::sleep(Duration::from_millis(100));
    }

    bail!(
        "ssh process started but local listener 127.0.0.1:{} was not established",
        local_port,
    )
}

fn wait_for_rule_stopped(local_port: u16, remote_port: u16, remote_host: &str) -> Result<()> {
    for _ in 0..50 {
        if resolve_rule_pid(None, local_port, remote_port, remote_host)?.is_none() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    if get_pid(local_port).is_err() {
        return Ok(());
    }

    bail!(
        "ssh listener for 127.0.0.1:{} is still running after stop",
        local_port
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
