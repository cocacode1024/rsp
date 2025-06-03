use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PortForwardRule {
    pub local_port: u16,
    pub remote_port: u16,
    pub remote_host: String,
    pub status: bool,
    pub pid: Option<u32>,
}

const RULES_FILE: &str = "~/.rsp.json";

pub fn load_rules() -> Result<HashMap<String, PortForwardRule>> {
    let path = shellexpand::tilde(RULES_FILE).into_owned();
    if !Path::new(&path).exists() {
        fs::write(&path, "{}")?;
        return Ok(HashMap::new());
    }
    let data = fs::read_to_string(&path)?;
    if data.trim().is_empty() {
        return Ok(HashMap::new());
    }
    let rules: HashMap<String, PortForwardRule> = serde_json::from_str(&data)?;
    Ok(rules)
}
pub fn save_rules(rules: &HashMap<String, PortForwardRule>) -> Result<()> {
    let path = shellexpand::tilde(RULES_FILE).into_owned();
    let data = serde_json::to_string_pretty(rules)?;
    fs::write(path, data)?;
    Ok(())
}




