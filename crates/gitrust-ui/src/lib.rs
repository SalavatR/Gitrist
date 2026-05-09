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
    kind: String,
    old_line: Option<u32>,
    new_line: Option<u32>,
    text: String,
    #[serde(default)]
    tokens: Option<Vec<HToken>>,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct HToken {
    text: String,
    class: String,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct DiffHunk {
    old_start: u32,
    old_count: u32,
    new_start: u32,
    new_count: u32,
    lines: Vec<DiffLine>,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct FileDiff {
    path: String,
    kind: String,
    is_binary: bool,
    hunks: Vec<DiffHunk>,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct CommitDiff {
    commit: CommitInfo,
    files: Vec<FileDiff>,
}

const DEFAULT_REPO: &str = "/home/salavat/gitrust";
const LOG_LIMIT: usize = 50;
#[cfg(target_arch = "wasm32")]
const REPO_STORAGE_KEY: &str = "gitrust.repo";

#[component]
pub fn App() -> Element {
    let initial = initial_repo();
    let mut current_repo = use_signal(|| initial.clone());
    let mut draft_repo = use_signal(|| initial.clone());
    let selected_oid = use_signal(|| None::<String>);
    let selected_file = use_signal(|| None::<String>);

    use_effect(move || {
        let path = current_repo.read().clone();
        persist_repo(&path);
    });

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
    let working_diff = use_resource(move || {
        let path = current_repo.read().clone();
        let file = selected_file.read().clone();
        async move {
            match file {
                Some(f) => fetch_diff_working(&path, &f).await.map(Some),
                None => Ok::<_, String>(None),
            }
        }
    });

    rsx! {
        style { {include_str!("../style.css")} }
        div { class: "app",
            header { class: "topbar",
                div { class: "brand",
                    span { class: "logo-mark", "g" }
                    span { class: "logo-name", "gitrust" }
                }
                form {
                    class: "repo-picker",
                    onsubmit: move |e| {
                        e.prevent_default();
                        current_repo.set(draft_repo.read().clone());
                    },
                    input {
                        id: "repo-input",
                        value: "{draft_repo}",
                        placeholder: "/absolute/path/to/repo",
                        spellcheck: "false",
                        autocapitalize: "off",
                        autocomplete: "off",
                        oninput: move |e| draft_repo.set(e.value()),
                    }
                    button { r#type: "submit", "Load" }
                }
            }

            div { class: "split",
                aside { class: "sidebar",
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Branches" }
                            {render_branch_count(&branches.read_unchecked())}
                        }
                        {render_branches(&branches.read_unchecked())}
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Working tree" }
                            {render_status_count(&status.read_unchecked())}
                        }
                        {render_status(&status.read_unchecked(), selected_oid, selected_file)}
                    }
                }

                main { class: "main",
                    {render_summary_card(&summary.read_unchecked())}

                    section { class: "main-block",
                        h2 { "History" }
                        {render_log(&log.read_unchecked(), selected_oid, selected_file)}
                    }

                    section { class: "main-block",
                        h2 {
                            if selected_file.read().is_some() { "Working tree change" }
                            else { "Commit detail" }
                        }
                        {render_detail(
                            &diff.read_unchecked(),
                            &working_diff.read_unchecked(),
                            selected_oid,
                            selected_file,
                        )}
                    }
                }
            }

            footer { class: "footbar",
                "API · "
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
}

fn render_summary_card(state: &Option<Result<RepoSummary, String>>) -> Element {
    match state {
        Some(Ok(s)) => {
            let branch = s.head_ref.as_deref().unwrap_or("(detached)").to_string();
            let oid_short = s
                .head_oid
                .as_ref()
                .map(|o| o.chars().take(12).collect::<String>())
                .unwrap_or_else(|| "(none)".to_string());
            let path = s.path.clone();
            rsx! {
                section { class: "summary-card",
                    div { class: "sc-path", code { "{path}" } }
                    div { class: "sc-head",
                        span { class: "sc-branch", "{branch}" }
                        span { class: "sc-oid", code { "{oid_short}" } }
                    }
                }
            }
        }
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! { section { class: "summary-card error", "Error: {msg}" } }
        }
        None => rsx! { section { class: "summary-card muted", "Loading…" } },
    }
}

fn render_branch_count(state: &Option<Result<Vec<BranchInfo>, String>>) -> Element {
    if let Some(Ok(bs)) = state {
        let n = bs.len();
        rsx! { span { class: "count", "{n}" } }
    } else {
        rsx! {}
    }
}

fn render_status_count(state: &Option<Result<Vec<StatusEntry>, String>>) -> Element {
    if let Some(Ok(s)) = state {
        let n = s.len();
        if n == 0 {
            rsx! { span { class: "count clean", "clean" } }
        } else {
            rsx! { span { class: "count", "{n}" } }
        }
    } else {
        rsx! {}
    }
}

fn render_branches(state: &Option<Result<Vec<BranchInfo>, String>>) -> Element {
    match state {
        Some(Ok(branches)) if branches.is_empty() => {
            rsx! { p { class: "muted small", "No local branches." } }
        }
        Some(Ok(branches)) => {
            let rows = branches.clone();
            rsx! {
                ul { class: "branch-list",
                    for b in rows {
                        li { key: "{b.name}", class: if b.is_head { "head" } else { "" },
                            span { class: "marker", if b.is_head { "●" } else { "○" } }
                            span { class: "name", "{b.name}" }
                            span { class: "oid",
                                {
                                    b.oid
                                        .as_ref()
                                        .map(|o| o.chars().take(7).collect::<String>())
                                        .unwrap_or_else(|| "—".to_string())
                                }
                            }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! { p { class: "err small", "Error: {msg}" } }
        }
        None => rsx! { p { class: "muted small", "Loading…" } },
    }
}

fn render_status(
    state: &Option<Result<Vec<StatusEntry>, String>>,
    mut selected_oid: Signal<Option<String>>,
    mut selected_file: Signal<Option<String>>,
) -> Element {
    match state {
        Some(Ok(entries)) if entries.is_empty() => {
            rsx! { p { class: "muted small", "Working tree is clean." } }
        }
        Some(Ok(entries)) => {
            let rows = entries.clone();
            rsx! {
                ul { class: "status-list",
                    for e in rows {
                        {
                            let path = e.path.clone();
                            let path_for_class = e.path.clone();
                            let is_selected = selected_file.read().as_deref() == Some(path_for_class.as_str());
                            rsx! {
                                li {
                                    key: "{e.path}",
                                    class: if is_selected { "selected" } else { "" },
                                    onclick: move |_| {
                                        let target = path.clone();
                                        let same = selected_file.read().as_deref() == Some(target.as_str());
                                        if same {
                                            selected_file.set(None);
                                        } else {
                                            selected_file.set(Some(target));
                                            selected_oid.set(None);
                                        }
                                    },
                                    span { class: "badge badge-{e.kind}", title: "{e.kind}",
                                        {status_glyph(&e.kind)}
                                    }
                                    span { class: "path", "{e.path}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! { p { class: "err small", "Error: {msg}" } }
        }
        None => rsx! { p { class: "muted small", "Loading…" } },
    }
}

fn status_glyph(kind: &str) -> &'static str {
    match kind {
        "modified" => "M",
        "added" => "A",
        "untracked" => "?",
        "conflict" => "!",
        "deleted" => "D",
        _ => "•",
    }
}

fn render_log(
    state: &Option<Result<Vec<CommitInfo>, String>>,
    selected_oid: Signal<Option<String>>,
    selected_file: Signal<Option<String>>,
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
                            th { class: "th-oid", "commit" }
                            th { class: "th-author", "author" }
                            th { class: "th-msg", "message" }
                            th { class: "th-when", "when" }
                        }
                    }
                    tbody {
                        for c in rows {
                            {render_commit_row(c, selected_oid, selected_file)}
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

fn render_commit_row(
    c: CommitInfo,
    mut selected_oid: Signal<Option<String>>,
    mut selected_file: Signal<Option<String>>,
) -> Element {
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
                    selected_file.set(None);
                }
            },
            td { class: "td-oid", code { "{c.short_oid}" } }
            td { class: "td-author", "{c.author_name}" }
            td { class: "td-msg", "{c.summary}" }
            td { class: "td-when", title: "{c.time_unix}", "{format_time(c.time_unix)}" }
        }
    }
}

fn render_detail(
    commit_state: &Option<Result<Option<CommitDiff>, String>>,
    working_state: &Option<Result<Option<FileDiff>, String>>,
    selected_oid: Signal<Option<String>>,
    selected_file: Signal<Option<String>>,
) -> Element {
    if let Some(file) = selected_file.read().clone() {
        return render_working_detail(working_state, &file);
    }
    if selected_oid.read().is_some() {
        return render_commit_detail(commit_state);
    }
    rsx! { p { class: "muted", "Select a commit or status entry to inspect." } }
}

fn render_working_detail(
    state: &Option<Result<Option<FileDiff>, String>>,
    file: &str,
) -> Element {
    match state {
        Some(Ok(Some(f))) => {
            let path = file.to_string();
            let kind = f.kind.clone();
            let f_clone = f.clone();
            rsx! {
                div { class: "diff-header",
                    div { class: "title",
                        span { class: "kind kind-{kind}", "{kind}" }
                        " "
                        code { class: "full-oid", "{path}" }
                    }
                    div { class: "meta", "Working tree vs index" }
                }
                {render_file_diff(f_clone)}
            }
        }
        Some(Ok(None)) | None => rsx! { p { class: "muted", "Loading…" } },
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! { p { class: "err", "Error: {msg}" } }
        }
    }
}

fn render_commit_detail(state: &Option<Result<Option<CommitDiff>, String>>) -> Element {
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
                    div { class: "title", "{summary}" }
                    div { class: "meta",
                        code { class: "full-oid", "{oid}" }
                        " · "
                        span { "{author}" }
                        " · "
                        span { "{when}" }
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

/// Files with more than this many diff lines start collapsed. Tunable.
const AUTO_COLLAPSE_LINES: usize = 300;

fn render_file_diff(f: FileDiff) -> Element {
    let path = f.path.clone();
    let kind = f.kind.clone();
    let is_binary = f.is_binary;
    let hunks = f.hunks.clone();
    let mut adds = 0usize;
    let mut dels = 0usize;
    let mut total_lines = 0usize;
    for h in &hunks {
        for l in &h.lines {
            total_lines += 1;
            match l.kind.as_str() {
                "add" => adds += 1,
                "del" => dels += 1,
                _ => {}
            }
        }
    }
    let no_hunks = hunks.is_empty();
    // Auto-collapse: huge diffs and binary/renamed files.
    let auto_open = !is_binary && !no_hunks && total_lines <= AUTO_COLLAPSE_LINES;
    rsx! {
        details { class: "file-diff", open: auto_open,
            summary { class: "file-header",
                span { class: "disclosure" }
                span { class: "kind kind-{kind}", "{kind}" }
                code { class: "path", "{path}" }
                span { class: "stats",
                    span { class: "add-stat", "+{adds}" }
                    " "
                    span { class: "del-stat", "−{dels}" }
                }
            }
            if is_binary {
                div { class: "binary-note", "Binary file, diff omitted." }
            } else if no_hunks {
                div { class: "binary-note", "No textual changes." }
            } else {
                for h in hunks {
                    {render_hunk(h)}
                }
            }
        }
    }
}

fn render_hunk(h: DiffHunk) -> Element {
    let header = format!(
        "@@ -{},{} +{},{} @@",
        h.old_start, h.old_count, h.new_start, h.new_count
    );
    let lines = h.lines.clone();
    rsx! {
        div { class: "hunk",
            div { class: "hunk-header", "{header}" }
            div { class: "hunk-lines",
                for l in lines {
                    {render_diff_line(l)}
                }
            }
        }
    }
}

fn render_diff_line(l: DiffLine) -> Element {
    let kind = l.kind.clone();
    let old = l
        .old_line
        .map(|n| n.to_string())
        .unwrap_or_default();
    let new = l
        .new_line
        .map(|n| n.to_string())
        .unwrap_or_default();
    let marker = match kind.as_str() {
        "add" => "+",
        "del" => "-",
        _ => " ",
    };
    let tokens = l.tokens.clone();
    let plain_text = l.text.clone();
    rsx! {
        div { class: "diff-line line-{kind}",
            span { class: "ln old", "{old}" }
            span { class: "ln new", "{new}" }
            span { class: "marker", "{marker}" }
            span { class: "text",
                if let Some(toks) = tokens {
                    if toks.is_empty() {
                        {rsx! { "{plain_text}" }}
                    } else {
                        for t in toks {
                            span { class: "tok tok-{token_class_to_css(&t.class)}", "{t.text}" }
                        }
                    }
                } else {
                    {rsx! { "{plain_text}" }}
                }
            }
        }
    }
}

fn token_class_to_css(class: &str) -> String {
    if class.is_empty() {
        "plain".to_string()
    } else {
        class.replace('.', "-")
    }
}

#[cfg(target_arch = "wasm32")]
fn initial_repo() -> String {
    use gloo_storage::Storage;
    let window = gloo_utils::window();
    if let Ok(hash) = window.location().hash() {
        if hash.len() > 1 {
            if let Ok(decoded) = urlencoding::decode(&hash[1..]) {
                let s = decoded.into_owned();
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    if let Ok(stored) = gloo_storage::LocalStorage::get::<String>(REPO_STORAGE_KEY) {
        if !stored.is_empty() {
            return stored;
        }
    }
    DEFAULT_REPO.to_string()
}

#[cfg(not(target_arch = "wasm32"))]
fn initial_repo() -> String {
    DEFAULT_REPO.to_string()
}

#[cfg(target_arch = "wasm32")]
fn persist_repo(path: &str) {
    use gloo_storage::Storage;
    if path.is_empty() {
        return;
    }
    let _ = gloo_storage::LocalStorage::set(REPO_STORAGE_KEY, path);
    let window = gloo_utils::window();
    let encoded = urlencoding::encode(path);
    let _ = window.location().set_hash(encoded.as_ref());
}

#[cfg(not(target_arch = "wasm32"))]
fn persist_repo(_path: &str) {}

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
async fn fetch_diff_working(path: &str, file: &str) -> Result<FileDiff, String> {
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

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_diff_working(_path: &str, _file: &str) -> Result<FileDiff, String> {
    Err("native build: fetching not implemented".into())
}
