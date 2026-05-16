//! HTTP-level integration tests for the API surface. Each test spawns a
//! fresh server on an ephemeral port and a fresh git repo in a tempdir,
//! then hits the endpoint with `reqwest`.

#[path = "common/mod.rs"]
mod common;

use common::{ServerHandle, TestRepo, spawn_server};
use serde_json::Value;

async fn get_json(server: &ServerHandle, path: &str, query: &[(&str, &str)]) -> (u16, Value) {
    get_json_with_auth(server, path, query, Some("test-token")).await
}

async fn get_json_with_auth(
    server: &ServerHandle,
    path: &str,
    query: &[(&str, &str)],
    auth: Option<&str>,
) -> (u16, Value) {
    let mut req = reqwest::Client::new()
        .get(format!("{}{path}", server.base()))
        .query(query);
    if let Some(token) = auth {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    let resp = req.send().await.expect("send");
    let status = resp.status().as_u16();
    let body = resp.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

async fn setup_with_initial_commit() -> (ServerHandle, TestRepo) {
    let server = spawn_server().await;
    let r = TestRepo::new();
    r.write("a.txt", "hello\n");
    r.git(&["add", "a.txt"]);
    r.git(&["commit", "-q", "-m", "initial"]);
    (server, r)
}

#[tokio::test]
async fn health_returns_ok_status_and_version() {
    let server = spawn_server().await;
    let (status, body) = get_json(&server, "/api/health", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
}

#[tokio::test]
async fn health_is_open_no_auth_required() {
    let server = spawn_server().await;
    let (status, body) = get_json_with_auth(&server, "/api/health", &[], None).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn protected_read_without_auth_returns_401() {
    let (server, r) = setup_with_initial_commit().await;
    let (status, _) = get_json_with_auth(
        &server,
        "/api/repo/summary",
        &[("path", r.path().to_str().unwrap())],
        None,
    )
    .await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn protected_read_with_query_token_works() {
    let (server, r) = setup_with_initial_commit().await;
    // ?token=… is the WebSocket-friendly auth channel; verify it works
    // for plain HTTP too so the path stays single-purpose.
    let (status, body) = get_json_with_auth(
        &server,
        "/api/repo/summary",
        &[
            ("path", r.path().to_str().unwrap()),
            ("token", "test-token"),
        ],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["head_ref"], "master");
}

#[tokio::test]
async fn summary_returns_head_for_real_repo() {
    let (server, r) = setup_with_initial_commit().await;
    let (status, body) = get_json(
        &server,
        "/api/repo/summary",
        &[("path", r.path().to_str().unwrap())],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["head_ref"], "master");
    assert!(body["head_oid"].as_str().is_some_and(|o| o.len() == 40));
    assert_eq!(body["is_detached"], false);
}

#[tokio::test]
async fn log_returns_commits_newest_first() {
    let (server, r) = setup_with_initial_commit().await;
    r.write("b.txt", "more\n");
    r.git(&["add", "b.txt"]);
    r.git(&["commit", "-q", "-m", "second"]);

    let (status, body) = get_json(
        &server,
        "/api/repo/log",
        &[("path", r.path().to_str().unwrap())],
    )
    .await;
    assert_eq!(status, 200);
    let arr = body.as_array().expect("array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["summary"], "second");
    assert_eq!(arr[1]["summary"], "initial");
}

#[tokio::test]
async fn branches_marks_head() {
    let (server, r) = setup_with_initial_commit().await;
    let (status, body) = get_json(
        &server,
        "/api/repo/branches",
        &[("path", r.path().to_str().unwrap())],
    )
    .await;
    assert_eq!(status, 200);
    let arr = body.as_array().expect("array");
    let master = arr.iter().find(|b| b["name"] == "master").expect("master");
    assert_eq!(master["is_head"], true);
}

#[tokio::test]
async fn staged_returns_index_vs_head_entries() {
    let (server, r) = setup_with_initial_commit().await;
    r.write("a.txt", "v2\n");
    r.git(&["add", "a.txt"]);

    let (status, body) = get_json(
        &server,
        "/api/repo/staged",
        &[("path", r.path().to_str().unwrap())],
    )
    .await;
    assert_eq!(status, 200);
    let arr = body.as_array().expect("array");
    assert!(
        arr.iter()
            .any(|e| e["path"] == "a.txt" && e["kind"] == "modified")
    );
}

#[tokio::test]
async fn status_reports_modified_file() {
    let (server, r) = setup_with_initial_commit().await;
    r.write("a.txt", "changed\n");

    let (status, body) = get_json(
        &server,
        "/api/repo/status",
        &[("path", r.path().to_str().unwrap())],
    )
    .await;
    assert_eq!(status, 200);
    let arr = body.as_array().expect("array");
    assert!(
        arr.iter()
            .any(|e| e["path"] == "a.txt" && e["kind"] == "modified"),
        "expected a.txt modified entry, got {arr:?}"
    );
}

#[tokio::test]
async fn diff_returns_per_file_hunks() {
    let (server, r) = setup_with_initial_commit().await;
    r.write("a.txt", "changed\n");
    r.git(&["add", "a.txt"]);
    r.git(&["commit", "-q", "-m", "update"]);
    let oid = r.git(&["rev-parse", "HEAD"]).trim().to_string();

    let (status, body) = get_json(
        &server,
        "/api/repo/diff",
        &[("path", r.path().to_str().unwrap()), ("oid", &oid)],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["commit"]["summary"], "update");
    let files = body["files"].as_array().expect("files");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "a.txt");
    assert_eq!(files[0]["kind"], "modified");
}

async fn post_json(
    server: &ServerHandle,
    path: &str,
    auth: Option<&str>,
    body: Value,
) -> (u16, Value) {
    let mut req = reqwest::Client::new().post(format!("{}{path}", server.base()));
    if let Some(token) = auth {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    let resp = req.json(&body).send().await.expect("send");
    let status = resp.status().as_u16();
    // Some success cases return no body (204) — be tolerant.
    let body = resp.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn stage_without_auth_returns_401() {
    let (server, r) = setup_with_initial_commit().await;
    r.write("new.txt", "x\n");
    let (status, _) = post_json(
        &server,
        "/api/repo/stage",
        None,
        serde_json::json!({
            "path": r.path().to_str().unwrap(),
            "files": ["new.txt"],
        }),
    )
    .await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn stage_with_auth_places_file_in_index() {
    let (server, r) = setup_with_initial_commit().await;
    r.write("new.txt", "x\n");
    let (status, _) = post_json(
        &server,
        "/api/repo/stage",
        Some("test-token"),
        serde_json::json!({
            "path": r.path().to_str().unwrap(),
            "files": ["new.txt"],
        }),
    )
    .await;
    assert_eq!(status, 204);
    let cached = r.git(&["ls-files", "--cached"]);
    assert!(cached.lines().any(|l| l == "new.txt"));
}

#[tokio::test]
async fn unstage_with_auth_drops_index_entry() {
    let (server, r) = setup_with_initial_commit().await;
    r.write("new.txt", "x\n");
    r.git(&["add", "new.txt"]);
    let (status, _) = post_json(
        &server,
        "/api/repo/unstage",
        Some("test-token"),
        serde_json::json!({
            "path": r.path().to_str().unwrap(),
            "files": ["new.txt"],
        }),
    )
    .await;
    assert_eq!(status, 204);
    let cached = r.git(&["ls-files", "--cached"]);
    assert!(!cached.lines().any(|l| l == "new.txt"));
}

#[tokio::test]
async fn commit_with_auth_creates_new_commit() {
    let (server, r) = setup_with_initial_commit().await;
    r.write("a.txt", "v2\n");
    r.git(&["add", "a.txt"]);
    let (status, body) = post_json(
        &server,
        "/api/repo/commit",
        Some("test-token"),
        serde_json::json!({
            "path": r.path().to_str().unwrap(),
            "message": "second",
        }),
    )
    .await;
    assert_eq!(status, 200);
    let oid = body["oid"].as_str().expect("oid string");
    assert_eq!(oid.len(), 40);
    let log = r.git(&["log", "--oneline"]);
    assert_eq!(log.lines().count(), 2);
    assert!(log.lines().next().unwrap().contains("second"));
}

#[tokio::test]
async fn nonexistent_repo_returns_400_with_error_envelope() {
    let server = spawn_server().await;
    let missing = format!("/nonexistent-test-repo-{}", std::process::id());
    let (status, body) =
        get_json(&server, "/api/repo/summary", &[("path", missing.as_str())]).await;
    assert_eq!(status, 400);
    assert!(
        body["error"].is_string(),
        "expected error message, got {body:?}"
    );
}
