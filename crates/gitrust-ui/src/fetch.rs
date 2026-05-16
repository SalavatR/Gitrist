//! Typed HTTP wrappers over the JSON API. Each function exists in two
//! flavours: a real `gloo_net`-backed call on `wasm32` and a stub that
//! errors out on native (the UI lives in the browser; the native build
//! only needs the crate to compile for `cargo check` / docs).

use gitrust_types::{
    BlameView, BlobView, BranchInfo, CommitDiff, CommitInfo, FileDiff, RemoteBranchInfo,
    RepoSummary, StashEntry, StatusEntry, TagInfo, TreeEntry,
};

// `q` percent-encodes a single query-string value. Paths can contain
// spaces, `#`, `&`, `?` and non-ASCII — letting them through raw would
// break URL parsing or get truncated at `#`.
#[cfg(target_arch = "wasm32")]
fn q(s: &str) -> std::borrow::Cow<'_, str> {
    urlencoding::encode(s)
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_summary(path: &str) -> Result<RepoSummary, String> {
    fetch_json(&format!("/api/repo/summary?path={}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_log(path: &str, limit: usize) -> Result<Vec<CommitInfo>, String> {
    fetch_json(&format!("/api/repo/log?path={}&limit={limit}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_status(path: &str) -> Result<Vec<StatusEntry>, String> {
    fetch_json(&format!("/api/repo/status?path={}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_branches(path: &str) -> Result<Vec<BranchInfo>, String> {
    fetch_json(&format!("/api/repo/branches?path={}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_tags(path: &str) -> Result<Vec<TagInfo>, String> {
    fetch_json(&format!("/api/repo/tags?path={}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_remotes(path: &str) -> Result<Vec<RemoteBranchInfo>, String> {
    fetch_json(&format!("/api/repo/remotes?path={}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_tree(path: &str) -> Result<Vec<TreeEntry>, String> {
    fetch_json(&format!("/api/repo/tree?path={}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_blob(path: &str, oid: &str, file: &str) -> Result<BlobView, String> {
    fetch_json(&format!(
        "/api/repo/blob?path={}&oid={}&file={}",
        q(path),
        q(oid),
        q(file),
    ))
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_diff(path: &str, oid: &str) -> Result<CommitDiff, String> {
    fetch_json(&format!("/api/repo/diff?path={}&oid={}", q(path), q(oid),)).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_diff_working(path: &str, file: &str) -> Result<FileDiff, String> {
    fetch_json(&format!(
        "/api/repo/diff/working?path={}&file={}",
        q(path),
        q(file),
    ))
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_blame(path: &str, file: &str) -> Result<BlameView, String> {
    fetch_json(&format!(
        "/api/repo/blame?path={}&file={}",
        q(path),
        q(file),
    ))
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_staged(path: &str) -> Result<Vec<StatusEntry>, String> {
    fetch_json(&format!("/api/repo/staged?path={}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_stashes(path: &str) -> Result<Vec<StashEntry>, String> {
    fetch_json(&format!("/api/repo/stashes?path={}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_stash_save(path: &str, message: Option<&str>) -> Result<(), String> {
    let mut body = serde_json::json!({ "path": path });
    if let Some(m) = message.filter(|s| !s.trim().is_empty()) {
        body["message"] = serde_json::Value::String(m.to_string());
    }
    post_empty("/api/repo/stashes/save", body).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_stash_pop(path: &str, index: usize) -> Result<(), String> {
    post_empty(
        "/api/repo/stashes/pop",
        serde_json::json!({ "path": path, "index": index }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_stash_drop(path: &str, index: usize) -> Result<(), String> {
    post_empty(
        "/api/repo/stashes/drop",
        serde_json::json!({ "path": path, "index": index }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_stage(path: &str, files: &[String]) -> Result<(), String> {
    post_empty(
        "/api/repo/stage",
        serde_json::json!({ "path": path, "files": files }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_unstage(path: &str, files: &[String]) -> Result<(), String> {
    post_empty(
        "/api/repo/unstage",
        serde_json::json!({ "path": path, "files": files }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_discard(path: &str, files: &[String]) -> Result<(), String> {
    post_empty(
        "/api/repo/discard",
        serde_json::json!({ "path": path, "files": files }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_checkout(path: &str, target: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/checkout",
        serde_json::json!({ "path": path, "target": target }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_branch_create(path: &str, name: &str, switch: bool) -> Result<(), String> {
    post_empty(
        "/api/repo/branches/create",
        serde_json::json!({ "path": path, "name": name, "switch": switch }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_branch_delete(path: &str, name: &str, force: bool) -> Result<(), String> {
    post_empty(
        "/api/repo/branches/delete",
        serde_json::json!({ "path": path, "name": name, "force": force }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_branch_rename(path: &str, old: &str, new: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/branches/rename",
        serde_json::json!({ "path": path, "old": old, "new": new }),
    )
    .await
}

/// Pops the native folder picker on the server side (via `rfd`) and
/// returns the chosen path. `Ok(None)` means the user cancelled. Only
/// works against a server built with `--features desktop`; vanilla
/// `gitrust serve` returns 404 here.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_pick_folder() -> Result<Option<String>, String> {
    let v: serde_json::Value =
        post_with_response("/api/repo/pick-folder", serde_json::Value::Null).await?;
    Ok(v.get("path")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string()))
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_commit(
    path: &str,
    message: &str,
    author: Option<&str>,
) -> Result<String, String> {
    let mut body = serde_json::json!({ "path": path, "message": message });
    if let Some(a) = author.filter(|s| !s.trim().is_empty()) {
        body["author"] = serde_json::Value::String(a.to_string());
    }
    let v: serde_json::Value = post_with_response("/api/repo/commit", body).await?;
    v.get("oid")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "commit response missing `oid` field".to_string())
}

/// Read the active token without subscribing the current Dioxus scope —
/// fetch tasks run inside use_resource closures whose dependencies are
/// already the things they care about (current_repo, oid, …).
#[cfg(target_arch = "wasm32")]
fn current_token() -> String {
    use dioxus::prelude::ReadableExt;
    crate::state::AUTH_TOKEN.peek().clone().unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
async fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let token = current_token();
    let resp = gloo_net::http::Request::get(url)
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(extract_error(resp).await);
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
async fn post_empty(url: &str, body: serde_json::Value) -> Result<(), String> {
    let token = current_token();
    let resp = gloo_net::http::Request::post(url)
        .header("Authorization", &format!("Bearer {token}"))
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(extract_error(resp).await);
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn post_with_response<T: serde::de::DeserializeOwned>(
    url: &str,
    body: serde_json::Value,
) -> Result<T, String> {
    let token = current_token();
    let resp = gloo_net::http::Request::post(url)
        .header("Authorization", &format!("Bearer {token}"))
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(extract_error(resp).await);
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

/// Pull the `error` field out of the server's `{ "error": "..." }`
/// envelope. Falls back to `HTTP <status>` when the body is missing
/// or not JSON — both keep the message readable in the UI.
///
/// Side effect: on `401 Unauthorized` we wipe the stored token and
/// flip the GlobalSignal back to `None`. App's top-level gate
/// subscribes to that signal, so the in-progress error briefly
/// flashes and then AppContent unmounts and the sign-in screen
/// takes over. Every in-flight request gets cancelled when its
/// owning `use_resource` is dropped.
#[cfg(target_arch = "wasm32")]
async fn extract_error(resp: gloo_net::http::Response) -> String {
    let status = resp.status();
    if status == 401 {
        crate::state::clear_auth_token();
        *crate::state::AUTH_TOKEN.write() = None;
        return "session expired — sign in again".to_string();
    }
    match resp.json::<serde_json::Value>().await {
        Ok(v) => {
            let msg = v
                .get("error")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let hint = v.get("hint").and_then(|x| x.as_str()).unwrap_or("").trim();
            match (msg.is_empty(), hint.is_empty()) {
                (true, true) => format!("HTTP {status}"),
                (true, false) => hint.to_string(),
                (false, true) => msg,
                (false, false) => format!("{msg} · {hint}"),
            }
        }
        Err(_) => format!("HTTP {status}"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_summary(_path: &str) -> Result<RepoSummary, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_log(_path: &str, _limit: usize) -> Result<Vec<CommitInfo>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_status(_path: &str) -> Result<Vec<StatusEntry>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_branches(_path: &str) -> Result<Vec<BranchInfo>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_tags(_path: &str) -> Result<Vec<TagInfo>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_remotes(_path: &str) -> Result<Vec<RemoteBranchInfo>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_tree(_path: &str) -> Result<Vec<TreeEntry>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_blob(_path: &str, _oid: &str, _file: &str) -> Result<BlobView, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_diff(_path: &str, _oid: &str) -> Result<CommitDiff, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_diff_working(_path: &str, _file: &str) -> Result<FileDiff, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_blame(_path: &str, _file: &str) -> Result<BlameView, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_staged(_path: &str) -> Result<Vec<StatusEntry>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_stashes(_path: &str) -> Result<Vec<StashEntry>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_stash_save(_path: &str, _message: Option<&str>) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_stash_pop(_path: &str, _index: usize) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_stash_drop(_path: &str, _index: usize) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_stage(_path: &str, _files: &[String]) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_unstage(_path: &str, _files: &[String]) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_commit(
    _path: &str,
    _message: &str,
    _author: Option<&str>,
) -> Result<String, String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_discard(_path: &str, _files: &[String]) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_checkout(_path: &str, _target: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_branch_create(
    _path: &str,
    _name: &str,
    _switch: bool,
) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_branch_delete(
    _path: &str,
    _name: &str,
    _force: bool,
) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_branch_rename(_path: &str, _old: &str, _new: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_pick_folder() -> Result<Option<String>, String> {
    Err("native build: writes not implemented".into())
}
