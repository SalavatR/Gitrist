use dioxus::prelude::*;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use gitrust_types::{
    BlobLine, BlobView, BranchInfo, CommitDiff, CommitInfo, DiffHunk, DiffLine, FileDiff,
    RemoteBranchInfo, RepoSummary, StatusEntry, TagInfo, Token, TreeEntry,
};

#[derive(Clone, PartialEq, Debug)]
struct BlobSelection {
    oid: String,
    path: String,
}

const DEFAULT_REPO: &str = "/home/salavat/gitrust";
const LOG_LIMIT: usize = 50;
const STATUS_POLL_INTERVAL_MS: u32 = 2_000;
const REFS_POLL_INTERVAL_MS: u32 = 10_000;
#[cfg(target_arch = "wasm32")]
const REPO_STORAGE_KEY: &str = "gitrust.repo";
#[cfg(target_arch = "wasm32")]
const VIEW_MODE_STORAGE_KEY: &str = "gitrust.view_mode";

#[component]
pub fn App() -> Element {
    let initial = initial_repo();
    let mut current_repo = use_signal(|| initial.clone());
    let mut draft_repo = use_signal(|| initial.clone());
    let selected_oid = use_signal(|| None::<String>);
    let selected_file = use_signal(|| None::<String>);
    let selected_blob = use_signal(|| None::<BlobSelection>);
    let mut side_by_side = use_signal(initial_side_by_side);

    use_effect(move || {
        let path = current_repo.read().clone();
        persist_repo(&path);
    });
    use_effect(move || {
        let sbs = *side_by_side.read();
        persist_side_by_side(sbs);
    });

    let mut summary = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_summary(&path).await }
    });
    let mut log = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_log(&path, LOG_LIMIT).await }
    });
    let mut status = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_status(&path).await }
    });
    let branches = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_branches(&path).await }
    });
    let remotes = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_remotes(&path).await }
    });
    let tags = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_tags(&path).await }
    });
    let tree = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_tree(&path).await }
    });

    // Polling stays as a silent fallback — WS push is the primary path.
    use_future(move || async move {
        loop {
            sleep_ms(STATUS_POLL_INTERVAL_MS).await;
            status.restart();
        }
    });
    use_future(move || async move {
        loop {
            sleep_ms(REFS_POLL_INTERVAL_MS).await;
            summary.restart();
            log.restart();
        }
    });

    // Live WS push: `use_resource` is keyed on `current_repo` so swapping
    // the repo cancels the previous future and drops the socket cleanly.
    #[cfg(target_arch = "wasm32")]
    {
        let live = LiveResources {
            summary,
            log,
            status,
            branches,
            tags,
            remotes,
            tree,
        };
        let _ws_lifecycle = use_resource(move || {
            let path = current_repo.read().clone();
            async move { run_event_stream(path, live).await }
        });
    }
    let blob_view = use_resource(move || {
        let path = current_repo.read().clone();
        let sel = selected_blob.read().clone();
        async move {
            match sel {
                Some(b) => fetch_blob(&path, &b.oid, &b.path).await.map(Some),
                None => Ok::<_, String>(None),
            }
        }
    });
    let mut blob_view_stale = use_signal(|| None::<Result<Option<BlobView>, String>>);
    use_effect(move || {
        if let Some(v) = blob_view.read_unchecked().clone() {
            blob_view_stale.set(Some(v));
        }
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
    let mut diff_stale = use_signal(|| None::<Result<Option<CommitDiff>, String>>);
    use_effect(move || {
        if let Some(v) = diff.read_unchecked().clone() {
            diff_stale.set(Some(v));
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
    let mut working_diff_stale = use_signal(|| None::<Result<Option<FileDiff>, String>>);
    use_effect(move || {
        if let Some(v) = working_diff.read_unchecked().clone() {
            working_diff_stale.set(Some(v));
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
                            span { "Remotes" }
                            {render_remote_count(&remotes.read_unchecked())}
                        }
                        {render_remotes(&remotes.read_unchecked())}
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Tags" }
                            {render_tag_count(&tags.read_unchecked())}
                        }
                        {render_tags(&tags.read_unchecked())}
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Working tree" }
                            {render_status_count(&status.read_unchecked())}
                        }
                        {render_status(&status.read_unchecked(), selected_oid, selected_file, selected_blob)}
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Files at HEAD" }
                            {render_tree_count(&tree.read_unchecked())}
                        }
                        {render_tree(
                            &tree.read_unchecked(),
                            selected_oid,
                            selected_file,
                            selected_blob,
                        )}
                    }
                }

                main { class: "main",
                    {render_summary_card(&summary.read_unchecked())}

                    section { class: "main-block",
                        h2 { "History" }
                        {render_log(&log.read_unchecked(), selected_oid, selected_file, selected_blob)}
                    }

                    section { class: "main-block",
                        div { class: "block-toolbar",
                            h2 { style: "margin: 0;",
                                if selected_blob.read().is_some() { "File viewer" }
                                else if selected_file.read().is_some() { "Working tree change" }
                                else { "Commit detail" }
                            }
                            if selected_blob.read().is_none() {
                                button {
                                    class: "view-toggle",
                                    onclick: move |_| {
                                        let cur = *side_by_side.read();
                                        side_by_side.set(!cur);
                                    },
                                    if *side_by_side.read() { "Unified" } else { "Side-by-side" }
                                }
                            }
                        }
                        {render_detail(
                            &diff_stale.read_unchecked(),
                            &working_diff_stale.read_unchecked(),
                            &blob_view_stale.read_unchecked(),
                            selected_oid,
                            selected_file,
                            selected_blob,
                            *side_by_side.read(),
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

fn render_tag_count(state: &Option<Result<Vec<TagInfo>, String>>) -> Element {
    if let Some(Ok(ts)) = state {
        let n = ts.len();
        rsx! { span { class: "count", "{n}" } }
    } else {
        rsx! {}
    }
}

fn render_remote_count(state: &Option<Result<Vec<RemoteBranchInfo>, String>>) -> Element {
    if let Some(Ok(rs)) = state {
        let n = rs.len();
        rsx! { span { class: "count", "{n}" } }
    } else {
        rsx! {}
    }
}

fn render_tree_count(state: &Option<Result<Vec<TreeEntry>, String>>) -> Element {
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

fn render_tree(
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

fn render_tags(state: &Option<Result<Vec<TagInfo>, String>>) -> Element {
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

fn render_remotes(state: &Option<Result<Vec<RemoteBranchInfo>, String>>) -> Element {
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

fn render_status(
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

fn render_log(
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

fn render_detail(
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

/// Files with more than this many diff lines start collapsed. Tunable.
const AUTO_COLLAPSE_LINES: usize = 300;

fn render_file_diff(f: FileDiff, side_by_side: bool) -> Element {
    let path = f.path.clone();
    let old_path = f.old_path.clone();
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
                if let Some(op) = old_path {
                    code { class: "path old-path", "{op}" }
                    span { class: "rename-arrow", "→" }
                }
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
                    {if side_by_side { render_hunk_sbs(h) } else { render_hunk(h) }}
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

fn render_hunk_sbs(h: DiffHunk) -> Element {
    let header = format!(
        "@@ -{},{} +{},{} @@",
        h.old_start, h.old_count, h.new_start, h.new_count
    );
    let rows = pair_sbs_rows(h.lines);
    rsx! {
        div { class: "hunk",
            div { class: "hunk-header", "{header}" }
            div { class: "hunk-sbs",
                for row in rows {
                    {render_sbs_row(row)}
                }
            }
        }
    }
}

#[derive(Clone)]
enum SbsRow {
    Ctx(DiffLine),
    Pair(DiffLine, DiffLine),
    OnlyDel(DiffLine),
    OnlyAdd(DiffLine),
}

fn pair_sbs_rows(lines: Vec<DiffLine>) -> Vec<SbsRow> {
    let mut out = Vec::with_capacity(lines.len());
    let mut dels: Vec<DiffLine> = Vec::new();
    let mut adds: Vec<DiffLine> = Vec::new();

    let flush = |out: &mut Vec<SbsRow>, dels: &mut Vec<DiffLine>, adds: &mut Vec<DiffLine>| {
        let dv = std::mem::take(dels);
        let av = std::mem::take(adds);
        let pairs = dv.len().min(av.len());
        let mut di = dv.into_iter();
        let mut ai = av.into_iter();
        for _ in 0..pairs {
            out.push(SbsRow::Pair(di.next().unwrap(), ai.next().unwrap()));
        }
        for d in di {
            out.push(SbsRow::OnlyDel(d));
        }
        for a in ai {
            out.push(SbsRow::OnlyAdd(a));
        }
    };

    for line in lines {
        match line.kind.as_str() {
            "ctx" => {
                flush(&mut out, &mut dels, &mut adds);
                out.push(SbsRow::Ctx(line));
            }
            "del" => {
                if !adds.is_empty() {
                    flush(&mut out, &mut dels, &mut adds);
                }
                dels.push(line);
            }
            "add" => adds.push(line),
            _ => {}
        }
    }
    flush(&mut out, &mut dels, &mut adds);
    out
}

fn render_sbs_row(row: SbsRow) -> Element {
    match row {
        SbsRow::Ctx(l) => {
            let old_n = l.old_line.map(|n| n.to_string()).unwrap_or_default();
            let new_n = l.new_line.map(|n| n.to_string()).unwrap_or_default();
            let tokens = l.tokens.clone();
            let plain = l.text.clone();
            rsx! {
                div { class: "sbs-row sbs-ctx",
                    span { class: "ln", "{old_n}" }
                    span { class: "txt", {render_line_content(&tokens, &plain)} }
                    span { class: "ln", "{new_n}" }
                    span { class: "txt", {render_line_content(&tokens, &plain)} }
                }
            }
        }
        SbsRow::Pair(d, a) => {
            let old_n = d.old_line.map(|n| n.to_string()).unwrap_or_default();
            let new_n = a.new_line.map(|n| n.to_string()).unwrap_or_default();
            let d_tokens = d.tokens.clone();
            let a_tokens = a.tokens.clone();
            let d_plain = d.text.clone();
            let a_plain = a.text.clone();
            rsx! {
                div { class: "sbs-row sbs-mod",
                    span { class: "ln ln-del", "{old_n}" }
                    span { class: "txt txt-del", {render_line_content(&d_tokens, &d_plain)} }
                    span { class: "ln ln-add", "{new_n}" }
                    span { class: "txt txt-add", {render_line_content(&a_tokens, &a_plain)} }
                }
            }
        }
        SbsRow::OnlyDel(d) => {
            let old_n = d.old_line.map(|n| n.to_string()).unwrap_or_default();
            let tokens = d.tokens.clone();
            let plain = d.text.clone();
            rsx! {
                div { class: "sbs-row sbs-del",
                    span { class: "ln ln-del", "{old_n}" }
                    span { class: "txt txt-del", {render_line_content(&tokens, &plain)} }
                    span { class: "ln empty", "" }
                    span { class: "txt empty", "" }
                }
            }
        }
        SbsRow::OnlyAdd(a) => {
            let new_n = a.new_line.map(|n| n.to_string()).unwrap_or_default();
            let tokens = a.tokens.clone();
            let plain = a.text.clone();
            rsx! {
                div { class: "sbs-row sbs-add",
                    span { class: "ln empty", "" }
                    span { class: "txt empty", "" }
                    span { class: "ln ln-add", "{new_n}" }
                    span { class: "txt txt-add", {render_line_content(&tokens, &plain)} }
                }
            }
        }
    }
}

fn render_line_content(tokens: &Option<Vec<Token>>, plain: &str) -> Element {
    match tokens {
        Some(toks) if !toks.is_empty() => {
            let toks = toks.clone();
            rsx! {
                for t in toks {
                    span { class: "tok tok-{token_class_to_css(&t.class)}", "{t.text}" }
                }
            }
        }
        _ => {
            let plain_owned = plain.to_string();
            rsx! { "{plain_owned}" }
        }
    }
}

fn render_diff_line(l: DiffLine) -> Element {
    let kind = l.kind.clone();
    let old = l.old_line.map(|n| n.to_string()).unwrap_or_default();
    let new = l.new_line.map(|n| n.to_string()).unwrap_or_default();
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
    if let Ok(hash) = window.location().hash()
        && hash.len() > 1
        && let Ok(decoded) = urlencoding::decode(&hash[1..])
    {
        let s = decoded.into_owned();
        if !s.is_empty() {
            return s;
        }
    }
    if let Ok(stored) = gloo_storage::LocalStorage::get::<String>(REPO_STORAGE_KEY)
        && !stored.is_empty()
    {
        return stored;
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

#[cfg(target_arch = "wasm32")]
fn initial_side_by_side() -> bool {
    use gloo_storage::Storage;
    gloo_storage::LocalStorage::get::<String>(VIEW_MODE_STORAGE_KEY)
        .ok()
        .map(|s| s == "side")
        .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn initial_side_by_side() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn persist_side_by_side(side_by_side: bool) {
    use gloo_storage::Storage;
    let val = if side_by_side { "side" } else { "unified" };
    let _ = gloo_storage::LocalStorage::set(VIEW_MODE_STORAGE_KEY, val);
}

#[cfg(not(target_arch = "wasm32"))]
fn persist_side_by_side(_side_by_side: bool) {}

fn format_time(unix: i64) -> String {
    OffsetDateTime::from_unix_timestamp(unix)
        .ok()
        .and_then(|dt| dt.format(&Rfc3339).ok())
        .unwrap_or_else(|| unix.to_string())
}

fn format_time_relative(unix: i64) -> String {
    let now = now_unix();
    let delta = now - unix;
    if delta < 0 {
        return "in the future".to_string();
    }
    if delta < 60 {
        return "just now".to_string();
    }
    if delta < 3600 {
        return format!("{}m ago", delta / 60);
    }
    if delta < 86400 {
        return format!("{}h ago", delta / 3600);
    }
    let days = delta / 86400;
    if days < 7 {
        return format!("{}d ago", days);
    }
    if days < 30 {
        return format!("{}w ago", days / 7);
    }
    if days < 365 {
        return format!("{}mo ago", days / 30);
    }
    format!("{}y ago", days / 365)
}

#[cfg(target_arch = "wasm32")]
fn now_unix() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}

#[cfg(not(target_arch = "wasm32"))]
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
async fn sleep_ms(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep_ms(_ms: u32) {
    // No-op on native — auto-refresh only matters in the browser.
    std::future::pending::<()>().await
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
async fn fetch_tags(path: &str) -> Result<Vec<TagInfo>, String> {
    fetch_json(&format!("/api/repo/tags?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_remotes(path: &str) -> Result<Vec<RemoteBranchInfo>, String> {
    fetch_json(&format!("/api/repo/remotes?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_tree(path: &str) -> Result<Vec<TreeEntry>, String> {
    fetch_json(&format!("/api/repo/tree?path={path}")).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_blob(path: &str, oid: &str, file: &str) -> Result<BlobView, String> {
    fetch_json(&format!("/api/repo/blob?path={path}&oid={oid}&file={file}")).await
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
async fn fetch_tags(_path: &str) -> Result<Vec<TagInfo>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_remotes(_path: &str) -> Result<Vec<RemoteBranchInfo>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_tree(_path: &str) -> Result<Vec<TreeEntry>, String> {
    Err("native build: fetching not implemented".into())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_blob(_path: &str, _oid: &str, _file: &str) -> Result<BlobView, String> {
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

#[cfg(target_arch = "wasm32")]
#[derive(Copy, Clone)]
struct LiveResources {
    summary: Resource<Result<RepoSummary, String>>,
    log: Resource<Result<Vec<CommitInfo>, String>>,
    status: Resource<Result<Vec<StatusEntry>, String>>,
    branches: Resource<Result<Vec<BranchInfo>, String>>,
    tags: Resource<Result<Vec<TagInfo>, String>>,
    remotes: Resource<Result<Vec<RemoteBranchInfo>, String>>,
    tree: Resource<Result<Vec<TreeEntry>, String>>,
}

#[cfg(target_arch = "wasm32")]
impl LiveResources {
    fn dispatch(mut self, kind: &str) {
        match kind {
            "head_changed" => {
                self.summary.restart();
                self.log.restart();
                self.status.restart();
                self.branches.restart();
                self.tags.restart();
                self.remotes.restart();
                self.tree.restart();
            }
            "refs_changed" => {
                self.summary.restart();
                self.log.restart();
                self.branches.restart();
                self.tags.restart();
                self.remotes.restart();
                self.tree.restart();
            }
            "index_changed" | "worktree_changed" => {
                self.status.restart();
            }
            _ => {}
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize)]
struct EventMsg {
    kind: String,
}

#[cfg(target_arch = "wasm32")]
async fn run_event_stream(path: String, live: LiveResources) {
    use futures::StreamExt;
    use gloo_net::websocket::{Message, futures::WebSocket};

    if path.is_empty() {
        return;
    }
    let url = format!(
        "{}/api/repo/events?path={}",
        ws_origin(),
        urlencoding::encode(&path),
    );

    let mut backoff_ms: u32 = 500;
    loop {
        let ws = match WebSocket::open(&url) {
            Ok(w) => w,
            Err(_) => {
                sleep_ms(backoff_ms).await;
                backoff_ms = backoff_ms.saturating_mul(2).min(30_000);
                continue;
            }
        };
        backoff_ms = 500;
        let (_write, mut read) = ws.split();
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(t)) => {
                    if let Ok(e) = serde_json::from_str::<EventMsg>(&t) {
                        live.dispatch(&e.kind);
                    }
                }
                Ok(Message::Bytes(_)) => {}
                Err(_) => break,
            }
        }
        sleep_ms(backoff_ms).await;
        backoff_ms = backoff_ms.saturating_mul(2).min(30_000);
    }
}

#[cfg(target_arch = "wasm32")]
fn ws_origin() -> String {
    let window = gloo_utils::window();
    let loc = window.location();
    let proto = loc.protocol().unwrap_or_else(|_| "http:".into());
    let host = loc.host().unwrap_or_else(|_| "localhost:3737".into());
    let ws_proto = if proto == "https:" { "wss:" } else { "ws:" };
    format!("{ws_proto}//{host}")
}
