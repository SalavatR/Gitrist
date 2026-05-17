use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use gitrust_core::{
    BlameView, BlobView, BranchInfo, CommitDiff, CommitInfo, FileDiff, NetworkOpResult,
    RemoteBranchInfo, RepoState, RepoSummary, StashEntry, StatusEntry, TagInfo, TreeEntry,
    blame_file, checkout as core_checkout, cherry_pick as core_cherry_pick,
    cherry_pick_abort as core_cherry_pick_abort, cherry_pick_continue as core_cherry_pick_continue,
    commit as core_commit, commit_info, create_branch as core_create_branch,
    delete_branch as core_delete_branch, diff_commit, diff_working, discard_files,
    fetch as core_fetch, list_branches, list_remote_branches, list_staged, list_status, list_tags,
    list_tree, log_commits, merge as core_merge, merge_abort as core_merge_abort,
    merge_continue as core_merge_continue, pull as core_pull, push as core_push,
    rebase as core_rebase, rebase_abort as core_rebase_abort,
    rebase_continue as core_rebase_continue, rebase_skip as core_rebase_skip,
    rename_branch as core_rename_branch, repo_state as core_repo_state, reset as core_reset,
    resolve_file as core_resolve_file, revert as core_revert, revert_abort as core_revert_abort,
    revert_continue as core_revert_continue, revert_skip as core_revert_skip, show_blob,
    stage_files as core_stage_files, stage_hunks as core_stage_hunks,
    stash_drop as core_stash_drop, stash_list as core_stash_list, stash_pop as core_stash_pop,
    stash_save as core_stash_save, summarize_repo, unstage_files as core_unstage,
};

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
}

#[derive(Deserialize)]
struct LogQuery {
    path: String,
    #[serde(default = "default_limit")]
    limit: usize,
    /// Optional substring filter on commit message / author / oid prefix.
    /// Case-insensitive. When absent or empty the log is unfiltered.
    #[serde(default)]
    q: Option<String>,
    /// `true` walks every ref tip (`git log --all`); `false` walks only
    /// HEAD's ancestors. Default `false` keeps backward compatibility.
    #[serde(default)]
    all: bool,
}

fn default_limit() -> usize {
    50
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn repo_summary(Query(q): Query<PathQuery>) -> Result<Json<RepoSummary>, ApiError> {
    let path = PathBuf::from(q.path);
    summarize_repo(&path).map(Json).map_err(ApiError::from)
}

async fn repo_log(Query(q): Query<LogQuery>) -> Result<Json<Vec<CommitInfo>>, ApiError> {
    let path = PathBuf::from(q.path);
    log_commits(&path, q.limit.min(500), q.q.as_deref(), q.all)
        .map(Json)
        .map_err(ApiError::from)
}

async fn repo_status(Query(q): Query<PathQuery>) -> Result<Json<Vec<StatusEntry>>, ApiError> {
    let path = PathBuf::from(q.path);
    list_status(&path).map(Json).map_err(ApiError::from)
}

async fn repo_staged(Query(q): Query<PathQuery>) -> Result<Json<Vec<StatusEntry>>, ApiError> {
    let path = PathBuf::from(q.path);
    list_staged(&path).map(Json).map_err(ApiError::from)
}

async fn repo_branches(Query(q): Query<PathQuery>) -> Result<Json<Vec<BranchInfo>>, ApiError> {
    let path = PathBuf::from(q.path);
    list_branches(&path).map(Json).map_err(ApiError::from)
}

async fn repo_tags(Query(q): Query<PathQuery>) -> Result<Json<Vec<TagInfo>>, ApiError> {
    let path = PathBuf::from(q.path);
    list_tags(&path).map(Json).map_err(ApiError::from)
}

async fn repo_remotes(Query(q): Query<PathQuery>) -> Result<Json<Vec<RemoteBranchInfo>>, ApiError> {
    let path = PathBuf::from(q.path);
    list_remote_branches(&path)
        .map(Json)
        .map_err(ApiError::from)
}

async fn repo_tree(Query(q): Query<PathQuery>) -> Result<Json<Vec<TreeEntry>>, ApiError> {
    let path = PathBuf::from(q.path);
    list_tree(&path).map(Json).map_err(ApiError::from)
}

#[derive(Deserialize)]
struct BlobQuery {
    path: String,
    oid: String,
    file: String,
}

async fn repo_blob(Query(q): Query<BlobQuery>) -> Result<Json<BlobView>, ApiError> {
    let path = PathBuf::from(q.path);
    show_blob(&path, &q.oid, &q.file)
        .map(Json)
        .map_err(ApiError::from)
}

#[derive(Deserialize)]
struct DiffQuery {
    path: String,
    oid: String,
}

async fn repo_diff(Query(q): Query<DiffQuery>) -> Result<Json<CommitDiff>, ApiError> {
    let path = PathBuf::from(q.path);
    diff_commit(&path, &q.oid).map(Json).map_err(ApiError::from)
}

#[derive(Deserialize)]
struct DiffWorkingQuery {
    path: String,
    file: String,
}

async fn repo_diff_working(Query(q): Query<DiffWorkingQuery>) -> Result<Json<FileDiff>, ApiError> {
    let path = PathBuf::from(q.path);
    diff_working(&path, &q.file)
        .map(Json)
        .map_err(ApiError::from)
}

#[derive(Deserialize)]
struct BlameQuery {
    path: String,
    file: String,
}

async fn repo_blame(Query(q): Query<BlameQuery>) -> Result<Json<BlameView>, ApiError> {
    let path = PathBuf::from(q.path);
    blame_file(&path, &q.file).map(Json).map_err(ApiError::from)
}

#[derive(Deserialize)]
struct StageBody {
    path: String,
    files: Vec<String>,
}

async fn repo_stage(Json(body): Json<StageBody>) -> Result<StatusCode, ApiError> {
    let StageBody { path, files } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_stage_files(&repo, &files))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_unstage(Json(body): Json<StageBody>) -> Result<StatusCode, ApiError> {
    let StageBody { path, files } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_unstage(&repo, &files))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CommitBody {
    path: String,
    message: String,
    #[serde(default)]
    author: Option<String>,
}

async fn repo_commit(Json(body): Json<CommitBody>) -> Result<Json<serde_json::Value>, ApiError> {
    let CommitBody {
        path,
        message,
        author,
    } = body;
    let repo = PathBuf::from(path);
    let oid = tokio::task::spawn_blocking(move || core_commit(&repo, &message, author.as_deref()))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "oid": oid })))
}

