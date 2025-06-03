use anyhow::Result;
use crate::cmd::common::{load_rules, save_rules};
use crate::interaction::add_rule_form;

pub fn add_rule() -> Result<()> {
    let mut rules = load_rules()?;
    let (name, rule) = add_rule_form()?;
    if rules.contains_key(&name) {
        anyhow::bail!("Rule already exists, please choose a different name.");
    }
    rules.insert(name, rule);
    save_rules(&rules)?;
    Ok(())
}