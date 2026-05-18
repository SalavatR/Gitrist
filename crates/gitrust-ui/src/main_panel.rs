//! Right-pane render functions: summary card at the top, the commit
//! history table, and the unified detail panel that switches between
//! the blob viewer, the working-tree diff, and the commit diff based
//! on which signal is non-empty.

use std::collections::HashMap;

use dioxus::prelude::*;
use gitrust_types::{
    BlameLine, BlameView, BlobLine, BlobView, CommitDiff, CommitInfo, FileDiff, RepoSummary,
};

use std::collections::HashSet;

use gitrust_types::{ConflictBlock, ConflictView};

use crate::diff::{render_file_diff, render_line_content};
use crate::graph::{RowLayout, compute_graph, graph_width, render_graph_cell};
use crate::state::BlobSelection;
use crate::time_fmt::{format_time, format_time_relative};

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_commit_form(
    mut message: Signal<String>,
    mut error: Signal<Option<String>>,
    mut author: Signal<String>,
    staged_count: usize,
    current_repo: Signal<String>,
) -> Element {
    let msg_text = message.read().clone();
    let author_text = author.read().clone();
    let can_submit = !msg_text.trim().is_empty() && staged_count > 0;
    let err_text = error.read().clone();
    rsx! {
        section { class: "commit-form",
            h2 { "Commit" }
            textarea {
                class: "commit-msg",
                placeholder: "Subject line, then optionally a body…",
                value: "{msg_text}",
                rows: "3",
                oninput: move |e| {
                    message.set(e.value());
                    error.set(None);
                },
            }
            input {
                class: "commit-author",
                r#type: "text",
                placeholder: "Author override — Name <email>  (optional)",
                value: "{author_text}",
                spellcheck: "false",
                autocapitalize: "off",
                autocomplete: "off",
                oninput: move |e| author.set(e.value()),
            }
            div { class: "commit-row",
                if staged_count == 0 {
                    span { class: "muted small", "Nothing staged." }
                } else {
                    span { class: "muted small", "{staged_count} staged" }
                }
                button {
                    class: "commit-btn",
                    disabled: !can_submit,
                    onclick: move |_| {
                        let path = current_repo.read().clone();
                        let body = message.read().clone();
                        let auth = author.read().trim().to_string();
                        let auth_opt = if auth.is_empty() { None } else { Some(auth) };
                        spawn(async move {
                            match crate::fetch::post_commit(&path, &body, auth_opt.as_deref()).await {
                                Ok(_) => {
                                    message.set(String::new());
                                    error.set(None);
                                    // Keep author across commits so repeated
                                    // commits in a session don't re-type it.
                                }
                                Err(e) => error.set(Some(e)),
                            }
                        });
                    },
                    "Commit"
                }
            }
            if let Some(e) = err_text {
                p { class: "err small", "Commit failed: {e}" }
            }
        }
    }
}

