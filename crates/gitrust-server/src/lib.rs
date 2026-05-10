use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{Json, Router, extract::Query, http::StatusCode, response::IntoResponse, routing::get};
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use gitrust_core::{
    BlobView, BranchInfo, CommitDiff, CommitInfo, FileDiff, RemoteBranchInfo, RepoSummary,
    StatusEntry, TagInfo, TreeEntry, diff_commit, diff_working, list_branches,
    list_remote_branches, list_status, list_tags, list_tree, log_commits, show_blob,
    summarize_repo,
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
    log_commits(&path, q.limit.min(500))
        .map(Json)
        .map_err(ApiError::from)
}

async fn repo_status(Query(q): Query<PathQuery>) -> Result<Json<Vec<StatusEntry>>, ApiError> {
    let path = PathBuf::from(q.path);
    list_status(&path).map(Json).map_err(ApiError::from)
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

struct ApiError(anyhow::Error);

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = serde_json::json!({ "error": self.0.to_string() });
        (StatusCode::BAD_REQUEST, Json(body)).into_response()
    }
}

pub fn router(web_dist: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/repo/summary", get(repo_summary))
        .route("/repo/log", get(repo_log))
        .route("/repo/status", get(repo_status))
        .route("/repo/branches", get(repo_branches))
        .route("/repo/tags", get(repo_tags))
        .route("/repo/remotes", get(repo_remotes))
        .route("/repo/tree", get(repo_tree))
        .route("/repo/blob", get(repo_blob))
        .route("/repo/diff", get(repo_diff))
        .route("/repo/diff/working", get(repo_diff_working));

    let mut app = Router::new().nest("/api", api);
    if let Some(dist) = web_dist {
        app = app.fallback_service(ServeDir::new(dist));
    }
    app.layer(TraceLayer::new_for_http())
}

pub async fn serve(addr: SocketAddr, web_dist: Option<PathBuf>) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(
        "gitrust-server listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, router(web_dist)).await?;
    Ok(())
}