async fn repo_commit_get(Query(q): Query<DiffQuery>) -> Result<Json<CommitInfo>, ApiError> {
    let path = PathBuf::from(q.path);
    commit_info(&path, &q.oid).map(Json).map_err(ApiError::from)
}

#[derive(Deserialize)]
struct CreateBranchBody {
    path: String,
    name: String,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    switch: bool,
}

async fn repo_branch_create(Json(body): Json<CreateBranchBody>) -> Result<StatusCode, ApiError> {
    let CreateBranchBody {
        path,
        name,
        from,
        switch,
    } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_create_branch(&repo, &name, from.as_deref(), switch))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct DeleteBranchBody {
    path: String,
    name: String,
    #[serde(default)]
    force: bool,
}

async fn repo_branch_delete(Json(body): Json<DeleteBranchBody>) -> Result<StatusCode, ApiError> {
    let DeleteBranchBody { path, name, force } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_delete_branch(&repo, &name, force))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct RenameBranchBody {
    path: String,
    old: String,
    new: String,
}

async fn repo_branch_rename(Json(body): Json<RenameBranchBody>) -> Result<StatusCode, ApiError> {
    let RenameBranchBody { path, old, new } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_rename_branch(&repo, &old, &new))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CheckoutBody {
    path: String,
    target: String,
}

async fn repo_checkout(Json(body): Json<CheckoutBody>) -> Result<StatusCode, ApiError> {
    let CheckoutBody { path, target } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_checkout(&repo, &target))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_discard(Json(body): Json<StageBody>) -> Result<StatusCode, ApiError> {
    let StageBody { path, files } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || discard_files(&repo, &files))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_stash_list(Query(q): Query<PathQuery>) -> Result<Json<Vec<StashEntry>>, ApiError> {
    let path = PathBuf::from(q.path);
    core_stash_list(&path).map(Json).map_err(ApiError::from)
}

#[derive(Deserialize)]
struct StashSaveBody {
    path: String,
    #[serde(default)]
    message: Option<String>,
}

async fn repo_stash_save(Json(body): Json<StashSaveBody>) -> Result<StatusCode, ApiError> {
    let StashSaveBody { path, message } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_stash_save(&repo, message.as_deref()))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct StashIndexBody {
    path: String,
    index: usize,
}

