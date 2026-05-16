//! Right-pane render functions: summary card at the top, the commit
//! history table, and the unified detail panel that switches between
//! the blob viewer, the working-tree diff, and the commit diff based
//! on which signal is non-empty.

use dioxus::prelude::*;
use gitrust_types::{BlobLine, BlobView, CommitDiff, CommitInfo, FileDiff, RepoSummary};

use crate::diff::{render_file_diff, render_line_content};
use crate::state::BlobSelection;
use crate::time_fmt::{format_time, format_time_relative};

pub(crate) fn render_summary_card(state: &Option<Result<RepoSummary, String>>) -> Element {
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

pub(crate) fn render_log(
    state: &Option<Result<Vec<CommitInfo>, String>>,
    selected_oid: Signal<Option<String>>,
    selected_file: Signal<Option<String>>,
    selected_blob: Signal<Option<BlobSelection>>,
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
                            {render_commit_row(c, selected_oid, selected_file, selected_blob)}
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
    mut selected_blob: Signal<Option<BlobSelection>>,
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
                    selected_blob.set(None);
                }
            },
            td { class: "td-oid", code { "{c.short_oid}" } }
            td { class: "td-author", "{c.author_name}" }
            td { class: "td-msg", "{c.summary}" }
            td { class: "td-when", title: "{format_time(c.time_unix)}", "{format_time_relative(c.time_unix)}" }
        }
    }
}

pub(crate) fn render_detail(
    commit_state: &Option<Result<Option<CommitDiff>, String>>,
    working_state: &Option<Result<Option<FileDiff>, String>>,
    blob_state: &Option<Result<Option<BlobView>, String>>,
    selected_oid: Signal<Option<String>>,
    selected_file: Signal<Option<String>>,
    selected_blob: Signal<Option<BlobSelection>>,
    side_by_side: bool,
) -> Element {
    if selected_blob.read().is_some() {
        return render_blob_viewer(blob_state);
    }
    if let Some(file) = selected_file.read().clone() {
        return render_working_detail(working_state, &file, side_by_side);
    }
    if selected_oid.read().is_some() {
        return render_commit_detail(commit_state, side_by_side);
    }
    rsx! { p { class: "muted", "Select a commit, status entry, or file to inspect." } }
}

fn render_blob_viewer(state: &Option<Result<Option<BlobView>, String>>) -> Element {
    match state {
        Some(Ok(Some(b))) => {
            let path = b.path.clone();
            let oid = b.oid.clone();
            let size = b.size;
            let is_binary = b.is_binary;
            let line_count = b.lines.len();
            let lines = b.lines.clone();
            rsx! {
                div { class: "diff-header",
                    div { class: "title", code { class: "full-oid", "{path}" } }
                    div { class: "meta",
                        "{line_count} lines · {size} bytes · "
                        code { "{oid}" }
                    }
                }
                if is_binary {
                    p { class: "muted", "Binary file, content omitted." }
                } else {
                    div { class: "blob-viewer",
                        for l in lines {
                            {render_blob_line(l)}
                        }
                    }
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

fn render_blob_line(l: BlobLine) -> Element {
    let n = l.number;
    let tokens = l.tokens.clone();
    let plain = l.text.clone();
    rsx! {
        div { class: "blob-line",
            span { class: "ln", "{n}" }
            span { class: "txt", {render_line_content(&tokens, &plain)} }
        }
    }
}

fn render_working_detail(
    state: &Option<Result<Option<FileDiff>, String>>,
    file: &str,
    side_by_side: bool,
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
                {render_file_diff(f_clone, side_by_side)}
            }
        }
        Some(Ok(None)) | None => rsx! { p { class: "muted", "Loading…" } },
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! { p { class: "err", "Error: {msg}" } }
        }
    }
}

fn render_commit_detail(
    state: &Option<Result<Option<CommitDiff>, String>>,
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
                    {render_file_diff(f, side_by_side)}
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
