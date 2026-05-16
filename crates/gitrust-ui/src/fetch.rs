//! Typed HTTP wrappers over the JSON API. Each function exists in two
//! flavours: a real `gloo_net`-backed call on `wasm32` and a stub that
//! errors out on native (the UI lives in the browser; the native build
//! only needs the crate to compile for `cargo check` / docs).

use gitrust_types::{
    BlobView, BranchInfo, CommitDiff, CommitInfo, FileDiff, RemoteBranchInfo, RepoSummary,
    StatusEntry, TagInfo, TreeEntry,
};

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_summary(path: &str) -> Result<RepoSummary, String> {
    fetch_json(&format!("/api/repo/summary?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_log(path: &str, limit: usize) -> Result<Vec<CommitInfo>, String> {
    fetch_json(&format!("/api/repo/log?path={path}&limit={limit}")).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_status(path: &str) -> Result<Vec<StatusEntry>, String> {
    fetch_json(&format!("/api/repo/status?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_branches(path: &str) -> Result<Vec<BranchInfo>, String> {
    fetch_json(&format!("/api/repo/branches?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_tags(path: &str) -> Result<Vec<TagInfo>, String> {
    fetch_json(&format!("/api/repo/tags?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_remotes(path: &str) -> Result<Vec<RemoteBranchInfo>, String> {
    fetch_json(&format!("/api/repo/remotes?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_tree(path: &str) -> Result<Vec<TreeEntry>, String> {
    fetch_json(&format!("/api/repo/tree?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_blob(path: &str, oid: &str, file: &str) -> Result<BlobView, String> {
    fetch_json(&format!("/api/repo/blob?path={path}&oid={oid}&file={file}")).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_diff(path: &str, oid: &str) -> Result<CommitDiff, String> {
    fetch_json(&format!("/api/repo/diff?path={path}&oid={oid}")).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_diff_working(path: &str, file: &str) -> Result<FileDiff, String> {
    fetch_json(&format!("/api/repo/diff/working?path={path}&file={file}")).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let resp = gloo_net::http::Request::get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
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