async fn repo_stash_pop(Json(body): Json<StashIndexBody>) -> Result<StatusCode, ApiError> {
    let StashIndexBody { path, index } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_stash_pop(&repo, index))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_stash_drop(Json(body): Json<StashIndexBody>) -> Result<StatusCode, ApiError> {
    let StashIndexBody { path, index } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_stash_drop(&repo, index))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct FetchBody {
    path: String,
    #[serde(default)]
    remote: Option<String>,
}

async fn repo_fetch(Json(body): Json<FetchBody>) -> Result<Json<NetworkOpResult>, ApiError> {
    let FetchBody { path, remote } = body;
    let repo = PathBuf::from(path);
    let result = tokio::task::spawn_blocking(move || core_fetch(&repo, remote.as_deref()))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Deserialize)]
struct PullBody {
    path: String,
    #[serde(default)]
    remote: Option<String>,
    /// Default `true` — refuse non-fast-forward integrations. Pass
    /// `false` to let git's configured `pull.rebase`/`pull.ff` decide.
    #[serde(default = "default_true")]
    ff_only: bool,
}

fn default_true() -> bool {
    true
}

async fn repo_pull(Json(body): Json<PullBody>) -> Result<Json<NetworkOpResult>, ApiError> {
    let PullBody {
        path,
        remote,
        ff_only,
    } = body;
    let repo = PathBuf::from(path);
    let result = tokio::task::spawn_blocking(move || core_pull(&repo, remote.as_deref(), ff_only))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Deserialize)]
struct PushBody {
    path: String,
    #[serde(default)]
    remote: Option<String>,
    #[serde(default)]
    refspec: Option<String>,
    #[serde(default)]
    force_with_lease: bool,
    #[serde(default)]
    set_upstream: bool,
}

async fn repo_state(Query(q): Query<PathQuery>) -> Result<Json<RepoState>, ApiError> {
    let path = PathBuf::from(q.path);
    core_repo_state(&path).map(Json).map_err(ApiError::from)
}

