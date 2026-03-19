mod cli;
mod cmd;
mod gui;
mod interaction;
mod services;
mod utils;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Err(e) = cli::Cli::run().await {
        eprintln!("{}", e);
    }
    std::process::exit(0);
}
