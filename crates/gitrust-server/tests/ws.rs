//! WebSocket integration test for `/api/repo/events`. Verifies that a
//! worktree change reaches the client as a debounced `worktree_changed`
//! frame within a few seconds of the file event.

#[path = "common/mod.rs"]
mod common;

use std::time::Duration;

use common::{TestRepo, spawn_server};
use futures::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::test]
async fn worktree_change_emits_frame() {
    let server = spawn_server().await;
    let r = TestRepo::new();
    r.write("seed.txt", "v1\n");
    r.git(&["add", "seed.txt"]);
    r.git(&["commit", "-q", "-m", "init"]);

    // Let any straggler fs events from `git commit` settle before opening
    // the socket so they don't bleed into the frames we collect below.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let url = reqwest::Url::parse_with_params(
        &format!("ws://{}/api/repo/events", server.addr),
        &[
            ("path", r.path().to_str().unwrap()),
            ("token", "test-token"),
        ],
    )
    .expect("build ws url");

    let (mut ws, _resp) = connect_async(url.as_str()).await.expect("ws connect");

    // Give the server's notify watcher time to register before we trigger.
    tokio::time::sleep(Duration::from_millis(300)).await;

    r.write("seed.txt", "v2\n");

    // Collect every frame that lands within the next two seconds. The
    // server debounces at 150 ms, so any worktree event we caused should
    // appear well within this window. We don't assert on a specific
    // first-frame because notify can deliver additional unrelated kinds
    // (refs/index activity from background writes) and we only care
    // that *our* edit produced its expected category.
    let mut kinds: Vec<String> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                let s = t.to_string();
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s)
                    && let Some(k) = v["kind"].as_str()
                {
                    kinds.push(k.to_string());
                }
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => panic!("ws stream error: {e}"),
            Ok(None) => break,
            Err(_) => break,
        }
    }
    assert!(
        kinds.iter().any(|k| k == "worktree_changed"),
        "expected at least one `worktree_changed` frame after touching seed.txt, \
         got {kinds:?}"
    );
}
