//! HTTP-level integration tests for the API surface. Each test spawns a
//! fresh server on an ephemeral port and a fresh git repo in a tempdir,
//! then hits the endpoint with `reqwest`.

#[path = "common/mod.rs"]
mod common;

use common::{ServerHandle, TestRepo, spawn_server};
use serde_json::Value;

async fn get_json(server: &ServerHandle, path: &str, query: &[(&str, &str)]) -> (u16, Value) {
    let resp = reqwest::Client::new()
        .get(format!("{}{path}", server.base()))
        .query(query)
        .send()
        .await
        .expect("send");
    let status = resp.status().as_u16();
    let body = resp.json::<Value>().await.expect("json");
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
