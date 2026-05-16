//! Left-pane render functions: per-block count badges, branch / remote /
//! tag lists, the working-tree status list with click-to-select, and the
//! `Files at HEAD` tree.

use dioxus::prelude::*;
use gitrust_types::{BranchInfo, RemoteBranchInfo, StatusEntry, TagInfo, TreeEntry};

use crate::state::BlobSelection;

pub(crate) fn render_branch_count(state: &Option<Result<Vec<BranchInfo>, String>>) -> Element {
    if let Some(Ok(bs)) = state {
        let n = bs.len();
        rsx! { span { class: "count", "{n}" } }
    } else {
        rsx! {}
    }
}

pub(crate) fn render_tag_count(state: &Option<Result<Vec<TagInfo>, String>>) -> Element {
    if let Some(Ok(ts)) = state {
        let n = ts.len();
        rsx! { span { class: "count", "{n}" } }
    } else {
        rsx! {}
    }
}

pub(crate) fn render_remote_count(
    state: &Option<Result<Vec<RemoteBranchInfo>, String>>,
) -> Element {
    if let Some(Ok(rs)) = state {
        let n = rs.len();
        rsx! { span { class: "count", "{n}" } }
    } else {
        rsx! {}
    }
}

pub(crate) fn render_tree_count(state: &Option<Result<Vec<TreeEntry>, String>>) -> Element {
    if let Some(Ok(t)) = state {
        let n = count_blobs(t);
        rsx! { span { class: "count", "{n}" } }
    } else {
        rsx! {}
    }
}

fn count_blobs(entries: &[TreeEntry]) -> usize {
    entries
        .iter()
        .map(|e| {
            if e.kind == "tree" {
                count_blobs(&e.children)
            } else {
                1
            }
        })
        .sum()
}

pub(crate) fn render_tree(
    state: &Option<Result<Vec<TreeEntry>, String>>,
    selected_oid: Signal<Option<String>>,
    selected_file: Signal<Option<String>>,
    selected_blob: Signal<Option<BlobSelection>>,
) -> Element {
    match state {
        Some(Ok(entries)) if entries.is_empty() => {
            rsx! { p { class: "muted small", "Empty tree." } }
        }
        Some(Ok(entries)) => {
            let rows = entries.clone();
            rsx! {
                div { class: "file-tree",
                    for e in rows {
                        {render_tree_node(e, selected_oid, selected_file, selected_blob)}
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

fn render_tree_node(
    entry: TreeEntry,
    mut selected_oid: Signal<Option<String>>,
    mut selected_file: Signal<Option<String>>,
    mut selected_blob: Signal<Option<BlobSelection>>,
) -> Element {
    let name = entry.name.clone();
    let kind = entry.kind.clone();
    let path = entry.path.clone();
    if entry.kind == "tree" {
        let children = entry.children.clone();
        rsx! {
            details { class: "tree-folder",
                summary { class: "tree-row tree-folder-row",
                    span { class: "tree-glyph" }
                    span { class: "tree-name", "{name}" }
                }
                div { class: "tree-children",
                    for c in children {
                        {render_tree_node(c, selected_oid, selected_file, selected_blob)}
                    }
                }
            }
        }
    } else {
        let glyph = match kind.as_str() {
            "symlink" => "↗",
            "submodule" => "⊕",
            _ => "·",
        };
        let is_selected = selected_blob
            .read()
            .as_ref()
            .is_some_and(|s| s.path == path);
        let oid_for_click = entry.oid.clone();
        let path_for_click = entry.path.clone();
        rsx! {
            div {
                key: "{path}",
                class: if is_selected { "tree-row tree-blob-row tree-kind-{kind} selected" } else { "tree-row tree-blob-row tree-kind-{kind}" },
                title: "{path}",
                onclick: move |_| {
                    let same = selected_blob
                        .read()
                        .as_ref()
                        .is_some_and(|s| s.path == path_for_click);
                    if same {
                        selected_blob.set(None);
                    } else {
                        selected_blob.set(Some(BlobSelection {
                            oid: oid_for_click.clone(),
                            path: path_for_click.clone(),
                        }));
                        selected_oid.set(None);
                        selected_file.set(None);
                    }
                },
                span { class: "tree-glyph blob", "{glyph}" }
                span { class: "tree-name", "{name}" }
            }
        }
    }
}

pub(crate) fn render_status_count(state: &Option<Result<Vec<StatusEntry>, String>>) -> Element {
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

pub(crate) fn render_branches(state: &Option<Result<Vec<BranchInfo>, String>>) -> Element {
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

pub(crate) fn render_tags(state: &Option<Result<Vec<TagInfo>, String>>) -> Element {
    match state {
        Some(Ok(tags)) if tags.is_empty() => {
            rsx! { p { class: "muted small", "No tags." } }
        }
        Some(Ok(tags)) => {
            let rows = tags.clone();
            rsx! {
                ul { class: "branch-list",
                    for t in rows {
                        li { key: "{t.name}",
                            span {
                                class: if t.annotated { "marker tag annotated" } else { "marker tag" },
                                if t.annotated { "❖" } else { "◆" }
                            }
                            span { class: "name", "{t.name}" }
                            span { class: "oid",
                                {
                                    t.oid
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

pub(crate) fn render_remotes(state: &Option<Result<Vec<RemoteBranchInfo>, String>>) -> Element {
    match state {
        Some(Ok(rems)) if rems.is_empty() => {
            rsx! { p { class: "muted small", "No remote-tracking branches." } }
        }
        Some(Ok(rems)) => {
            let rows = rems.clone();
            rsx! {
                ul { class: "branch-list",
                    for r in rows {
                        li { key: "{r.name}",
                            span { class: "marker remote", "↗" }
                            span { class: "name", "{r.name}" }
                            span { class: "oid",
                                {
                                    r.oid
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

pub(crate) fn render_status(
    state: &Option<Result<Vec<StatusEntry>, String>>,
    mut selected_oid: Signal<Option<String>>,
    mut selected_file: Signal<Option<String>>,
    mut selected_blob: Signal<Option<BlobSelection>>,
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
                                            selected_blob.set(None);
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
