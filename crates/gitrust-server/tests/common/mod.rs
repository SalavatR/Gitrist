//! Test harness: builds a real git repo in a `TempDir`, spawns the axum
//! server on an OS-assigned port, and hands the test code back the URL +
//! repo path. Each test owns its own server task and repo so they don't
//! interfere with each other.

// Each integration-test binary compiles its own copy of this module via
// `#[path = "common/mod.rs"]`, so an item used only by one binary looks
// dead to the other. Silencing the lint here keeps both green without
// per-binary cfg gates.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub struct TestRepo {
    dir: TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("create tempdir");
        let path = dir.path();
        run(
            path,
            "git",
            &["-c", "init.defaultBranch=master", "init", "-q"],
        );
        run(path, "git", &["config", "user.name", "Test"]);
        run(path, "git", &["config", "user.email", "test@example.com"]);
        run(path, "git", &["config", "commit.gpgsign", "false"]);
        Self { dir }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Write `contents` to `rel`, creating parent dirs as needed.
    pub fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let p = self.path().join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir -p");
        }
        std::fs::write(&p, contents).expect("write file");
        p
    }

    /// Run `git` in the repo and return stdout. Panics on non-zero exit.
    pub fn git(&self, args: &[&str]) -> String {
        run(self.path(), "git", args)
    }
}

fn run(cwd: &Path, prog: &str, args: &[&str]) -> String {
    let out = Command::new(prog)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("spawn {prog}: {e}"));
    if !out.status.success() {
        panic!(
            "{prog} {args:?} failed (status {})\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout).unwrap_or_default()
}

/// Handle to a running test server. Drops the JoinHandle when dropped so
/// the server task is cancelled at test exit; the spawned future then runs
/// to completion (or aborts) without leaking.
pub struct ServerHandle {
    pub addr: SocketAddr,
    _task: JoinHandle<()>,
}

impl ServerHandle {
    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// Bind a fresh ephemeral port and start `gitrust_server::router` on it.
/// Returns once the listener is accepting connections.
pub async fn spawn_server() -> ServerHandle {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind 0");
    let addr = listener.local_addr().expect("local addr");
    let router = gitrust_server::router(gitrust_server::WebSource::None);
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    // Wait until the listener is actually accepting (the spawn returns
    // immediately; the task may not have started select-ing yet).
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return ServerHandle { addr, _task: task };
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server never became reachable on {addr}");
}