pub(crate) fn render_summary_card(
    state: &Option<Result<RepoSummary, String>>,
    mut res: Resource<Result<RepoSummary, String>>,
) -> Element {
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
            rsx! {
                section { class: "summary-card error",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
        None => rsx! { section { class: "summary-card muted", "Loading…" } },
    }
}

pub(crate) fn render_log(
    state: &Option<Result<Vec<CommitInfo>, String>>,
    mut res: Resource<Result<Vec<CommitInfo>, String>>,
    selected_oid: Signal<Option<String>>,
    selected_file: Signal<Option<String>>,
    selected_blob: Signal<Option<BlobSelection>>,
    show_graph: bool,
) -> Element {
    match state {
        Some(Ok(commits)) if commits.is_empty() => {
            rsx! { p { class: "muted", "No commits." } }
        }
        Some(Ok(commits)) => {
            let rows = commits.clone();
            // The graph only makes sense over a contiguous ancestry walk.
            // When the log is search-filtered the result set is sparse,
            // and a graph rendered over it would lie about parent edges.
            let layouts: Vec<RowLayout> = if show_graph {
                compute_graph(&rows)
            } else {
                Vec::new()
            };
            let g_width = graph_width(&layouts);
            rsx! {
                table { class: "log",
                    thead {
                        tr {
                            if show_graph && g_width > 0 {
                                th { class: "th-graph", "graph" }
                            }
                            th { class: "th-oid", "commit" }
                            th { class: "th-author", "author" }
                            th { class: "th-msg", "message" }
                            th { class: "th-when", "when" }
                        }
                    }
                    tbody {
                        for (i, c) in rows.into_iter().enumerate() {
                            {render_commit_row(
                                c,
                                layouts.get(i),
                                g_width,
                                selected_oid,
                                selected_file,
                                selected_blob,
                            )}
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! {
                p { class: "err",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
        None => rsx! { p { class: "muted", "Loading…" } },
    }
}

fn render_commit_row(
    c: CommitInfo,
    layout: Option<&RowLayout>,
    g_width: usize,
    mut selected_oid: Signal<Option<String>>,
    mut selected_file: Signal<Option<String>>,
    mut selected_blob: Signal<Option<BlobSelection>>,
) -> Element {
    let is_selected = selected_oid.read().as_deref() == Some(c.oid.as_str());
    let oid_for_click = c.oid.clone();
    let graph_cell = layout
        .filter(|_| g_width > 0)
        .map(|l| render_graph_cell(l, g_width));
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
                    selected_blob.set(None);
                }
            },
            if let Some(g) = graph_cell {
                td { class: "td-graph", {g} }
            }
            td { class: "td-oid", code { "{c.short_oid}" } }
            td { class: "td-author", "{c.author_name}" }
            td { class: "td-msg", "{c.summary}" }
            td { class: "td-when", title: "{format_time(c.time_unix)}", "{format_time_relative(c.time_unix)}" }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_detail(
    commit_state: &Option<Result<Option<CommitDiff>, String>>,
    commit_res: Resource<Result<Option<CommitDiff>, String>>,
    working_state: &Option<Result<Option<FileDiff>, String>>,
    working_res: Resource<Result<Option<FileDiff>, String>>,
    working_res_for_restart: Resource<Result<Option<FileDiff>, String>>,
    blob_state: &Option<Result<Option<BlobView>, String>>,
    blob_res: Resource<Result<Option<BlobView>, String>>,
    blame_state: &Option<Result<Option<BlameView>, String>>,
    blob_query: Signal<String>,
    selected_oid: Signal<Option<String>>,
    selected_file: Signal<Option<String>>,
    selected_blob: Signal<Option<BlobSelection>>,
    side_by_side: bool,
    current_repo: Signal<String>,
    hunk_picker: Signal<HashSet<usize>>,
    net_busy: Signal<bool>,
    net_result: Signal<Option<Result<gitrust_types::NetworkOpResult, String>>>,
    file_history: Signal<Option<String>>,
    conflict_state: &Option<Result<Option<ConflictView>, String>>,
    conflict_res: Resource<Result<Option<ConflictView>, String>>,
    state_res: Resource<Result<gitrust_types::RepoState, String>>,
    refs_diff_state: &Option<Result<Option<Vec<FileDiff>>, String>>,
    refs_diff_res: Resource<Result<Option<Vec<FileDiff>>, String>>,
    compare_refs: Signal<Option<(String, String)>>,
    staged_diff_state: &Option<Result<Option<FileDiff>, String>>,
    staged_diff_res: Resource<Result<Option<FileDiff>, String>>,
    unstage_target: Signal<Option<String>>,
    unstage_picker: Signal<HashSet<usize>>,
) -> Element {
    // Most-specific intent wins. The Unstage view fires from a precise
    // sidebar click and the user expects the panel to switch right then.
    if let Some(file) = unstage_target.read().clone() {
        return render_unstage_detail(
            staged_diff_state,
            staged_diff_res,
            &file,
            side_by_side,
            current_repo,
            unstage_target,
            unstage_picker,
            net_busy,
            net_result,
        );
    }
    if compare_refs.read().is_some() {
        return render_refs_diff(refs_diff_state, refs_diff_res, compare_refs, side_by_side);
    }
    if selected_blob.read().is_some() {
        return render_blob_viewer(blob_state, blob_res, blame_state, blob_query, file_history);
    }
    if let Some(file) = selected_file.read().clone() {
        // If the file is mid-conflict, render the per-hunk picker
        // instead of the standard working-tree diff. The conflict
        // resource is keyed on selected_file so it always tracks
        // the current file.
        let has_conflict = conflict_state
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .and_then(|opt| opt.as_ref())
            .is_some_and(|v| !v.blocks.is_empty());
        if has_conflict {
            return render_conflict_view(
                conflict_state,
                conflict_res,
                state_res,
                &file,
                current_repo,
                net_busy,
                net_result,
            );
        }
        return render_working_detail(
            working_state,
            working_res,
            working_res_for_restart,
            &file,
            side_by_side,
            current_repo,
            hunk_picker,
            net_busy,
            net_result,
        );
    }
    if selected_oid.read().is_some() {
        return render_commit_detail(commit_state, commit_res, side_by_side);
    }
    rsx! { p { class: "muted", "Select a commit, status entry, or file to inspect." } }
}

#[allow(clippy::too_many_arguments)]
fn render_unstage_detail(
    state: &Option<Result<Option<FileDiff>, String>>,
    mut res: Resource<Result<Option<FileDiff>, String>>,
    file: &str,
    side_by_side: bool,
    current_repo: Signal<String>,
    mut unstage_target: Signal<Option<String>>,
    mut unstage_picker: Signal<HashSet<usize>>,
    mut net_busy: Signal<bool>,
    mut net_result: Signal<Option<Result<gitrust_types::NetworkOpResult, String>>>,
) -> Element {
    match state {
        Some(Ok(Some(f))) => {
            let path = file.to_string();
            let path_for_action = path.clone();
            let kind = f.kind.clone();
            let f_clone = f.clone();
            let stageable = kind == "modified" && !f.is_binary;
            let picked = unstage_picker.read().clone();
            let n_picked = picked.len();
            let busy = *net_busy.read();
            rsx! {
                div { class: "diff-header",
                    div { class: "title",
                        span { class: "kind kind-{kind}", "{kind}" }
                        " "
                        code { class: "full-oid", "{path}" }
                        button {
                            class: "blob-history-btn",
                            title: "Close the unstage view",
                            onclick: move |_| unstage_target.set(None),
                            "Close"
                        }
                    }
                    div { class: "meta", "Index vs HEAD (staged)" }
                    if stageable {
                        div { class: "hunk-stage-bar",
                            span { class: "muted small",
                                if n_picked == 0 {
                                    "Tick hunks to unstage them piecemeal."
                                } else if n_picked == 1 {
                                    "1 hunk selected"
                                } else {
                                    "{n_picked} hunks selected"
                                }
                            }
                            button {
                                class: "stage-hunks-btn",
                                disabled: busy || n_picked == 0,
                                onclick: move |_| {
                                    let repo = current_repo.read().clone();
                                    let file = path_for_action.clone();
                                    let hunks: Vec<usize> = unstage_picker
                                        .read()
                                        .iter()
                                        .copied()
                                        .collect();
                                    net_busy.set(true);
                                    net_result.set(None);
                                    let mut res = res;
                                    spawn(async move {
                                        let r = crate::fetch::post_unstage_hunks(
                                            &repo, &file, &hunks,
                                        ).await;
                                        net_busy.set(false);
                                        if let Err(e) = r {
                                            net_result.set(Some(Err(e)));
                                        } else {
                                            unstage_picker.set(HashSet::new());
                                        }
                                        res.restart();
                                    });
                                },
                                if n_picked == 0 { "Unstage hunks" } else { "Unstage {n_picked} hunk(s)" }
                            }
                        }
                    }
                }
                {render_file_diff(f_clone, side_by_side, if stageable { Some(unstage_picker) } else { None })}
            }
        }
        Some(Ok(None)) | None => rsx! { p { class: "muted", "Loading…" } },
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! {
                p { class: "err",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
    }
}

fn render_refs_diff(
    state: &Option<Result<Option<Vec<FileDiff>>, String>>,
    mut res: Resource<Result<Option<Vec<FileDiff>>, String>>,
    compare_refs: Signal<Option<(String, String)>>,
    side_by_side: bool,
) -> Element {
    let (from_label, to_label) = compare_refs
        .read()
        .as_ref()
        .map(|(f, t)| (f.clone(), t.clone()))
        .unwrap_or_default();
    match state {
        Some(Ok(Some(files))) => {
            let n = files.len();
            let rows = files.clone();
            rsx! {
                div { class: "diff-header",
                    div { class: "title",
                        span { class: "kind kind-modified", "compare" }
                        " "
                        code { class: "full-oid", "{from_label} → {to_label}" }
                    }
                    div { class: "meta",
                        if n == 0 {
                            "Identical — no file changes between these refs."
                        } else if n == 1 {
                            "1 file changed"
                        } else {
                            "{n} files changed"
                        }
                    }
                }
                for f in rows {
                    {render_file_diff(f, side_by_side, None)}
                }
            }
        }
        Some(Ok(None)) | None => rsx! { p { class: "muted", "Loading…" } },
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! {
                p { class: "err",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_conflict_view(
    state: &Option<Result<Option<ConflictView>, String>>,
    mut res: Resource<Result<Option<ConflictView>, String>>,
    state_res: Resource<Result<gitrust_types::RepoState, String>>,
    file: &str,
    current_repo: Signal<String>,
    net_busy: Signal<bool>,
    net_result: Signal<Option<Result<gitrust_types::NetworkOpResult, String>>>,
) -> Element {
    match state {
        Some(Ok(Some(view))) => {
            let path = file.to_string();
            let n = view.blocks.len();
            let blocks = view.blocks.clone();
            let busy = *net_busy.read();
            rsx! {
                div { class: "diff-header",
                    div { class: "title",
                        span { class: "kind kind-conflict", "conflict" }
                        " "
                        code { class: "full-oid", "{path}" }
                    }
                    div { class: "meta", "{n} unresolved hunk(s)" }
                }
                div { class: "conflict-blocks",
                    for block in blocks {
                        {render_conflict_block(
                            block,
                            path.clone(),
                            current_repo,
                            res,
                            state_res,
                            net_busy,
                            net_result,
                            busy,
                        )}
                    }
                }
            }
        }
        Some(Ok(None)) | None => rsx! { p { class: "muted", "Loading…" } },
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! {
                p { class: "err",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_conflict_block(
    block: ConflictBlock,
    file: String,
    current_repo: Signal<String>,
    conflict_res: Resource<Result<Option<ConflictView>, String>>,
    state_res: Resource<Result<gitrust_types::RepoState, String>>,
    net_busy: Signal<bool>,
    net_result: Signal<Option<Result<gitrust_types::NetworkOpResult, String>>>,
    busy: bool,
) -> Element {
    let idx = block.index;
    let start = block.start_line;
    let end = block.end_line;
    let ours_label = if block.ours_label.is_empty() {
        "ours".to_string()
    } else {
        block.ours_label.clone()
    };
    let theirs_label = if block.theirs_label.is_empty() {
        "theirs".to_string()
    } else {
        block.theirs_label.clone()
    };
    let ours = block.ours.clone();
    let theirs = block.theirs.clone();
    let base = block.base.clone();

    // Each button gets its own clone of the file path so the per-
    // button closures stay FnOnce-friendly (file is a String, not Copy).
    let file_ours = file.clone();
    let file_theirs = file.clone();
    let file_both1 = file.clone();
    let file_both2 = file;

    rsx! {
        div { class: "conflict-block",
            div { class: "conflict-header",
                strong { "Hunk {idx + 1}" }
                span { class: "muted small", " · lines {start}–{end}" }
            }
            div { class: "conflict-cols",
                div { class: "conflict-col ours",
                    div { class: "col-label", "{ours_label}" }
                    pre { class: "col-body",
                        if ours.is_empty() {
                            span { class: "muted small", "(empty)" }
                        } else {
                            "{ours.join(\"\\n\")}"
                        }
                    }
                }
                if let Some(b) = base {
                    div { class: "conflict-col base",
                        div { class: "col-label", "base" }
                        pre { class: "col-body",
                            if b.is_empty() {
                                span { class: "muted small", "(empty)" }
                            } else {
                                "{b.join(\"\\n\")}"
                            }
                        }
                    }
                }
                div { class: "conflict-col theirs",
                    div { class: "col-label", "{theirs_label}" }
                    pre { class: "col-body",
                        if theirs.is_empty() {
                            span { class: "muted small", "(empty)" }
                        } else {
                            "{theirs.join(\"\\n\")}"
                        }
                    }
                }
            }
            div { class: "conflict-actions",
                button {
                    class: "conflict-action",
                    disabled: busy,
                    onclick: move |_| {
                        let p = current_repo.read().clone();
                        fire_resolve_hunk(p, file_ours.clone(), idx, "ours",
                            net_busy, net_result, conflict_res, state_res);
                    },
                    "Use ours"
                }
                button {
                    class: "conflict-action",
                    disabled: busy,
                    onclick: move |_| {
                        let p = current_repo.read().clone();
                        fire_resolve_hunk(p, file_theirs.clone(), idx, "theirs",
                            net_busy, net_result, conflict_res, state_res);
                    },
                    "Use theirs"
                }
                button {
                    class: "conflict-action",
                    disabled: busy,
                    title: "Keep both sides, ours-then-theirs",
                    onclick: move |_| {
                        let p = current_repo.read().clone();
                        fire_resolve_hunk(p, file_both1.clone(), idx, "both-ours-first",
                            net_busy, net_result, conflict_res, state_res);
                    },
                    "Both (ours first)"
                }
                button {
                    class: "conflict-action",
                    disabled: busy,
                    title: "Keep both sides, theirs-then-ours",
                    onclick: move |_| {
                        let p = current_repo.read().clone();
                        fire_resolve_hunk(p, file_both2.clone(), idx, "both-theirs-first",
                            net_busy, net_result, conflict_res, state_res);
                    },
                    "Both (theirs first)"
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_resolve_hunk(
    path: String,
    file: String,
    idx: usize,
    side: &'static str,
    mut net_busy: Signal<bool>,
    mut net_result: Signal<Option<Result<gitrust_types::NetworkOpResult, String>>>,
    mut conflict_res: Resource<Result<Option<ConflictView>, String>>,
    mut state_res: Resource<Result<gitrust_types::RepoState, String>>,
) {
    net_busy.set(true);
    net_result.set(None);
    spawn(async move {
        let r = crate::fetch::post_resolve_hunk(&path, &file, idx, side).await;
        net_busy.set(false);
        if let Err(e) = r {
            net_result.set(Some(Err(e)));
        }
        conflict_res.restart();
        state_res.restart();
    });
}

fn render_blob_viewer(
    state: &Option<Result<Option<BlobView>, String>>,
    mut res: Resource<Result<Option<BlobView>, String>>,
    blame_state: &Option<Result<Option<BlameView>, String>>,
    mut blob_query: Signal<String>,
    mut file_history: Signal<Option<String>>,
) -> Element {
    match state {
        Some(Ok(Some(b))) => {
            let path = b.path.clone();
            let oid = b.oid.clone();
            let size = b.size;
            let is_binary = b.is_binary;
            let line_count = b.lines.len();
            let lines = b.lines.clone();
            let history_target = path.clone();

            // Key blame entries by line_number so we can attach an
            // annotation to each blob line without a per-row scan.
            let blame_by_line: HashMap<u32, BlameLine> = blame_state
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .and_then(|opt| opt.as_ref())
                .map(|bv| {
                    bv.lines
                        .iter()
                        .map(|l| (l.line_number, l.clone()))
                        .collect()
                })
                .unwrap_or_default();

            let query_raw = blob_query.read().clone();
            let query_lc = query_raw.trim().to_lowercase();
            let match_count = if query_lc.is_empty() {
                0
            } else {
                lines
                    .iter()
                    .filter(|l| l.text.to_lowercase().contains(&query_lc))
                    .count()
            };

            rsx! {
                div { class: "diff-header",
                    div { class: "title",
                        code { class: "full-oid", "{path}" }
                        button {
                            class: "blob-history-btn",
                            title: "Show this file's commit history (git log --follow)",
                            onclick: move |_| {
                                file_history.set(Some(history_target.clone()));
                            },
                            "History"
                        }
                    }
                    div { class: "meta",
                        "{line_count} lines · {size} bytes · "
                        code { "{oid}" }
                    }
                }
                if !is_binary {
                    div { class: "blob-search",
                        input {
                            r#type: "search",
                            placeholder: "Find in file",
                            value: "{query_raw}",
                            spellcheck: "false",
                            autocapitalize: "off",
                            autocomplete: "off",
                            oninput: move |e| blob_query.set(e.value()),
                        }
                        if !query_lc.is_empty() {
                            span { class: "muted small", "{match_count} match(es)" }
                        }
                    }
                }
                if is_binary {
                    p { class: "muted", "Binary file, content omitted." }
                } else {
                    div { class: "blob-viewer",
                        for l in lines {
                            {
                                let blame = blame_by_line.get(&l.number).cloned();
                                let is_match = !query_lc.is_empty()
                                    && l.text.to_lowercase().contains(&query_lc);
                                render_blob_line(l, blame, is_match)
                            }
                        }
                    }
                }
            }
        }
        Some(Ok(None)) | None => rsx! { p { class: "muted", "Loading…" } },
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! {
                p { class: "err",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
    }
}

fn render_blob_line(l: BlobLine, blame: Option<BlameLine>, is_match: bool) -> Element {
    let n = l.number;
    let tokens = l.tokens.clone();
    let plain = l.text.clone();
    let class = if is_match {
        "blob-line match"
    } else {
        "blob-line"
    };
    rsx! {
        div { class: "{class}",
            {render_blame_cell(blame)}
            span { class: "ln", "{n}" }
            span { class: "txt", {render_line_content(&tokens, &plain)} }
        }
    }
}

fn render_blame_cell(blame: Option<BlameLine>) -> Element {
    let Some(b) = blame else {
        return rsx! { span { class: "blame-col blame-empty" } };
    };
    // Uncommitted lines (e.g. unstaged worktree edits) come from
    // `git blame` with the all-zero oid as a sentinel.
    let uncommitted = b.oid.chars().all(|c| c == '0');
    if uncommitted {
        return rsx! {
            span { class: "blame-col blame-uncommitted",
                span { class: "short-oid", "·" }
                span { class: "author", "uncommitted" }
            }
        };
    }
    let when = format_time_relative(b.time_unix);
    let when_abs = format_time(b.time_unix);
    let title = format!("{}\n{}\n{}", b.summary, b.author_name, when_abs);
    rsx! {
        span { class: "blame-col", title: "{title}",
            span { class: "short-oid", "{b.short_oid}" }
            span { class: "author", "{b.author_name}" }
            span { class: "when", "{when}" }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_working_detail(
    state: &Option<Result<Option<FileDiff>, String>>,
    mut res: Resource<Result<Option<FileDiff>, String>>,
    mut res_for_restart: Resource<Result<Option<FileDiff>, String>>,
    file: &str,
    side_by_side: bool,
    current_repo: Signal<String>,
    mut hunk_picker: Signal<HashSet<usize>>,
    mut net_busy: Signal<bool>,
    mut net_result: Signal<Option<Result<gitrust_types::NetworkOpResult, String>>>,
) -> Element {
    match state {
        Some(Ok(Some(f))) => {
            let path = file.to_string();
            let path_for_action = path.clone();
            let kind = f.kind.clone();
            let f_clone = f.clone();
            // Show the staging toolbar only for `modified` files —
            // untracked / deleted / added files don't have a meaningful
            // hunk-level subset to pick from (the underlying `git apply`
            // refuses too).
            let stageable = kind == "modified" && !f.is_binary;
            let picked = hunk_picker.read().clone();
            let n_picked = picked.len();
            let busy = *net_busy.read();
            rsx! {
                div { class: "diff-header",
                    div { class: "title",
                        span { class: "kind kind-{kind}", "{kind}" }
                        " "
                        code { class: "full-oid", "{path}" }
                    }
                    div { class: "meta", "Working tree vs index" }
                    if stageable {
                        div { class: "hunk-stage-bar",
                            span { class: "muted small",
                                if n_picked == 0 {
                                    "Tick hunks to stage them piecemeal — or stage the whole file from the sidebar."
                                } else if n_picked == 1 {
                                    "1 hunk selected"
                                } else {
                                    "{n_picked} hunks selected"
                                }
                            }
                            button {
                                class: "stage-hunks-btn",
                                disabled: busy || n_picked == 0,
                                onclick: move |_| {
                                    let repo = current_repo.read().clone();
                                    let file = path_for_action.clone();
                                    let hunks: Vec<usize> = hunk_picker
                                        .read()
                                        .iter()
                                        .copied()
                                        .collect::<Vec<_>>();
                                    net_busy.set(true);
                                    net_result.set(None);
                                    spawn(async move {
                                        let r = crate::fetch::post_stage_hunks(
                                            &repo, &file, &hunks,
                                        )
                                        .await;
                                        net_busy.set(false);
                                        if let Err(e) = r {
                                            net_result.set(Some(Err(e)));
                                        } else {
                                            hunk_picker.set(HashSet::new());
                                        }
                                        res_for_restart.restart();
                                    });
                                },
                                if n_picked == 0 { "Stage hunks" } else { "Stage {n_picked} hunk(s)" }
                            }
                        }
                    }
                }
                {render_file_diff(f_clone, side_by_side, if stageable { Some(hunk_picker) } else { None })}
            }
        }
        Some(Ok(None)) | None => rsx! { p { class: "muted", "Loading…" } },
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! {
                p { class: "err",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
    }
}

fn render_commit_detail(
    state: &Option<Result<Option<CommitDiff>, String>>,
    mut res: Resource<Result<Option<CommitDiff>, String>>,
    side_by_side: bool,
) -> Element {
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
                    {render_file_diff(f, side_by_side, None)}
                }
            }
        }
        Some(Ok(None)) | None => rsx! { p { class: "muted", "Loading…" } },
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! {
                p { class: "err",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
    }
}
