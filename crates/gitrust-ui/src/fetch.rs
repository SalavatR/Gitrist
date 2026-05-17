//! Typed HTTP wrappers over the JSON API. Each function exists in two
//! flavours: a real `gloo_net`-backed call on `wasm32` and a stub that
//! errors out on native (the UI lives in the browser; the native build
//! only needs the crate to compile for `cargo check` / docs).

use gitrust_types::{
    BlameView, BlobView, BranchInfo, CommitDiff, CommitInfo, FileDiff, NetworkOpResult,
    RemoteBranchInfo, RepoEntry, RepoState, RepoSummary, StashEntry, StatusEntry, TagInfo,
    TreeEntry,
};

// `q` percent-encodes a single query-string value. Paths can contain
// spaces, `#`, `&`, `?` and non-ASCII — letting them through raw would
// break URL parsing or get truncated at `#`.
#[cfg(target_arch = "wasm32")]
fn q(s: &str) -> std::borrow::Cow<'_, str> {
    urlencoding::encode(s)
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_repos() -> Result<Vec<RepoEntry>, String> {
    fetch_json("/api/repos").await
}

// `fetch_log_file` powers the file-history view; `fetch_diff_refs`
// still has no UI surface yet (queued as the next polish pass) so it
// keeps its dead-code allow.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_log_file(
    path: &str,
    file: &str,
    limit: usize,
) -> Result<Vec<CommitInfo>, String> {
    fetch_json(&format!(
        "/api/repo/log-file?path={}&file={}&limit={limit}",
        q(path),
        q(file),
    ))
    .await
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)] // ref-diff UI is the next polish step.
pub(crate) async fn fetch_diff_refs(
    path: &str,
    from: &str,
    to: &str,
) -> Result<Vec<FileDiff>, String> {
    fetch_json(&format!(
        "/api/repo/diff/refs?path={}&from={}&to={}",
        q(path),
        q(from),
        q(to),
    ))
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_tag_create(
    path: &str,
    name: &str,
    target: Option<&str>,
    message: Option<&str>,
) -> Result<(), String> {
    let mut body = serde_json::json!({ "path": path, "name": name });
    if let Some(t) = target.filter(|s| !s.trim().is_empty()) {
        body["target"] = serde_json::Value::String(t.to_string());
    }
    if let Some(m) = message.filter(|s| !s.trim().is_empty()) {
        body["message"] = serde_json::Value::String(m.to_string());
    }
    post_empty("/api/repo/tags/create", body).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_tag_delete(path: &str, name: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/tags/delete",
        serde_json::json!({ "path": path, "name": name }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_summary(path: &str) -> Result<RepoSummary, String> {
    fetch_json(&format!("/api/repo/summary?path={}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_log(
    path: &str,
    limit: usize,
    query: &str,
    all: bool,
) -> Result<Vec<CommitInfo>, String> {
    let mut url = format!("/api/repo/log?path={}&limit={limit}", q(path));
    if all {
        url.push_str("&all=true");
    }
    if !query.trim().is_empty() {
        url.push_str(&format!("&q={}", q(query.trim())));
    }
    fetch_json(&url).await
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

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_fetch(
    path: &str,
    remote: Option<&str>,
) -> Result<NetworkOpResult, String> {
    let mut body = serde_json::json!({ "path": path });
    if let Some(r) = remote.filter(|s| !s.trim().is_empty()) {
        body["remote"] = serde_json::Value::String(r.to_string());
    }
    post_with_response("/api/repo/fetch", body).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_pull(
    path: &str,
    remote: Option<&str>,
    ff_only: bool,
) -> Result<NetworkOpResult, String> {
    let mut body = serde_json::json!({ "path": path, "ff_only": ff_only });
    if let Some(r) = remote.filter(|s| !s.trim().is_empty()) {
        body["remote"] = serde_json::Value::String(r.to_string());
    }
    post_with_response("/api/repo/pull", body).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_push(
    path: &str,
    remote: Option<&str>,
    refspec: Option<&str>,
    force_with_lease: bool,
    set_upstream: bool,
) -> Result<NetworkOpResult, String> {
    let mut body = serde_json::json!({
        "path": path,
        "force_with_lease": force_with_lease,
        "set_upstream": set_upstream,
    });
    if let Some(r) = remote.filter(|s| !s.trim().is_empty()) {
        body["remote"] = serde_json::Value::String(r.to_string());
    }
    if let Some(rs) = refspec.filter(|s| !s.trim().is_empty()) {
        body["refspec"] = serde_json::Value::String(rs.to_string());
    }
    post_with_response("/api/repo/push", body).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_merge(
    path: &str,
    target: &str,
    no_ff: bool,
) -> Result<NetworkOpResult, String> {
    post_with_response(
        "/api/repo/merge",
        serde_json::json!({ "path": path, "target": target, "no_ff": no_ff }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_cherry_pick(path: &str, oid: &str) -> Result<NetworkOpResult, String> {
    post_with_response(
        "/api/repo/cherry-pick",
        serde_json::json!({ "path": path, "oid": oid }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_state(path: &str) -> Result<RepoState, String> {
    fetch_json(&format!("/api/repo/state?path={}", q(path))).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_merge_abort(path: &str) -> Result<(), String> {
    post_empty("/api/repo/merge/abort", serde_json::json!({ "path": path })).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_merge_continue(path: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/merge/continue",
        serde_json::json!({ "path": path }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_cherry_pick_abort(path: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/cherry-pick/abort",
        serde_json::json!({ "path": path }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_cherry_pick_continue(path: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/cherry-pick/continue",
        serde_json::json!({ "path": path }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_resolve(path: &str, file: &str, side: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/resolve",
        serde_json::json!({ "path": path, "file": file, "side": side }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_stage_hunks(
    path: &str,
    file: &str,
    hunks: &[usize],
) -> Result<(), String> {
    post_empty(
        "/api/repo/stage-hunks",
        serde_json::json!({ "path": path, "file": file, "hunks": hunks }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_rebase(path: &str, upstream: &str) -> Result<NetworkOpResult, String> {
    post_with_response(
        "/api/repo/rebase",
        serde_json::json!({ "path": path, "upstream": upstream }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_rebase_abort(path: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/rebase/abort",
        serde_json::json!({ "path": path }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_rebase_continue(path: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/rebase/continue",
        serde_json::json!({ "path": path }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_rebase_skip(path: &str) -> Result<(), String> {
    post_empty("/api/repo/rebase/skip", serde_json::json!({ "path": path })).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_revert(path: &str, oid: &str) -> Result<NetworkOpResult, String> {
    post_with_response(
        "/api/repo/revert",
        serde_json::json!({ "path": path, "oid": oid }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_revert_abort(path: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/revert/abort",
        serde_json::json!({ "path": path }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_revert_continue(path: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/revert/continue",
        serde_json::json!({ "path": path }),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_revert_skip(path: &str) -> Result<(), String> {
    post_empty("/api/repo/revert/skip", serde_json::json!({ "path": path })).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn post_reset(path: &str, target: &str, mode: &str) -> Result<(), String> {
    post_empty(
        "/api/repo/reset",
        serde_json::json!({ "path": path, "target": target, "mode": mode }),
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
    // Retry once on a transient `Failed to fetch` — that surfaces when
    // the browser throttles a backgrounded tab and the WS / fetch
    // connection is half-dead until focus comes back. A 200 ms pause
    // is enough for the browser to re-warm the connection.
    let mut last_err: Option<String> = None;
    for attempt in 0..2 {
        let send = gloo_net::http::Request::get(url)
            .header("Authorization", &format!("Bearer {token}"))
            .send()
            .await;
        match send {
            Ok(resp) => {
                if !resp.ok() {
                    return Err(extract_error(resp).await);
                }
                return resp.json::<T>().await.map_err(|e| e.to_string());
            }
            Err(e) => {
                let msg = e.to_string();
                if attempt == 0 && msg.contains("Failed to fetch") {
                    gloo_timers::future::TimeoutFuture::new(200).await;
                    continue;
                }
                last_err = Some(msg);
                break;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| "fetch failed".into()))
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
pub(crate) async fn fetch_repos() -> Result<Vec<RepoEntry>, String> {
    Err("native build: fetching not implemented".into())
}

// `fetch_log_file` IS called from lib.rs (the file-history view) on
// every target, so it needs a native stub. `fetch_diff_refs` is still
// unused on native — wrap its stub in `#[allow(dead_code)]` to silence
// the lint until the ref-diff UI is wired.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_log_file(
    _path: &str,
    _file: &str,
    _limit: usize,
) -> Result<Vec<CommitInfo>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub(crate) async fn fetch_diff_refs(
    _path: &str,
    _from: &str,
    _to: &str,
) -> Result<Vec<FileDiff>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_tag_create(
    _path: &str,
    _name: &str,
    _target: Option<&str>,
    _message: Option<&str>,
) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_tag_delete(_path: &str, _name: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_summary(_path: &str) -> Result<RepoSummary, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_log(
    _path: &str,
    _limit: usize,
    _query: &str,
    _all: bool,
) -> Result<Vec<CommitInfo>, String> {
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

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_fetch(
    _path: &str,
    _remote: Option<&str>,
) -> Result<NetworkOpResult, String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_pull(
    _path: &str,
    _remote: Option<&str>,
    _ff_only: bool,
) -> Result<NetworkOpResult, String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_push(
    _path: &str,
    _remote: Option<&str>,
    _refspec: Option<&str>,
    _force_with_lease: bool,
    _set_upstream: bool,
) -> Result<NetworkOpResult, String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_merge(
    _path: &str,
    _target: &str,
    _no_ff: bool,
) -> Result<NetworkOpResult, String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_cherry_pick(_path: &str, _oid: &str) -> Result<NetworkOpResult, String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn fetch_state(_path: &str) -> Result<RepoState, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_merge_abort(_path: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_merge_continue(_path: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_cherry_pick_abort(_path: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_cherry_pick_continue(_path: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_resolve(_path: &str, _file: &str, _side: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_stage_hunks(
    _path: &str,
    _file: &str,
    _hunks: &[usize],
) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_rebase(_path: &str, _upstream: &str) -> Result<NetworkOpResult, String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_rebase_abort(_path: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_rebase_continue(_path: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_rebase_skip(_path: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_revert(_path: &str, _oid: &str) -> Result<NetworkOpResult, String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_revert_abort(_path: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_revert_continue(_path: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_revert_skip(_path: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn post_reset(_path: &str, _target: &str, _mode: &str) -> Result<(), String> {
    Err("native build: writes not implemented".into())
}
