//! Left-pane render functions: per-block count badges, branch / remote /
//! tag lists, the working-tree status list with click-to-select, and the
//! `Files at HEAD` tree.

use dioxus::prelude::*;
use gitrust_types::{BranchInfo, RemoteBranchInfo, StashEntry, StatusEntry, TagInfo, TreeEntry};

use crate::state::BlobSelection;
use crate::time_fmt::format_time_relative;

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
    mut res: Resource<Result<Vec<TreeEntry>, String>>,
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
            rsx! {
                p { class: "err small",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
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

pub(crate) fn render_branches(
    state: &Option<Result<Vec<BranchInfo>, String>>,
    mut res: Resource<Result<Vec<BranchInfo>, String>>,
    current_repo: Signal<String>,
    mut new_branch: Signal<String>,
) -> Element {
    let list_body = match state {
        Some(Ok(branches)) if branches.is_empty() => {
            rsx! { p { class: "muted small", "No local branches." } }
        }
        Some(Ok(branches)) => {
            let rows = branches.clone();
            rsx! {
                ul { class: "branch-list",
                    for b in rows {
                        {
                            let name_for_switch = b.name.clone();
                            let name_for_rename = b.name.clone();
                            let name_for_delete = b.name.clone();
                            let is_head = b.is_head;
                            rsx! {
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
                                    button {
                                        class: "branch-act rename",
                                        title: "Rename this branch",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            rename_branch(name_for_rename.clone(), current_repo);
                                        },
                                        "✎"
                                    }
                                    if !is_head {
                                        button {
                                            class: "branch-act switch",
                                            title: "Check out this branch",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                checkout_branch(name_for_switch.clone(), current_repo);
                                            },
                                            "→"
                                        }
                                        button {
                                            class: "branch-act delete",
                                            title: "Delete this branch (with force-confirm if unmerged)",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                delete_branch(name_for_delete.clone(), current_repo);
                                            },
                                            "×"
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
            rsx! {
                p { class: "err small",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
        None => rsx! { p { class: "muted small", "Loading…" } },
    };

    let name_for_create = new_branch.read().clone();
    let can_create = !name_for_create.trim().is_empty();
    rsx! {
        {list_body}
        form {
            class: "new-branch",
            onsubmit: move |e| {
                e.prevent_default();
                let raw = new_branch.read().trim().to_string();
                if raw.is_empty() {
                    return;
                }
                create_branch(raw, current_repo);
                new_branch.set(String::new());
            },
            input {
                r#type: "text",
                placeholder: "new branch name",
                value: "{name_for_create}",
                spellcheck: "false",
                autocapitalize: "off",
                autocomplete: "off",
                oninput: move |e| new_branch.set(e.value()),
            }
            button {
                r#type: "submit",
                class: "new-branch-btn",
                disabled: !can_create,
                "Create"
            }
        }
    }
}

pub(crate) fn render_tags(
    state: &Option<Result<Vec<TagInfo>, String>>,
    mut res: Resource<Result<Vec<TagInfo>, String>>,
    mut tags_res: Resource<Result<Vec<TagInfo>, String>>,
    current_repo: Signal<String>,
    mut new_tag_name: Signal<String>,
) -> Element {
    let body = match state {
        Some(Ok(tags)) if tags.is_empty() => rsx! { p { class: "muted small", "No tags." } },
        Some(Ok(tags)) => {
            let rows = tags.clone();
            rsx! {
                ul { class: "branch-list",
                    for t in rows {
                        {
                            let name_for_del = t.name.clone();
                            rsx! {
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
                                    button {
                                        class: "row-action delete",
                                        title: "Delete this tag",
                                        onclick: move |_| {
                                            let name = name_for_del.clone();
                                            if !browser_confirm(&format!("Delete tag `{name}`?")) {
                                                return;
                                            }
                                            let path = current_repo.read().clone();
                                            spawn(async move {
                                                let _ = crate::fetch::post_tag_delete(&path, &name).await;
                                                tags_res.restart();
                                            });
                                        },
                                        "×"
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
            rsx! {
                p { class: "err small",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
        None => rsx! { p { class: "muted small", "Loading…" } },
    };
    let name_value = new_tag_name.read().clone();
    let can_create = !name_value.trim().is_empty();
    rsx! {
        {body}
        form {
            class: "branch-create",
            onsubmit: move |e| {
                e.prevent_default();
                let name = new_tag_name.read().trim().to_string();
                if name.is_empty() {
                    return;
                }
                let path = current_repo.read().clone();
                spawn(async move {
                    let _ = crate::fetch::post_tag_create(&path, &name, None, None).await;
                    tags_res.restart();
                    new_tag_name.set(String::new());
                });
            },
            input {
                r#type: "text",
                placeholder: "New tag at HEAD",
                value: "{name_value}",
                spellcheck: "false",
                autocapitalize: "off",
                autocomplete: "off",
                oninput: move |e| new_tag_name.set(e.value()),
            }
            button { r#type: "submit", disabled: !can_create, "Tag" }
        }
    }
}

pub(crate) fn render_remotes(
    state: &Option<Result<Vec<RemoteBranchInfo>, String>>,
    mut res: Resource<Result<Vec<RemoteBranchInfo>, String>>,
) -> Element {
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
            rsx! {
                p { class: "err small",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
        None => rsx! { p { class: "muted small", "Loading…" } },
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_status(
    state: &Option<Result<Vec<StatusEntry>, String>>,
    mut res: Resource<Result<Vec<StatusEntry>, String>>,
    mut selected_oid: Signal<Option<String>>,
    mut selected_file: Signal<Option<String>>,
    mut selected_blob: Signal<Option<BlobSelection>>,
    current_repo: Signal<String>,
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
                            let path_for_stage = e.path.clone();
                            let path_for_discard = e.path.clone();
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
                                    {render_status_path(&e)}
                                    button {
                                        class: "stage-btn discard",
                                        title: "Discard worktree changes to this file",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            discard_one(path_for_discard.clone(), current_repo);
                                        },
                                        "↺"
                                    }
                                    button {
                                        class: "stage-btn",
                                        title: "Stage this file",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            stage_one(path_for_stage.clone(), current_repo);
                                        },
                                        "+"
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
            rsx! {
                p { class: "err small",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
        None => rsx! { p { class: "muted small", "Loading…" } },
    }
}

pub(crate) fn render_staged_count(state: &Option<Result<Vec<StatusEntry>, String>>) -> Element {
    if let Some(Ok(s)) = state {
        let n = s.len();
        if n == 0 {
            rsx! {}
        } else {
            rsx! { span { class: "count", "{n}" } }
        }
    } else {
        rsx! {}
    }
}

pub(crate) fn render_staged(
    state: &Option<Result<Vec<StatusEntry>, String>>,
    mut res: Resource<Result<Vec<StatusEntry>, String>>,
    current_repo: Signal<String>,
    mut unstage_target: Signal<Option<String>>,
) -> Element {
    match state {
        Some(Ok(entries)) if entries.is_empty() => {
            rsx! { p { class: "muted small", "Nothing staged." } }
        }
        Some(Ok(entries)) => {
            let rows = entries.clone();
            rsx! {
                ul { class: "status-list",
                    for e in rows {
                        {
                            let p = e.path.clone();
                            let p_hunks = e.path.clone();
                            let modified = e.kind == "modified";
                            rsx! {
                                li { key: "{e.path}",
                                    span { class: "badge badge-{e.kind}", title: "{e.kind}",
                                        {status_glyph(&e.kind)}
                                    }
                                    {render_status_path(&e)}
                                    if modified {
                                        button {
                                            class: "stage-btn hunks",
                                            title: "Unstage individual hunks of this file",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                unstage_target.set(Some(p_hunks.clone()));
                                            },
                                            "⌥"
                                        }
                                    }
                                    button {
                                        class: "stage-btn unstage",
                                        title: "Unstage this file",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            unstage_one(p.clone(), current_repo);
                                        },
                                        "−"
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
            rsx! {
                p { class: "err small",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
        None => rsx! { p { class: "muted small", "Loading…" } },
    }
}

fn stage_one(file: String, current_repo: Signal<String>) {
    let path = current_repo.read().clone();
    spawn(async move {
        let _ = crate::fetch::post_stage(&path, &[file]).await;
    });
}

fn unstage_one(file: String, current_repo: Signal<String>) {
    let path = current_repo.read().clone();
    spawn(async move {
        let _ = crate::fetch::post_unstage(&path, &[file]).await;
    });
}

fn discard_one(file: String, current_repo: Signal<String>) {
    let path = current_repo.read().clone();
    spawn(async move {
        let _ = crate::fetch::post_discard(&path, &[file]).await;
    });
}

fn checkout_branch(name: String, current_repo: Signal<String>) {
    let path = current_repo.read().clone();
    spawn(async move {
        let _ = crate::fetch::post_checkout(&path, &name).await;
    });
}

fn delete_branch(name: String, current_repo: Signal<String>) {
    let path = current_repo.read().clone();
    spawn(async move {
        match crate::fetch::post_branch_delete(&path, &name, false).await {
            Ok(_) => {}
            Err(e) => {
                let lower = e.to_lowercase();
                // git's wording for "branch hasn't been merged into HEAD";
                // any other failure surfaces as a normal sidebar error row.
                if !(lower.contains("not fully merged")
                    || lower.contains("--force")
                    || lower.contains("'-d' to delete"))
                {
                    return;
                }
                let msg = format!(
                    "Branch `{name}` has commits not reachable from HEAD.\n\nForce delete?"
                );
                if browser_confirm(&msg) {
                    let _ = crate::fetch::post_branch_delete(&path, &name, true).await;
                }
            }
        }
    });
}

fn rename_branch(old: String, current_repo: Signal<String>) {
    let Some(new) = browser_prompt(&format!("Rename branch `{old}` to:"), &old) else {
        return;
    };
    let new = new.trim().to_string();
    if new.is_empty() || new == old {
        return;
    }
    let path = current_repo.read().clone();
    spawn(async move {
        let _ = crate::fetch::post_branch_rename(&path, &old, &new).await;
    });
}

fn create_branch(name: String, current_repo: Signal<String>) {
    let path = current_repo.read().clone();
    spawn(async move {
        // Create + switch in one step — matches `git checkout -b` semantics
        // and is the default expectation for "I made a new branch".
        let _ = crate::fetch::post_branch_create(&path, &name, true).await;
    });
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn browser_confirm(msg: &str) -> bool {
    gloo_utils::window()
        .confirm_with_message(msg)
        .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn browser_confirm(_msg: &str) -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn browser_prompt(msg: &str, default: &str) -> Option<String> {
    gloo_utils::window()
        .prompt_with_message_and_default(msg, default)
        .ok()
        .flatten()
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_prompt(_msg: &str, _default: &str) -> Option<String> {
    None
}

pub(crate) fn render_stash_count(state: &Option<Result<Vec<StashEntry>, String>>) -> Element {
    if let Some(Ok(s)) = state {
        let n = s.len();
        if n == 0 {
            rsx! {}
        } else {
            rsx! { span { class: "count", "{n}" } }
        }
    } else {
        rsx! {}
    }
}

pub(crate) fn render_stashes(
    state: &Option<Result<Vec<StashEntry>, String>>,
    mut res: Resource<Result<Vec<StashEntry>, String>>,
    current_repo: Signal<String>,
) -> Element {
    let body = match state {
        Some(Ok(stashes)) if stashes.is_empty() => {
            rsx! { p { class: "muted small", "No stashes." } }
        }
        Some(Ok(stashes)) => {
            let rows = stashes.clone();
            rsx! {
                ul { class: "stash-list",
                    for s in rows {
                        {
                            let idx = s.index;
                            let when = format_time_relative(s.time_unix);
                            let msg = s.message.clone();
                            let ref_for_title = s.ref_name.clone();
                            rsx! {
                                li { key: "{s.ref_name}",
                                    title: "{ref_for_title}",
                                    span { class: "name", "{msg}" }
                                    span { class: "when", "{when}" }
                                    button {
                                        class: "stash-act pop",
                                        title: "Pop — apply and drop this stash",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            pop_stash(idx, current_repo);
                                        },
                                        "↩"
                                    }
                                    button {
                                        class: "stash-act drop",
                                        title: "Drop — discard without applying",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            drop_stash(idx, current_repo);
                                        },
                                        "×"
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
            rsx! {
                p { class: "err small",
                    "Error: {msg} "
                    button { class: "retry-btn", onclick: move |_| res.restart(), "Retry" }
                }
            }
        }
        None => rsx! { p { class: "muted small", "Loading…" } },
    };
    rsx! {
        {body}
        button {
            class: "stash-save",
            title: "Save the current worktree as a new stash",
            onclick: move |_| save_stash(current_repo),
            "Stash worktree"
        }
    }
}

fn save_stash(current_repo: Signal<String>) {
    let path = current_repo.read().clone();
    // Pre-fill empty so the user gets a clean prompt; an empty submit
    // falls through to `git stash push` without -m.
    let message = browser_prompt("Stash message (optional):", "");
    spawn(async move {
        let _ = crate::fetch::post_stash_save(&path, message.as_deref()).await;
    });
}

fn pop_stash(index: usize, current_repo: Signal<String>) {
    let path = current_repo.read().clone();
    spawn(async move {
        let _ = crate::fetch::post_stash_pop(&path, index).await;
    });
}

fn drop_stash(index: usize, current_repo: Signal<String>) {
    if !browser_confirm("Drop this stash? It can't be undone.") {
        return;
    }
    let path = current_repo.read().clone();
    spawn(async move {
        let _ = crate::fetch::post_stash_drop(&path, index).await;
    });
}

/// Render the path cell for a status entry. Renames and copies get an
/// `old → new` treatment matching the diff viewer's file header.
fn render_status_path(e: &StatusEntry) -> Element {
    match e.old_path.as_deref() {
        Some(old) if old != e.path => {
            let old = old.to_string();
            let new = e.path.clone();
            rsx! {
                span { class: "path",
                    span { class: "old-path", "{old}" }
                    span { class: "rename-arrow", "→" }
                    "{new}"
                }
            }
        }
        _ => {
            let path = e.path.clone();
            rsx! { span { class: "path", "{path}" } }
        }
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
