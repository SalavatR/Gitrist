use std::net::SocketAddr;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "gitrust — Rust GUI git client (web + native)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve {
        #[arg(long, default_value = "127.0.0.1:3737")]
        addr: SocketAddr,
    },
    App,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,gitrust=debug,gitrust_server=debug".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Serve { addr } => gitrust_server::serve(addr).await?,
        Command::App => anyhow::bail!("`gitrust app` not implemented yet"),
    }
    Ok(())
}