async fn repo_merge_abort(Json(body): Json<PathQuery>) -> Result<StatusCode, ApiError> {
    let repo = PathBuf::from(body.path);
    tokio::task::spawn_blocking(move || core_merge_abort(&repo))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_merge_continue(Json(body): Json<PathQuery>) -> Result<StatusCode, ApiError> {
    let repo = PathBuf::from(body.path);
    tokio::task::spawn_blocking(move || core_merge_continue(&repo))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_cherry_pick_abort(Json(body): Json<PathQuery>) -> Result<StatusCode, ApiError> {
    let repo = PathBuf::from(body.path);
    tokio::task::spawn_blocking(move || core_cherry_pick_abort(&repo))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_cherry_pick_continue(Json(body): Json<PathQuery>) -> Result<StatusCode, ApiError> {
    let repo = PathBuf::from(body.path);
    tokio::task::spawn_blocking(move || core_cherry_pick_continue(&repo))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ResolveBody {
    path: String,
    file: String,
    /// `"ours"` | `"theirs"` — which side of the merge to keep.
    side: String,
}

async fn repo_resolve(Json(body): Json<ResolveBody>) -> Result<StatusCode, ApiError> {
    let ResolveBody { path, file, side } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_resolve_file(&repo, &file, &side))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct StageHunksBody {
    path: String,
    file: String,
    hunks: Vec<usize>,
}

async fn repo_stage_hunks(Json(body): Json<StageHunksBody>) -> Result<StatusCode, ApiError> {
    let StageHunksBody { path, file, hunks } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_stage_hunks(&repo, &file, &hunks))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct RebaseBody {
    path: String,
    upstream: String,
}

async fn repo_rebase(Json(body): Json<RebaseBody>) -> Result<Json<NetworkOpResult>, ApiError> {
    let RebaseBody { path, upstream } = body;
    let repo = PathBuf::from(path);
    let result = tokio::task::spawn_blocking(move || core_rebase(&repo, &upstream))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

async fn repo_rebase_abort(Json(body): Json<PathQuery>) -> Result<StatusCode, ApiError> {
    let repo = PathBuf::from(body.path);
    tokio::task::spawn_blocking(move || core_rebase_abort(&repo))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_rebase_continue(Json(body): Json<PathQuery>) -> Result<StatusCode, ApiError> {
    let repo = PathBuf::from(body.path);
    tokio::task::spawn_blocking(move || core_rebase_continue(&repo))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_rebase_skip(Json(body): Json<PathQuery>) -> Result<StatusCode, ApiError> {
    let repo = PathBuf::from(body.path);
    tokio::task::spawn_blocking(move || core_rebase_skip(&repo))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct RevertBody {
    path: String,
    oid: String,
}

async fn repo_revert(Json(body): Json<RevertBody>) -> Result<Json<NetworkOpResult>, ApiError> {
    let RevertBody { path, oid } = body;
    let repo = PathBuf::from(path);
    let result = tokio::task::spawn_blocking(move || core_revert(&repo, &oid))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

async fn repo_revert_abort(Json(body): Json<PathQuery>) -> Result<StatusCode, ApiError> {
    let repo = PathBuf::from(body.path);
    tokio::task::spawn_blocking(move || core_revert_abort(&repo))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_revert_continue(Json(body): Json<PathQuery>) -> Result<StatusCode, ApiError> {
    let repo = PathBuf::from(body.path);
    tokio::task::spawn_blocking(move || core_revert_continue(&repo))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repo_revert_skip(Json(body): Json<PathQuery>) -> Result<StatusCode, ApiError> {
    let repo = PathBuf::from(body.path);
    tokio::task::spawn_blocking(move || core_revert_skip(&repo))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ResetBody {
    path: String,
    target: String,
    /// `"soft"` | `"mixed"` | `"hard"`. Defaults to `"mixed"` (git's default).
    #[serde(default = "default_reset_mode")]
    mode: String,
}

fn default_reset_mode() -> String {
    "mixed".into()
}

async fn repo_reset(Json(body): Json<ResetBody>) -> Result<StatusCode, ApiError> {
    let ResetBody { path, target, mode } = body;
    let repo = PathBuf::from(path);
    tokio::task::spawn_blocking(move || core_reset(&repo, &target, &mode))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct MergeBody {
    path: String,
    /// Branch name, tag, or commit oid to merge into the current branch.
    target: String,
    #[serde(default)]
    no_ff: bool,
}

async fn repo_merge(Json(body): Json<MergeBody>) -> Result<Json<NetworkOpResult>, ApiError> {
    let MergeBody {
        path,
        target,
        no_ff,
    } = body;
    let repo = PathBuf::from(path);
    let result = tokio::task::spawn_blocking(move || core_merge(&repo, &target, no_ff))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Deserialize)]
struct CherryPickBody {
    path: String,
    oid: String,
}

async fn repo_cherry_pick(
    Json(body): Json<CherryPickBody>,
) -> Result<Json<NetworkOpResult>, ApiError> {
    let CherryPickBody { path, oid } = body;
    let repo = PathBuf::from(path);
    let result = tokio::task::spawn_blocking(move || core_cherry_pick(&repo, &oid))
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

async fn repo_push(Json(body): Json<PushBody>) -> Result<Json<NetworkOpResult>, ApiError> {
    let PushBody {
        path,
        remote,
        refspec,
        force_with_lease,
        set_upstream,
    } = body;
    let repo = PathBuf::from(path);
    let result = tokio::task::spawn_blocking(move || {
        core_push(
            &repo,
            remote.as_deref(),
            refspec.as_deref(),
            force_with_lease,
            set_upstream,
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("join error: {e}"))?
    .map_err(ApiError::from)?;
    Ok(Json(result))
}

/// Open the OS-native folder picker (NSOpenPanel on macOS, the GTK
/// portal/file-chooser on Linux, IFileDialog on Windows) and return
/// the chosen path. This is the official way around macOS TCC — when
/// the user picks a folder through `rfd`, macOS grants the app
/// access to that folder for the rest of the session without any
/// entitlement plumbing.
///
/// Only built when the `desktop` feature is enabled — the headless
/// `gitrust serve` flavour has no UI to put behind the dialog.
#[cfg(feature = "desktop")]
async fn pick_folder() -> Result<Json<serde_json::Value>, ApiError> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Open a git repository")
        .pick_folder()
        .await;
    let path = handle.map(|h| h.path().display().to_string());
    Ok(Json(serde_json::json!({ "path": path })))
}

/// Live filesystem-event stream for a single repo. Client opens a WebSocket
/// and gets debounced, deduplicated event kinds (`head_changed`,
/// `refs_changed`, `index_changed`, `worktree_changed`) as JSON text frames.
/// Noisy paths (`.git/objects`, `.git/lfs`, `*.lock`) are filtered out.
async fn repo_events(
    Query(q): Query<PathQuery>,
    ws: axum::extract::WebSocketUpgrade,
) -> axum::response::Response {
    let repo = PathBuf::from(q.path);
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = run_event_stream(socket, repo).await {
            tracing::warn!("event stream ended: {e:#}");
        }
    })
}

async fn run_event_stream(
    socket: axum::extract::ws::WebSocket,
    repo: PathBuf,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message;
    use futures::SinkExt;

    // notify normalizes event paths; do the same to `repo` so `strip_prefix` succeeds.
    let repo = std::fs::canonicalize(&repo).unwrap_or(repo);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Event>();
    let mut watcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
            Ok(event) => {
                let _ = tx.send(event);
            }
            Err(e) => {
                tracing::warn!("events: notify error: {e}");
            }
        })?;
    add_watches(&mut watcher, &repo)?;

    let (mut sink, mut stream) = socket.split();
    let mut pending: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
    let mut flush_due: Option<tokio::time::Instant> = None;

    loop {
        let timer = async {
            match flush_due {
                Some(t) => tokio::time::sleep_until(t).await,
                None => std::future::pending::<()>().await,
            }
        };

        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };
                let kinds = categorize(&event, &repo);
                if !kinds.is_empty() {
                    for kind in kinds {
                        pending.insert(kind);
                    }
                    flush_due.get_or_insert_with(|| {
                        tokio::time::Instant::now() + std::time::Duration::from_millis(150)
                    });
                }
            }
            _ = timer => {
                for kind in pending.drain() {
                    let msg = serde_json::json!({ "kind": kind }).to_string();
                    if sink.send(Message::Text(msg.into())).await.is_err() {
                        return Ok(());
                    }
                }
                flush_due = None;
            }
            client = stream.next() => {
                // Client closed or sent something unexpected — exit cleanly.
                if matches!(client, None | Some(Err(_)) | Some(Ok(Message::Close(_)))) {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Walk `root` and register a `NonRecursive` watch per directory, skipping
/// build/output dirs that would blow past `max_user_watches` (target/,
/// node_modules/) and `.git` subtrees that are pure noise (objects/, lfs/).
/// `RecursiveMode::Recursive` would do the same walk under the hood but
/// without the skip list — on a repo with a populated `target/` that walks
/// thousands of dirs and stalls badly inside Termux/PRoot.
///
/// New dirs created after startup won't be watched until reconnect — fine
/// for v0.1, since the parent dir's watch still fires Create events.
fn add_watches(
    watcher: &mut notify::RecommendedWatcher,
    root: &std::path::Path,
) -> notify::Result<()> {
    use notify::{RecursiveMode, Watcher};
    let skip_names: &[&str] = &["target", "node_modules", ".direnv", ".venv"];
    let skip_rel: &[&str] = &[".git/objects", ".git/lfs"];

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        watcher.watch(&dir, RecursiveMode::NonRecursive)?;
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            // FileType::is_dir is false for symlinks, so this also skips link loops.
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            let path = entry.path();
            let Some(name) = path.file_name() else {
                continue;
            };
            let name_s = name.to_string_lossy();
            if skip_names.iter().any(|s| *s == name_s.as_ref()) {
                continue;
            }
            if let Ok(rel) = path.strip_prefix(root) {
                let rel_s = rel.to_string_lossy().replace('\\', "/");
                if skip_rel.iter().any(|p| rel_s.starts_with(p)) {
                    continue;
                }
            }
            stack.push(path);
        }
    }
    Ok(())
}

/// Map a raw fs event to one or more high-level categories. A single event
/// can touch multiple paths (e.g. renames), so the result is a deduplicated
/// list. Returns empty for noisy paths the UI doesn't care about (object
/// database churn, transient `*.lock`, LFS staging).
fn categorize(event: &notify::Event, repo: &std::path::Path) -> Vec<&'static str> {
    let mut kinds: Vec<&'static str> = Vec::new();
    let mut push = |kind: &'static str| {
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    };

    for path in &event.paths {
        let Ok(rel) = path.strip_prefix(repo) else {
            continue;
        };
        let s = rel.to_string_lossy().replace('\\', "/");

        if s.starts_with(".git/objects/") || s.ends_with(".lock") || s.starts_with(".git/lfs/") {
            continue;
        }

        if s == ".git/HEAD" {
            push("head_changed");
        } else if s.starts_with(".git/refs/") || s == ".git/packed-refs" {
            push("refs_changed");
        } else if s == ".git/index" {
            push("index_changed");
        } else if !s.starts_with(".git/") && !s.is_empty() {
            push("worktree_changed");
        }
    }
    kinds
}

/// Wire-shape error: `{ "error": "...", "code": "...", "hint"?: "..." }`.
/// `code` is a short, stable string the UI can match on without having
/// to parse the human-readable message; `hint` is an optional bit of
/// extra context for cases where we can guess what the user did wrong
/// (wrong path, unmerged branch, etc.). Both are derived from the
/// underlying anyhow message in [`classify`].
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    hint: Option<&'static str>,
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        let message = e.to_string();
        let (status, code, hint) = classify(&message);
        Self {
            status,
            code,
            message,
            hint,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let mut body = serde_json::json!({
            "error": self.message,
            "code": self.code,
        });
        if let Some(hint) = self.hint {
            body["hint"] = serde_json::Value::String(hint.into());
        }
        (self.status, Json(body)).into_response()
    }
}

/// Map a raw `anyhow` message to a categorised (status, code, hint).
/// The message strings here are best-effort heuristics over git / gix
/// error wording — if a category drifts, fall back to the catch-all
/// at the bottom which surfaces the raw message as-is.
fn classify(message: &str) -> (StatusCode, &'static str, Option<&'static str>) {
    let lower = message.to_lowercase();
    if lower.contains("does not appear to be a git repository")
        || lower.contains("not a git repository")
    {
        return (
            StatusCode::NOT_FOUND,
            "repo_not_found",
            Some("Check that the path points at a working tree or `.git` directory."),
        );
    }
    if lower.contains("not fully merged") {
        return (
            StatusCode::CONFLICT,
            "branch_unmerged",
            Some("Pass `force: true` to delete anyway, or merge the branch first."),
        );
    }
    if lower.contains("would be overwritten by checkout") || lower.contains("would be overwritten")
    {
        return (
            StatusCode::CONFLICT,
            "worktree_dirty",
            Some("Stash, commit, or discard the working-tree changes first."),
        );
    }
    if lower.contains("permission denied") {
        return (
            StatusCode::FORBIDDEN,
            "permission_denied",
            Some("Check filesystem permissions on the repo path."),
        );
    }
    if lower.contains("invalid oid") || lower.contains("not a valid object name") {
        return (
            StatusCode::BAD_REQUEST,
            "bad_oid",
            Some("Expected a full hex SHA — short oids aren't resolved here."),
        );
    }
    if lower.contains("already exists") {
        return (StatusCode::CONFLICT, "already_exists", None);
    }
    // Default — preserve the original message, no extra hint.
    (StatusCode::BAD_REQUEST, "generic", None)
}

/// Bearer token used to gate write endpoints. Generated on first
/// launch under `$XDG_CONFIG_HOME/gitrust/token` (or
/// `~/.config/gitrust/token`); on subsequent launches the same token
/// is reused. Reads stay open — the server is `localhost`-only by
/// default and there's no value in gating `summary` or `log` behind
/// auth in a single-user GUI client.
#[derive(Clone)]
pub struct AuthState {
    token: String,
}

impl AuthState {
    /// Construct directly from a token — useful for tests that want a
    /// known value without touching the filesystem.
    pub fn new(token: String) -> Self {
        Self { token }
    }

    /// Load (or create) the token at the default path.
    pub fn from_default_path() -> anyhow::Result<Self> {
        Ok(Self {
            token: load_or_create_token()?,
        })
    }
}

fn token_path() -> anyhow::Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".config");
                p
            })
        })
        .ok_or_else(|| anyhow::anyhow!("neither XDG_CONFIG_HOME nor HOME is set"))?;
    Ok(base.join("gitrust").join("token"))
}

fn load_or_create_token() -> anyhow::Result<String> {
    let path = token_path()?;
    if let Ok(s) = std::fs::read_to_string(&path) {
        let t = s.trim().to_string();
        if t.len() >= 32 {
            return Ok(t);
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let token = generate_token();
    std::fs::write(&path, &token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    tracing::info!("wrote new auth token to {}", path.display());
    Ok(token)
}

fn generate_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf).expect("getrandom for auth token");
    let mut out = String::with_capacity(64);
    for b in buf {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Slap `Cache-Control: no-store` on every API response. Without it,
/// browsers were silently serving cached JSON for endpoints like
/// `/api/repo/branches` — `branches.restart()` after a WS-pushed
/// `refs_changed` would re-fetch but get the same bytes back, the
/// signal would treat the value as unchanged, and the sidebar
/// wouldn't reflect a freshly-created branch until a full page
/// reload.
async fn no_store(req: axum::extract::Request, next: middleware::Next) -> axum::response::Response {
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    resp
}

/// Middleware that gates every endpoint except `/api/health`. Accepts the
/// token from either:
/// - `Authorization: Bearer <token>` — what reqwest and the in-browser
///   `fetch` API use for normal HTTP.
/// - `?token=<token>` query string — the browser WebSocket API can't
///   send custom headers, so the UI tacks the token onto the
///   `/api/repo/events` upgrade URL instead.
async fn require_auth(
    State(auth): State<AuthState>,
    req: axum::extract::Request,
    next: middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let from_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    let from_query = req
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|p| p.strip_prefix("token=")));
    let provided = from_header.or(from_query);
    match provided {
        Some(t) if t == auth.token => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Where to source the WASM bundle from when serving the web UI.
pub enum WebSource {
    /// Serve files from a directory on disk (e.g. `crates/gitrust-web/dist`).
    /// Useful for `make run` dev workflow with live-rebuild.
    Disk(PathBuf),
    /// Serve files from a `Dir` baked into the binary at compile time via
    /// `include_dir!`. The chosen mode for release / `gitrust app` builds.
    Embedded(&'static include_dir::Dir<'static>),
    /// API only — every non-`/api` path 404s.
    None,
}

pub fn router(source: WebSource, auth: AuthState) -> Router {
    // /api/health is the only open endpoint — gives users (and the UI's
    // auth gate) a way to verify the server is reachable before the
    // bearer token is in place. Everything else is gated.
    let open = Router::new().route("/health", get(health));

    let protected = Router::new()
        .route("/repo/summary", get(repo_summary))
        .route("/repo/log", get(repo_log))
        .route("/repo/status", get(repo_status))
        .route("/repo/staged", get(repo_staged))
        .route("/repo/branches", get(repo_branches))
        .route("/repo/tags", get(repo_tags))
        .route("/repo/remotes", get(repo_remotes))
        .route("/repo/tree", get(repo_tree))
        .route("/repo/blob", get(repo_blob))
        .route("/repo/diff", get(repo_diff))
        .route("/repo/diff/working", get(repo_diff_working))
        .route("/repo/blame", get(repo_blame))
        .route("/repo/events", get(repo_events))
        .route("/repo/stage", post(repo_stage))
        .route("/repo/unstage", post(repo_unstage))
        .route("/repo/discard", post(repo_discard))
        .route("/repo/stashes", get(repo_stash_list))
        .route("/repo/stashes/save", post(repo_stash_save))
        .route("/repo/stashes/pop", post(repo_stash_pop))
        .route("/repo/stashes/drop", post(repo_stash_drop))
        .route("/repo/commit", get(repo_commit_get).post(repo_commit))
        .route("/repo/branches/create", post(repo_branch_create))
        .route("/repo/branches/delete", post(repo_branch_delete))
        .route("/repo/branches/rename", post(repo_branch_rename))
        .route("/repo/checkout", post(repo_checkout))
        .route("/repo/fetch", post(repo_fetch))
        .route("/repo/pull", post(repo_pull))
        .route("/repo/push", post(repo_push))
        .route("/repo/merge", post(repo_merge))
        .route("/repo/cherry-pick", post(repo_cherry_pick))
        .route("/repo/state", get(repo_state))
        .route("/repo/merge/abort", post(repo_merge_abort))
        .route("/repo/merge/continue", post(repo_merge_continue))
        .route("/repo/cherry-pick/abort", post(repo_cherry_pick_abort))
        .route(
            "/repo/cherry-pick/continue",
            post(repo_cherry_pick_continue),
        )
        .route("/repo/resolve", post(repo_resolve))
        .route("/repo/stage-hunks", post(repo_stage_hunks))
        .route("/repo/rebase", post(repo_rebase))
        .route("/repo/rebase/abort", post(repo_rebase_abort))
        .route("/repo/rebase/continue", post(repo_rebase_continue))
        .route("/repo/rebase/skip", post(repo_rebase_skip))
        .route("/repo/revert", post(repo_revert))
        .route("/repo/revert/abort", post(repo_revert_abort))
        .route("/repo/revert/continue", post(repo_revert_continue))
        .route("/repo/revert/skip", post(repo_revert_skip))
        .route("/repo/reset", post(repo_reset));

    #[cfg(feature = "desktop")]
    let protected = protected.route("/repo/pick-folder", post(pick_folder));

    let protected =
        protected.route_layer(middleware::from_fn_with_state(auth.clone(), require_auth));

    let api = open
        .merge(protected)
        .layer(middleware::from_fn(no_store))
        .with_state(auth);

    let mut app = Router::new().nest("/api", api);
    match source {
        WebSource::Disk(dist) => {
            app = app.fallback_service(ServeDir::new(dist));
        }
        WebSource::Embedded(bundle) => {
            app = app
                .fallback(move |uri: axum::http::Uri| async move { serve_embedded(uri, bundle) });
        }
        WebSource::None => {}
    }
    app.layer(TraceLayer::new_for_http())
}

fn serve_embedded(
    uri: axum::http::Uri,
    bundle: &'static include_dir::Dir<'static>,
) -> axum::response::Response {
    let path_str = uri.path().trim_start_matches('/');
    let lookup = if path_str.is_empty() {
        "index.html"
    } else {
        path_str
    };
    let Some(file) = bundle.get_file(lookup) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content_type = guess_content_type(lookup);
    let body = axum::body::Body::from(file.contents());
    let mut res = axum::response::Response::new(body);
    res.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static(content_type),
    );
    res
}

fn guess_content_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub async fn serve(addr: SocketAddr, source: WebSource) -> anyhow::Result<()> {
    let auth = AuthState::from_default_path()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    tracing::info!("gitrust-server listening on http://{bound}");
    print_auth_banner(bound, &auth.token);
    axum::serve(listener, router(source, auth)).await?;
    Ok(())
}

fn print_auth_banner(addr: SocketAddr, token: &str) {
    // stdout, line-buffered: stays right after cargo's "Running …"
    // line so the token is the first thing the user sees in the
    // terminal when launching via `make run` or `cargo run`.
    println!();
    println!("  gitrust ready at http://{addr}");
    println!("  paste the access token below into the browser:");
    println!();
    println!("    {token}");
    println!();
}

#[cfg(test)]
mod tests {
    use super::categorize;
    use notify::{Event, EventKind};
    use std::path::{Path, PathBuf};

    fn ev(repo: &Path, rels: &[&str]) -> Event {
        Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Any),
            paths: rels.iter().map(|r| repo.join(r)).collect(),
            attrs: Default::default(),
        }
    }

    #[test]
    fn categorizes_known_paths() {
        let repo = PathBuf::from("/r");
        assert_eq!(
            categorize(&ev(&repo, &[".git/HEAD"]), &repo),
            vec!["head_changed"]
        );
        assert_eq!(
            categorize(&ev(&repo, &[".git/refs/heads/master"]), &repo),
            vec!["refs_changed"],
        );
        assert_eq!(
            categorize(&ev(&repo, &[".git/packed-refs"]), &repo),
            vec!["refs_changed"],
        );
        assert_eq!(
            categorize(&ev(&repo, &[".git/index"]), &repo),
            vec!["index_changed"]
        );
        assert_eq!(
            categorize(&ev(&repo, &["src/main.rs"]), &repo),
            vec!["worktree_changed"],
        );
    }

    #[test]
    fn filters_noise() {
        let repo = PathBuf::from("/r");
        assert!(categorize(&ev(&repo, &[".git/objects/ab/cdef"]), &repo).is_empty());
        assert!(categorize(&ev(&repo, &[".git/index.lock"]), &repo).is_empty());
        assert!(categorize(&ev(&repo, &[".git/lfs/objects/aa/bb/cc"]), &repo).is_empty());
        assert!(categorize(&ev(&repo, &[]), &repo).is_empty());
    }

    #[test]
    fn collects_multiple_kinds_from_one_event() {
        let repo = PathBuf::from("/r");
        let mut out = categorize(&ev(&repo, &[".git/HEAD", "src/main.rs"]), &repo);
        out.sort();
        assert_eq!(out, vec!["head_changed", "worktree_changed"]);
    }

    #[test]
    fn deduplicates_within_one_event() {
        let repo = PathBuf::from("/r");
        let out = categorize(
            &ev(&repo, &[".git/refs/heads/master", ".git/refs/heads/dev"]),
            &repo,
        );
        assert_eq!(out, vec!["refs_changed"]);
    }

    #[test]
    fn ignores_paths_outside_repo() {
        let repo = PathBuf::from("/r");
        let ev = Event {
            kind: EventKind::Any,
            paths: vec![PathBuf::from("/elsewhere/file")],
            attrs: Default::default(),
        };
        assert!(categorize(&ev, &repo).is_empty());
    }
}
