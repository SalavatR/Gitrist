use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use gitrust_server::WebSource;
use include_dir::{Dir, include_dir};

/// WASM bundle baked in at compile time. `build.rs` ensures the dir exists
/// (an empty bundle is OK — the server detects that case at startup); a
/// real bundle is produced by `make web` and lives in
/// `crates/gitrust-web/dist/`.
static EMBEDDED_BUNDLE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../gitrust-web/dist");

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
    /// Open the UI in a native window. Falls back to printing a URL on
    /// platforms without a usable webview (headless build, no display).
    App {
        #[arg(long, default_value = "127.0.0.1:3737")]
        addr: SocketAddr,
        /// Skip the window; just run the server and print the URL.
        #[arg(long)]
        no_window: bool,
    },
}

fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    match cli.command {
        Command::Serve { addr, web_dist } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let source = pick_web_source(web_dist);
                gitrust_server::serve(addr, source).await
            })?;
            Ok(())
        }
        Command::App { addr, no_window } => run_app(addr, no_window),
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,gitrust=debug,gitrust_server=debug".into()),
        )
        .init();
}

fn run_app(addr: SocketAddr, no_window: bool) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let source = pick_web_source(None);
    let server_handle = rt.spawn(async move { gitrust_server::serve(addr, source).await });
    rt.block_on(wait_listening(addr))?;
    let url = format!("http://{addr}");

    if !no_window && desktop_supported() {
        match open_window(&url) {
            // The successful path enters tao's event loop and never returns;
            // process exits when the window is closed.
            Err(e) => {
                tracing::warn!("native window unavailable ({e}); falling back to URL mode");
            }
            #[allow(unreachable_patterns)]
            Ok(()) => return Ok(()),
        }
    }

    print_url_banner(&url);
    rt.block_on(server_handle)??;
    Ok(())
}

async fn wait_listening(addr: SocketAddr) -> Result<()> {
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("server did not start listening on {addr}")
}

fn print_url_banner(url: &str) {
    println!();
    println!("  gitrust ready at {url}");
    println!("  Open this URL in a browser. Ctrl+C to stop.");
    println!();
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

fn desktop_supported() -> bool {
    #[cfg(not(feature = "desktop"))]
    {
        false
    }
    #[cfg(feature = "desktop")]
    {
        #[cfg(target_os = "linux")]
        {
            std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok()
        }
        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }
}

#[cfg(feature = "desktop")]
fn open_window(url: &str) -> Result<()> {
    use tao::event::{ElementState, Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::keyboard::{Key, ModifiersState};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("gitrust")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 800.0))
        .build(&event_loop)?;
    let webview = WebViewBuilder::new().with_url(url).build(&window)?;

    let mut modifiers = ModifiersState::empty();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::ModifiersChanged(new_mods),
                ..
            } => {
                modifiers = new_mods;
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { event: ev, .. },
                ..
            } if ev.state == ElementState::Pressed => {
                // Primary modifier: Cmd on macOS, Ctrl elsewhere — matches
                // how Quit / Reload / Close shortcuts feel native on each OS.
                let primary = if cfg!(target_os = "macos") {
                    modifiers.super_key()
                } else {
                    modifiers.control_key()
                };
                if !primary {
                    return;
                }
                if let Key::Character(s) = &ev.logical_key {
                    match s.to_ascii_lowercase().as_str() {
                        "r" => {
                            let _ = webview.load_url(url);
                        }
                        "q" | "w" => {
                            *control_flow = ControlFlow::Exit;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    });
}

#[cfg(not(feature = "desktop"))]
fn open_window(_url: &str) -> Result<()> {
    anyhow::bail!(
        "`desktop` feature not compiled in — use --no-window or rebuild with `--features desktop`"
    )
}
