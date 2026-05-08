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
    body: String,
    parents: Vec<String>,
    author_name: String,
    author_email: String,
    time_unix: i64,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct StatusEntry {
    path: String,
    kind: String,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct BranchInfo {
    name: String,
    oid: Option<String>,
    is_head: bool,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct DiffLine {
    added: bool,
    text: String,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct FileDiff {
    path: String,
    kind: String,
    lines: Vec<DiffLine>,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct CommitDiff {
    commit: CommitInfo,
    files: Vec<FileDiff>,
}

const DEFAULT_REPO: &str = "/home/salavat/gitrust";
const LOG_LIMIT: usize = 50;

#[component]
pub fn App() -> Element {
    let mut current_repo = use_signal(|| DEFAULT_REPO.to_string());
    let mut draft_repo = use_signal(|| DEFAULT_REPO.to_string());
    let selected_oid = use_signal(|| None::<String>);

    let summary = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_summary(&path).await }
    });
    let log = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_log(&path, LOG_LIMIT).await }
    });
    let status = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_status(&path).await }
    });
    let branches = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_branches(&path).await }
    });
    let diff = use_resource(move || {
        let path = current_repo.read().clone();
        let oid = selected_oid.read().clone();
        async move {
            match oid {
                Some(o) => fetch_diff(&path, &o).await.map(Some),
                None => Ok::<_, String>(None),
            }
        }
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
            h2 { "Branches" }
            {render_branches(&branches.read_unchecked())}
        }

        section {
            h2 { "Working tree" }
            {render_status(&status.read_unchecked())}
        }

        section {
            h2 { "Recent commits" }
            {render_log(&log.read_unchecked(), selected_oid)}
        }

        section {
            h2 { "Commit detail" }
            {render_diff(&diff.read_unchecked(), selected_oid)}
        }

        p { class: "links",
            "Raw API: "
            a { href: "/api/health", target: "_blank", "health" }
            " · "
            a { href: "/api/repo/summary?path={current_repo}", target: "_blank", "summary" }
            " · "
            a { href: "/api/repo/log?path={current_repo}&limit={LOG_LIMIT}", target: "_blank", "log" }
            " · "
            a { href: "/api/repo/status?path={current_repo}", target: "_blank", "status" }
            " · "
            a { href: "/api/repo/branches?path={current_repo}", target: "_blank", "branches" }
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

fn render_log(
    state: &Option<Result<Vec<CommitInfo>, String>>,
    selected_oid: Signal<Option<String>>,
) -> Element {
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
                            {render_commit_row(c, selected_oid)}
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

fn render_commit_row(c: CommitInfo, mut selected_oid: Signal<Option<String>>) -> Element {
    let is_selected = selected_oid.read().as_deref() == Some(c.oid.as_str());
    let oid_for_click = c.oid.clone();
    rsx! {
        tr {
            key: "{c.oid}",
            class: if is_selected { "selected" } else { "" },
            onclick: move |_| {
                let target = oid_for_click.clone();
                let same = selected_oid.read().as_deref() == Some(target.as_str());
                if same {
                    selected_oid.set(None);
                } else {
                    selected_oid.set(Some(target));
                }
            },
            td { code { "{c.short_oid}" } }
            td { class: "author", "{c.author_name}" }
            td { class: "summary-cell", "{c.summary}" }
            td { class: "when", title: "{c.time_unix}", "{format_time(c.time_unix)}" }
        }
    }
}

fn render_branches(state: &Option<Result<Vec<BranchInfo>, String>>) -> Element {
    match state {
        Some(Ok(branches)) if branches.is_empty() => {
            rsx! { p { class: "muted", "No local branches." } }
        }
        Some(Ok(branches)) => {
            let rows = branches.clone();
            rsx! {
                table { class: "branches",
                    tbody {
                        for b in rows {
                            tr { key: "{b.name}", class: if b.is_head { "head" } else { "" },
                                td { class: "marker", if b.is_head { "●" } else { "" } }
                                td { class: "name", "{b.name}" }
                                td { class: "oid",
                                    code {
                                        {
                                            b.oid
                                                .as_ref()
                                                .map(|o| o.chars().take(12).collect::<String>())
                                                .unwrap_or_else(|| "(none)".to_string())
                                        }
                                    }
                                }
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

fn render_status(state: &Option<Result<Vec<StatusEntry>, String>>) -> Element {
    match state {
        Some(Ok(entries)) if entries.is_empty() => {
            rsx! { p { class: "muted", "Working tree is clean." } }
        }
        Some(Ok(entries)) => {
            let rows = entries.clone();
            rsx! {
                table { class: "status",
                    tbody {
                        for e in rows {
                            tr { key: "{e.path}",
                                td { class: "kind kind-{e.kind}", "{e.kind}" }
                                td { class: "path", code { "{e.path}" } }
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

fn render_diff(
    state: &Option<Result<Option<CommitDiff>, String>>,
    selected_oid: Signal<Option<String>>,
) -> Element {
    if selected_oid.read().is_none() {
        return rsx! { p { class: "muted", "Click a commit row above to inspect its diff." } };
    }
    match state {
        Some(Ok(Some(d))) => {
            let body = d.commit.body.clone();
            let summary = d.commit.summary.clone();
            let oid = d.commit.oid.clone();
            let author = d.commit.author_name.clone();
            let when = format_time(d.commit.time_unix);
            let parents_short = d
                .commit
                .parents
                .iter()
                .map(|p| p.chars().take(8).collect::<String>())
                .collect::<Vec<_>>()
                .join("  ");
            let has_parents = !d.commit.parents.is_empty();
            let files = d.files.clone();
            let no_files = files.is_empty();
            rsx! {
                div { class: "diff-header",
                    div { class: "title", strong { "{summary}" } }
                    div { class: "meta",
                        code { "{oid}" }
                        " · "
                        "{author}"
                        " · "
                        "{when}"
                        if has_parents {
                            " · parents "
                            code { "{parents_short}" }
                        }
                    }
                    if !body.is_empty() {
                        pre { class: "body", "{body}" }
                    }
                }
                if no_files {
                    p { class: "muted", "No file changes." }
                }
                for f in files {
                    {render_file_diff(f)}
                }
            }
        }
        Some(Ok(None)) | None => rsx! { p { class: "muted", "Loading…" } },
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! { p { class: "err", "Error: {msg}" } }
        }
    }
}

fn render_file_diff(f: FileDiff) -> Element {
    let path = f.path.clone();
    let kind = f.kind.clone();
    let lines = f.lines.clone();
    let adds = lines.iter().filter(|l| l.added).count();
    let dels = lines.iter().filter(|l| !l.added).count();
    let no_lines = lines.is_empty();
    rsx! {
        div { class: "file-diff",
            div { class: "file-header",
                span { class: "kind kind-{kind}", "{kind}" }
                code { class: "path", "{path}" }
                span { class: "stats", "+{adds} −{dels}" }
            }
            if !no_lines {
                pre { class: "lines",
                    for l in lines {
                        span { class: if l.added { "add" } else { "del" },
                            {if l.added { "+ " } else { "- " }}
                            "{l.text}"
                            "\n"
                        }
                    }
                }
            }
        }
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
    fetch_json(&format!("/api/repo/summary?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_log(path: &str, limit: usize) -> Result<Vec<CommitInfo>, String> {
    fetch_json(&format!("/api/repo/log?path={path}&limit={limit}")).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_status(path: &str) -> Result<Vec<StatusEntry>, String> {
    fetch_json(&format!("/api/repo/status?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_branches(path: &str) -> Result<Vec<BranchInfo>, String> {
    fetch_json(&format!("/api/repo/branches?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_diff(path: &str, oid: &str) -> Result<CommitDiff, String> {
    fetch_json(&format!("/api/repo/diff?path={path}&oid={oid}")).await
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
async fn fetch_summary(_path: &str) -> Result<RepoSummary, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_log(_path: &str, _limit: usize) -> Result<Vec<CommitInfo>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_status(_path: &str) -> Result<Vec<StatusEntry>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_branches(_path: &str) -> Result<Vec<BranchInfo>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_diff(_path: &str, _oid: &str) -> Result<CommitDiff, String> {
    Err("native build: fetching not implemented".into())
}
