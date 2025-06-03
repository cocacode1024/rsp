use anyhow::{Context, Ok};
use std::process::Command;
use crate::cmd::common::{load_rules, save_rules, PortForwardRule};

pub async fn portforward(name: String, rule: &PortForwardRule) -> anyhow::Result<()> {
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
    if let Some(rule) = rules.get_mut(&name) {
        rule.pid = Some(pid);
        rule.status = true;
        save_rules(&rules)?;
    }
    println!("SSH port forward is running in background, PID: {}", pid);
    Ok(())
}

pub fn get_pid(port: u16) -> anyhow::Result<u32> {
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
