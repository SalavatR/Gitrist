use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use gitrust_server::WebSource;
use include_dir::{Dir, include_dir};

/// WASM bundle baked in at compile time. `build.rs` ensures the dir exists
/// (an empty bundle is OK — the server just 404s on `/`); a real bundle is
/// produced by `make web` and lives in `crates/gitrust-web/dist/`.
static EMBEDDED_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../gitrust-web/dist");

#[derive(Parser)]
#[command(version, about = "gitrust — Rust GUI git client (web + native)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server. Without --web-dist, the embedded bundle is used.
    Serve {
        #[arg(long, default_value = "127.0.0.1:3737")]
        addr: SocketAddr,
        /// Serve the WASM bundle from this directory instead of the embedded
        /// one. Useful during development with `make web`.
        #[arg(long)]
        web_dist: Option<PathBuf>,
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
        Command::Serve { addr, web_dist } => {
            let source = pick_web_source(web_dist);
            gitrust_server::serve(addr, source).await?;
        }
        Command::App => anyhow::bail!("`gitrust app` not implemented yet"),
    }
    Ok(())
}

fn pick_web_source(disk: Option<PathBuf>) -> WebSource {
    if let Some(dir) = disk {
        tracing::info!("serving WASM bundle from disk: {}", dir.display());
        return WebSource::Disk(dir);
    }
    if EMBEDDED_BUNDLE.get_file("index.html").is_some() {
        tracing::info!(
            "serving WASM bundle from embedded resources ({} files)",
            count_files(&EMBEDDED_BUNDLE)
        );
        return WebSource::Embedded(&EMBEDDED_BUNDLE);
    }
    tracing::warn!(
        "no WASM bundle available — UI will 404. Run `make web` and rebuild, \
         or pass --web-dist to point at a built bundle."
    );
    WebSource::None
}

fn count_files(dir: &Dir<'_>) -> usize {
    dir.files().count() + dir.dirs().map(count_files).sum::<usize>()
}
