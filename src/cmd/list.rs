use anyhow::Result;
use prettytable::{row, Table};
use crate::cmd::common::load_rules;

pub fn list_rules() -> Result<()> {
    let rules = load_rules()?;
    if rules.is_empty() {
        println!("No rules available.");
        return Ok(());
    }
    let mut table = Table::new();
    table.add_row(row![
        "RuleName",
        "LocalPort",
        "RemotePort",
        "RemoteHost",
        "Status",
        "PID"
    ]);
    for (name, rule) in rules {
        table.add_row(row![
            name,
            rule.local_port,
            rule.remote_port,
            rule.remote_host,
            if rule.status { "Running" } else { "Stopped" },
            rule.pid.map_or("".to_string(), |pid| pid.to_string())
        ]);
    }
    table.printstd();
    Ok(())
}