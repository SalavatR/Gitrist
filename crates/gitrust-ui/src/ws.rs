//! Live WebSocket push-refresh client. Maintains one socket per repo,
//! reconnects with exponential backoff, and dispatches incoming
//! `{kind}` frames to the affected `use_resource` restarts.
//!
//! Compiled only on `wasm32` — there's no use case for a native client
//! talking to the same `localhost` server.

#![cfg(target_arch = "wasm32")]

use dioxus::prelude::*;
use gitrust_types::{
    BranchInfo, CommitInfo, RemoteBranchInfo, RepoSummary, StatusEntry, TagInfo, TreeEntry,
};

use crate::time_fmt::sleep_ms;

#[derive(Copy, Clone)]
pub(crate) struct LiveResources {
    pub summary: Resource<Result<RepoSummary, String>>,
    pub log: Resource<Result<Vec<CommitInfo>, String>>,
    pub status: Resource<Result<Vec<StatusEntry>, String>>,
    pub staged: Resource<Result<Vec<StatusEntry>, String>>,
    pub branches: Resource<Result<Vec<BranchInfo>, String>>,
    pub tags: Resource<Result<Vec<TagInfo>, String>>,
    pub remotes: Resource<Result<Vec<RemoteBranchInfo>, String>>,
    pub tree: Resource<Result<Vec<TreeEntry>, String>>,
}

impl LiveResources {
    fn dispatch(mut self, kind: &str) {
        match kind {
            "head_changed" => {
                self.summary.restart();
                self.log.restart();
                self.status.restart();
                self.staged.restart();
                self.branches.restart();
                self.tags.restart();
                self.remotes.restart();
                self.tree.restart();
            }
            "refs_changed" => {
                self.summary.restart();
                self.log.restart();
                self.branches.restart();
                self.tags.restart();
                self.remotes.restart();
                self.tree.restart();
            }
            "index_changed" => {
                self.status.restart();
                self.staged.restart();
            }
            "worktree_changed" => {
                self.status.restart();
            }
            _ => {}
        }
    }
}

#[derive(serde::Deserialize)]
struct EventMsg {
    kind: String,
}

pub(crate) async fn run_event_stream(path: String, live: LiveResources) {
    use futures::StreamExt;
    use gloo_net::websocket::{Message, futures::WebSocket};

    if path.is_empty() {
        return;
    }
    let url = format!(
        "{}/api/repo/events?path={}",
        ws_origin(),
        urlencoding::encode(&path),
    );

    let mut backoff_ms: u32 = 500;
    loop {
        let ws = match WebSocket::open(&url) {
            Ok(w) => w,
            Err(_) => {
                sleep_ms(backoff_ms).await;
                backoff_ms = backoff_ms.saturating_mul(2).min(30_000);
                continue;
            }
        };
        backoff_ms = 500;
        let (_write, mut read) = ws.split();
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(t)) => {
                    if let Ok(e) = serde_json::from_str::<EventMsg>(&t) {
                        live.dispatch(&e.kind);
                    }
                }
                Ok(Message::Bytes(_)) => {}
                Err(_) => break,
            }
        }
        sleep_ms(backoff_ms).await;
        backoff_ms = backoff_ms.saturating_mul(2).min(30_000);
    }
}

fn ws_origin() -> String {
    let window = gloo_utils::window();
    let loc = window.location();
    let proto = loc.protocol().unwrap_or_else(|_| "http:".into());
    let host = loc.host().unwrap_or_else(|_| "localhost:3737".into());
    let ws_proto = if proto == "https:" { "wss:" } else { "ws:" };
    format!("{ws_proto}//{host}")
}
