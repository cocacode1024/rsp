use cli::Cli;
mod cli;
mod cmd;
mod interaction;
mod ssh;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Err(e) = Cli::run().await {
        eprintln!("{}", e);
    }
    std::process::exit(0);
}
