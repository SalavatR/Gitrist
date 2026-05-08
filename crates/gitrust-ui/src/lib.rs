use dioxus::prelude::*;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct RepoSummary {
    path: String,
    git_dir: String,
    head_ref: Option<String>,
    head_oid: Option<String>,
    is_detached: bool,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct CommitInfo {
    oid: String,
    short_oid: String,
    summary: String,
    author_name: String,
    author_email: String,
    time_unix: i64,
}

const DEFAULT_REPO: &str = "/home/salavat/gitrust";
const LOG_LIMIT: usize = 50;

#[component]
pub fn App() -> Element {
    let mut current_repo = use_signal(|| DEFAULT_REPO.to_string());
    let mut draft_repo = use_signal(|| DEFAULT_REPO.to_string());

    let summary = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_summary(&path).await }
    });
    let log = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_log(&path, LOG_LIMIT).await }
    });

    rsx! {
        style { {include_str!("../style.css")} }
        h1 { "gitrust" }
        p { class: "lede",
            "Self-hosted Rust GUI git client. The page is "
            code { "gitrust-ui" }
            " compiled to WebAssembly, served by "
            code { "gitrust-server" }
            "."
        }

        form {
            class: "repo-picker",
            onsubmit: move |e| {
                e.prevent_default();
                current_repo.set(draft_repo.read().clone());
            },
            label { r#for: "repo-input", "Repository path:" }
            input {
                id: "repo-input",
                value: "{draft_repo}",
                spellcheck: "false",
                autocapitalize: "off",
                autocomplete: "off",
                oninput: move |e| draft_repo.set(e.value()),
            }
            button { r#type: "submit", "Load" }
        }

        section {
            h2 { "Summary" }
            {render_summary(&summary.read_unchecked())}
        }

        section {
            h2 { "Recent commits" }
            {render_log(&log.read_unchecked())}
        }

        p { class: "links",
            "Raw API: "
            a { href: "/api/health", target: "_blank", "health" }
            " · "
            a { href: "/api/repo/summary?path={current_repo}", target: "_blank", "summary" }
            " · "
            a { href: "/api/repo/log?path={current_repo}&limit={LOG_LIMIT}", target: "_blank", "log" }
        }
    }
}

fn render_summary(state: &Option<Result<RepoSummary, String>>) -> Element {
    match state {
        Some(Ok(s)) => {
            let branch = s.head_ref.as_deref().unwrap_or("(detached)").to_string();
            let oid_short = s
                .head_oid
                .as_ref()
                .map(|o| o.chars().take(12).collect::<String>())
                .unwrap_or_else(|| "(none)".to_string());
            let path = s.path.clone();
            let git_dir = s.git_dir.clone();
            rsx! {
                table { class: "kv",
                    tr { td { "path" }    td { code { "{path}" } } }
                    tr { td { "git dir" } td { code { "{git_dir}" } } }
                    tr { td { "branch" }  td { code { "{branch}" } } }
                    tr { td { "head" }    td { code { "{oid_short}" } } }
                }
            }
        }
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! { p { class: "err", "Error: {msg}" } }
        }
        None => rsx! { p { class: "muted", "Loading…" } },
    }
}

fn render_log(state: &Option<Result<Vec<CommitInfo>, String>>) -> Element {
    match state {
        Some(Ok(commits)) if commits.is_empty() => {
            rsx! { p { class: "muted", "No commits." } }
        }
        Some(Ok(commits)) => {
            let rows = commits.clone();
            rsx! {
                table { class: "log",
                    thead {
                        tr {
                            th { "commit" }
                            th { "author" }
                            th { "message" }
                            th { "when" }
                        }
                    }
                    tbody {
                        for c in rows {
                            tr { key: "{c.oid}",
                                td { code { "{c.short_oid}" } }
                                td { class: "author", "{c.author_name}" }
                                td { class: "summary", "{c.summary}" }
                                td { class: "when", title: "{c.time_unix}", "{format_time(c.time_unix)}" }
                            }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! { p { class: "err", "Error: {msg}" } }
        }
        None => rsx! { p { class: "muted", "Loading…" } },
    }
}

fn format_time(unix: i64) -> String {
    OffsetDateTime::from_unix_timestamp(unix)
        .ok()
        .and_then(|dt| dt.format(&Rfc3339).ok())
        .unwrap_or_else(|| unix.to_string())
}

#[cfg(target_arch = "wasm32")]
async fn fetch_summary(path: &str) -> Result<RepoSummary, String> {
    let url = format!("/api/repo/summary?path={path}");
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<RepoSummary>().await.map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
async fn fetch_log(path: &str, limit: usize) -> Result<Vec<CommitInfo>, String> {
    let url = format!("/api/repo/log?path={path}&limit={limit}");
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<Vec<CommitInfo>>()
        .await
        .map_err(|e| e.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_summary(_path: &str) -> Result<RepoSummary, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_log(_path: &str, _limit: usize) -> Result<Vec<CommitInfo>, String> {
    Err("native build: fetching not implemented".into())
}
