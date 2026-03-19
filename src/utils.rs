use crate::cmd::common::load_rules;
use anyhow::{Context, Result};
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
    let pids = get_listening_pids(port)?;
    pids.into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to get portforward process PID"))
}

pub fn get_listening_pids(port: u16) -> Result<Vec<u32>> {
    let lsof = Command::new("lsof")
        .arg("-nP")
        .arg(format!("-iTCP:{}", port))
        .arg("-sTCP:LISTEN")
        .arg("-t")
        .output()
        .context("Failed to execute lsof command")?;

    let mut pids = Vec::new();
    for line in String::from_utf8_lossy(&lsof.stdout).lines() {
        let pid = match line.trim().parse::<u32>() {
            Ok(pid) => pid,
            Err(_) => continue,
        };
        pids.push(pid);
    }
    Ok(pids)
}

pub fn get_rule_process_pids(local_port: u16, remote_port: u16, remote_host: &str) -> Result<Vec<u32>> {
    let ps = Command::new("ps")
        .arg("-axo")
        .arg("pid=,command=")
        .output()
        .context("Failed to execute ps command")?;

    let signature = format!("127.0.0.1:{}:localhost:{}", local_port, remote_port);
    let legacy_signature = format!("{}:localhost:{}", local_port, remote_port);
    let mut pids = Vec::new();
    for line in String::from_utf8_lossy(&ps.stdout).lines() {
        let trimmed = line.trim();
        let mut parts = trimmed.split_whitespace();
        let pid = match parts.next().and_then(|value| value.parse::<u32>().ok()) {
            Some(pid) => pid,
            None => continue,
        };
        let command_line = parts.collect::<Vec<_>>().join(" ");
        if command_line.contains("ssh")
            && command_line.contains(remote_host)
            && command_line.contains("-L")
            && (command_line.contains(&signature) || command_line.contains(&legacy_signature))
        {
            pids.push(pid);
        }
    }

    Ok(pids)
}

pub fn resolve_rule_pid(
    saved_pid: Option<u32>,
    local_port: u16,
    remote_port: u16,
    remote_host: &str,
) -> Result<Option<u32>> {
    if let Some(pid) = saved_pid {
        if process_matches_rule(pid, local_port, remote_port, remote_host)? {
            return Ok(Some(pid));
        }
    }

    for pid in get_rule_process_pids(local_port, remote_port, remote_host)? {
        return Ok(Some(pid));
    }

    for pid in get_listening_pids(local_port)? {
        if process_matches_rule(pid, local_port, remote_port, remote_host)? {
            return Ok(Some(pid));
        }
    }

    Ok(None)
}

pub fn process_is_ssh(pid: u32) -> Result<bool> {
    let ps = Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("command=")
        .output()
        .context("Failed to execute ps command")?;

    if !ps.status.success() {
        return Ok(false);
    }

    Ok(String::from_utf8_lossy(&ps.stdout).contains("ssh"))
}

pub fn process_matches_rule(pid: u32, local_port: u16, remote_port: u16, remote_host: &str) -> Result<bool> {
    let ps = Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("command=")
        .output()
        .context("Failed to execute ps command")?;

    if !ps.status.success() {
        return Ok(false);
    }

    let command_line = String::from_utf8_lossy(&ps.stdout);
    let loopback_signature = format!("127.0.0.1:{}:localhost:{}", local_port, remote_port);
    let legacy_signature = format!("{}:localhost:{}", local_port, remote_port);

    Ok(
        command_line.contains("ssh")
            && command_line.contains(remote_host)
            && command_line.contains("-L")
            && (command_line.contains(&loopback_signature) || command_line.contains(&legacy_signature)),
    )
}
