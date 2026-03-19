use crate::cmd::add::add_rule;
use crate::cmd::check::check_rules;
use crate::cmd::edit::edit_rule;
use crate::cmd::list::list_rules;
use crate::cmd::remove::remove_rules;
use crate::cmd::start::start_forward;
use crate::cmd::stop::stop_forward;
use crate::gui;
use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "rst")]
#[command(about = "A SSH-based portforward tool", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(name = "add", about = "Add a new rule", disable_help_flag = true)]
    Add,
    #[command(
        name = "remove",
        about = "Remove a rule or rules",
        visible_alias = "rm",
        allow_missing_positional = true,
        disable_help_flag = true
    )]
    Remove(Rulenames),
    #[command(
        name = "edit",
        about = "Edit a rule",
        allow_missing_positional = true,
        disable_help_flag = false
    )]
    Edit(Rulename),
    #[command(
        name = "list",
        about = "List all rules",
        visible_alias = "ls",
        disable_help_flag = true
    )]
    List,
    #[command(
        name = "start",
        about = "Start one or all portforward",
        disable_help_flag = false
    )]
    Start(Rulenames),
    #[command(
        name = "stop",
        about = "Stop one or all portforward",
        disable_help_flag = false
    )]
    Stop(Rulenames),
    #[command(
        name = "check",
        about = "Check rules status",
        disable_help_flag = false
    )]
    Check(Rulenames),
    #[command(name = "gui", about = "Launch the graphical interface")]
    Gui,
}

#[derive(Debug, Args)]
pub struct Rulenames {
    #[arg(help = "Rule names to operate on. Use 'all' to operate on all rules")]
    pub names: Vec<String>,
}

#[derive(Debug, Args)]
pub struct Rulename {
    pub names: Option<String>,
}

impl Cli {
    pub async fn run() -> anyhow::Result<()> {
        let cli = Cli::parse();
        match &cli.command {
            None | Some(Commands::Gui) => {
                gui::run()?;
            }
            Some(Commands::Add) => {
                add_rule()?;
            }
            Some(Commands::Remove(args)) => {
                let names = args.names.clone();
                remove_rules(names)?;
            }
            Some(Commands::Edit(name)) => {
                let name = name.names.clone().unwrap_or_default();
                edit_rule(name)?;
            }
            Some(Commands::List) => {
                list_rules()?;
            }
            Some(Commands::Start(args)) => {
                let names = args.names.clone();
                start_forward(names).await?;
            }
            Some(Commands::Stop(args)) => {
                let names = args.names.clone();
                stop_forward(names).await?;
            }
            Some(Commands::Check(args)) => {
                let names = args.names.clone();
                check_rules(names).await?;
            }
        }
        Ok(())
    }
}
