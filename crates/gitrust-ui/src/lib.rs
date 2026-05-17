//! Dioxus UI for gitrust. Mounted by `gitrust-web` in the browser; also
//! compiles on native targets (with stub fetchers) so `cargo check` is
//! cheap for non-WASM contributors.

use dioxus::prelude::*;
use gitrust_types::{BlameView, BlobView, CommitDiff, FileDiff, NetworkOpResult};

mod diff;
mod fetch;
mod graph;
mod main_panel;
mod sidebar;
mod state;
mod time_fmt;
#[cfg(target_arch = "wasm32")]
mod ws;

use fetch::{
    fetch_blame, fetch_blob, fetch_branches, fetch_diff, fetch_diff_working, fetch_log,
    fetch_remotes, fetch_repos, fetch_staged, fetch_stashes, fetch_state, fetch_status,
    fetch_summary, fetch_tags, fetch_tree,
};
use main_panel::{render_commit_form, render_detail, render_log, render_summary_card};
use sidebar::{
    render_branch_count, render_branches, render_remote_count, render_remotes, render_staged,
    render_staged_count, render_stash_count, render_stashes, render_status, render_status_count,
    render_tag_count, render_tags, render_tree, render_tree_count,
};
use state::{
    AUTH_TOKEN, BlobSelection, LOG_LIMIT, REFS_POLL_INTERVAL_MS, STATUS_POLL_INTERVAL_MS,
    ThemeMode, apply_theme, clear_auth_token, initial_log_all, initial_repo, initial_side_by_side,
    initial_theme, persist_auth_token, persist_log_all, persist_repo, persist_side_by_side,
    recent_repos, record_recent_repo,
};
use time_fmt::sleep_ms;

/// Top-level component. Splits between the auth gate (when no token
/// is loaded) and the real `AppContent`. The gate hides itself once
/// `AUTH_TOKEN` becomes `Some` — at which point `AppContent` mounts
/// fresh, every `use_resource` boots with the token already in place,
/// and the WS handshake includes it as a query parameter.
#[component]
pub fn App() -> Element {
    let signed_in = AUTH_TOKEN.read().is_some();
    if signed_in {
        rsx! { AppContent {} }
    } else {
        rsx! { AuthGate {} }
    }
}

#[component]
fn AuthGate() -> Element {
    let mut value = use_signal(String::new);
    rsx! {
        style { {include_str!("../style.css")} }
        div { class: "auth-gate",
            div { class: "auth-card",
                div { class: "auth-brand",
                    span { class: "logo-mark", "g" }
                    span { class: "logo-name", "gitrust" }
                }
                p { class: "muted",
                    "Paste the access token printed in the terminal:"
                }
                form {
                    class: "auth-form",
                    onsubmit: move |e| {
                        e.prevent_default();
                        let t = value.read().trim().to_string();
                        if !t.is_empty() {
                            persist_auth_token(&t);
                            *AUTH_TOKEN.write() = Some(t);
                        }
                    },
                    input {
                        r#type: "password",
                        placeholder: "64 hex chars",
                        value: "{value}",
                        autofocus: true,
                        spellcheck: "false",
                        autocomplete: "off",
                        oninput: move |e| value.set(e.value()),
                    }
                    button { r#type: "submit", "Sign in" }
                }
            }
        }
    }
}

#[component]
fn AppContent() -> Element {
    let initial = initial_repo();
    let mut current_repo = use_signal(|| initial.clone());
    let mut draft_repo = use_signal(|| initial.clone());
    let selected_oid = use_signal(|| None::<String>);
    let selected_file = use_signal(|| None::<String>);
    let selected_blob = use_signal(|| None::<BlobSelection>);
    let mut side_by_side = use_signal(initial_side_by_side);
    let mut recent = use_signal(recent_repos);
    let mut theme = use_signal(initial_theme);
    let commit_msg = use_signal(String::new);
    let commit_err = use_signal(|| None::<String>);
    let commit_author = use_signal(String::new);
    let new_branch = use_signal(String::new);
    let mut log_query = use_signal(String::new);
    let mut log_all = use_signal(initial_log_all);
    let blob_query = use_signal(String::new);
    let mut net_busy = use_signal(|| false);
    let mut net_result = use_signal(|| None::<Result<NetworkOpResult, String>>);
    let mut reset_mode = use_signal(|| "mixed".to_string());
    let mut hunk_picker = use_signal(std::collections::HashSet::<usize>::new);

    use_effect(move || persist_log_all(*log_all.read()));
    // Clear the hunk selection whenever the user switches to a different
    // file — otherwise indices saved for file A would be re-applied as
    // checkbox state on file B's unrelated hunks.
    use_effect(move || {
        let _ = selected_file.read();
        hunk_picker.set(std::collections::HashSet::new());
    });

    use_effect(move || {
        let path = current_repo.read().clone();
        persist_repo(&path);
        record_recent_repo(&path);
        recent.set(recent_repos());
    });
    use_effect(move || {
        let sbs = *side_by_side.read();
        persist_side_by_side(sbs);
    });
    use_effect(move || {
        let t = *theme.read();
        apply_theme(t);
    });

    let mut summary = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_summary(&path).await }
    });
    let mut log = use_resource(move || {
        let path = current_repo.read().clone();
        let q = log_query.read().clone();
        let all = *log_all.read();
        async move { fetch_log(&path, LOG_LIMIT, &q, all).await }
    });
    let mut status = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_status(&path).await }
    });
    let staged = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_staged(&path).await }
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
    let stashes = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_stashes(&path).await }
    });
    let mut state = use_resource(move || {
        let path = current_repo.read().clone();
        async move { fetch_state(&path).await }
    });
    // Workspaces (multi-repo browser). Empty list = no `--root` set on
    // the server → we hide the sidebar block entirely. Doesn't depend
    // on `current_repo`, so it only fetches once per session unless
    // the user signs out and back in.
    let repos = use_resource(|| async move { fetch_repos().await });

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
        let live = ws::LiveResources {
            summary,
            log,
            status,
            staged,
            branches,
            tags,
            remotes,
            tree,
            stashes,
        };
        let _ws_lifecycle = use_resource(move || {
            let path = current_repo.read().clone();
            async move { ws::run_event_stream(path, live).await }
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

    let blame_view = use_resource(move || {
        let path = current_repo.read().clone();
        let sel = selected_blob.read().clone();
        async move {
            match sel {
                Some(b) => fetch_blame(&path, &b.path).await.map(Some),
                None => Ok::<_, String>(None),
            }
        }
    });
    let mut blame_view_stale = use_signal(|| None::<Result<Option<BlameView>, String>>);
    use_effect(move || {
        if let Some(v) = blame_view.read_unchecked().clone() {
            blame_view_stale.set(Some(v));
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

    let repo_q = urlencoding::encode(&current_repo.read()).into_owned();

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
                        list: "recent-repos",
                        oninput: move |e| draft_repo.set(e.value()),
                    }
                    datalist { id: "recent-repos",
                        {
                            // Always seed with the current repo so the
                            // dropdown is useful on first-mount before
                            // the persist effect has had a chance to
                            // write to localStorage and bump `recent`.
                            let cur = current_repo.read().clone();
                            let stored: Vec<String> = recent.read().iter().cloned().collect();
                            let mut out: Vec<String> = Vec::with_capacity(stored.len() + 1);
                            if !cur.is_empty() {
                                out.push(cur.clone());
                            }
                            for p in stored {
                                if !out.contains(&p) {
                                    out.push(p);
                                }
                            }
                            rsx! {
                                for r in out {
                                    option { key: "{r}", value: "{r}" }
                                }
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "browse-btn",
                        title: "Open the native folder picker — needed on macOS to grant access to a repo under a TCC-protected directory like ~/Documents.",
                        onclick: move |_| {
                            spawn(async move {
                                if let Ok(Some(picked)) = fetch::post_pick_folder().await {
                                    draft_repo.set(picked.clone());
                                    current_repo.set(picked);
                                }
                            });
                        },
                        "Browse…"
                    }
                    button { r#type: "submit", "Load" }
                }
                {
                    // Three POST handlers share the same shape — capture
                    // path, flip net_busy, post, store the result. WS
                    // push handles UI-resource refresh on success.
                    let busy = *net_busy.read();
                    rsx! {
                        div { class: "net-ops",
                            button {
                                class: "net-btn",
                                disabled: busy,
                                title: "git fetch (default remote)",
                                onclick: move |_| {
                                    let path = current_repo.read().clone();
                                    net_busy.set(true);
                                    net_result.set(None);
                                    spawn(async move {
                                        let r = fetch::post_fetch(&path, None).await;
                                        net_busy.set(false);
                                        net_result.set(Some(r));
                                    });
                                },
                                "Fetch"
                            }
                            button {
                                class: "net-btn",
                                disabled: busy,
                                title: "git pull --ff-only",
                                onclick: move |_| {
                                    let path = current_repo.read().clone();
                                    net_busy.set(true);
                                    net_result.set(None);
                                    spawn(async move {
                                        let r = fetch::post_pull(&path, None, true).await;
                                        net_busy.set(false);
                                        net_result.set(Some(r));
                                    });
                                },
                                "Pull"
                            }
                            button {
                                class: "net-btn",
                                disabled: busy,
                                title: "git push (current branch to its upstream)",
                                onclick: move |_| {
                                    let path = current_repo.read().clone();
                                    net_busy.set(true);
                                    net_result.set(None);
                                    spawn(async move {
                                        let r = fetch::post_push(
                                            &path, None, None, false, false,
                                        ).await;
                                        net_busy.set(false);
                                        net_result.set(Some(r));
                                    });
                                },
                                "Push"
                            }
                        }
                    }
                }
                button {
                    class: "theme-toggle",
                    title: "Theme — click to cycle",
                    onclick: move |_| {
                        let next: ThemeMode = theme.read().next();
                        theme.set(next);
                    },
                    {theme.read().label()}
                }
                button {
                    class: "sign-out",
                    title: "Forget the access token; back to the sign-in screen",
                    onclick: move |_| {
                        clear_auth_token();
                        *AUTH_TOKEN.write() = None;
                    },
                    "Sign out"
                }
            }

            {
                // In-progress merge / cherry-pick banner with Abort /
                // Continue affordances. Visible only when the worktree
                // is mid-op; clean state renders nothing.
                let s = state.read_unchecked().clone();
                let busy = *net_busy.read();
                match s.as_ref().and_then(|r| r.as_ref().ok()) {
                    Some(rs) if rs.kind != "clean" => {
                        let kind = rs.kind.clone();
                        let subject = rs.subject.clone().unwrap_or_else(|| kind.clone());
                        let n = rs.conflicted.len();
                        let kind_for_abort = kind.clone();
                        let kind_for_continue = kind.clone();
                        let kind_for_skip = kind.clone();
                        let supports_skip = kind == "rebasing" || kind == "reverting";
                        rsx! {
                            div { class: "merge-banner",
                                div { class: "merge-banner-head",
                                    strong { "{kind}" }
                                    " · "
                                    span { "{subject}" }
                                    span { class: "muted small",
                                        if n == 0 {
                                            " · all conflicts resolved"
                                        } else if n == 1 {
                                            " · 1 conflict"
                                        } else {
                                            " · {n} conflicts"
                                        }
                                    }
                                }
                                div { class: "merge-banner-actions",
                                    button {
                                        class: "merge-action danger",
                                        disabled: busy,
                                        onclick: move |_| {
                                            let path = current_repo.read().clone();
                                            let kind = kind_for_abort.clone();
                                            net_busy.set(true);
                                            net_result.set(None);
                                            spawn(async move {
                                                let r = match kind.as_str() {
                                                    "merging" => fetch::post_merge_abort(&path).await,
                                                    "rebasing" => fetch::post_rebase_abort(&path).await,
                                                    "reverting" => fetch::post_revert_abort(&path).await,
                                                    _ => fetch::post_cherry_pick_abort(&path).await,
                                                };
                                                net_busy.set(false);
                                                match r {
                                                    Ok(()) => net_result.set(None),
                                                    Err(e) => net_result.set(Some(Err(e))),
                                                }
                                                state.restart();
                                            });
                                        },
                                        "Abort"
                                    }
                                    if supports_skip {
                                        button {
                                            class: "merge-action",
                                            disabled: busy,
                                            title: "Drop the commit currently being applied and continue with the next one",
                                            onclick: move |_| {
                                                let path = current_repo.read().clone();
                                                let kind = kind_for_skip.clone();
                                                net_busy.set(true);
                                                net_result.set(None);
                                                spawn(async move {
                                                    let r = match kind.as_str() {
                                                        "rebasing" => fetch::post_rebase_skip(&path).await,
                                                        _ => fetch::post_revert_skip(&path).await,
                                                    };
                                                    net_busy.set(false);
                                                    match r {
                                                        Ok(()) => net_result.set(None),
                                                        Err(e) => net_result.set(Some(Err(e))),
                                                    }
                                                    state.restart();
                                                });
                                            },
                                            "Skip"
                                        }
                                    }
                                    button {
                                        class: "merge-action primary",
                                        disabled: busy || n > 0,
                                        title: if n > 0 {
                                            "Resolve all conflicts first"
                                        } else {
                                            "Finalize with the staged commit message"
                                        },
                                        onclick: move |_| {
                                            let path = current_repo.read().clone();
                                            let kind = kind_for_continue.clone();
                                            net_busy.set(true);
                                            net_result.set(None);
                                            spawn(async move {
                                                let r = match kind.as_str() {
                                                    "merging" => fetch::post_merge_continue(&path).await,
                                                    "rebasing" => fetch::post_rebase_continue(&path).await,
                                                    "reverting" => fetch::post_revert_continue(&path).await,
                                                    _ => fetch::post_cherry_pick_continue(&path).await,
                                                };
                                                net_busy.set(false);
                                                match r {
                                                    Ok(()) => net_result.set(None),
                                                    Err(e) => net_result.set(Some(Err(e))),
                                                }
                                                state.restart();
                                            });
                                        },
                                        "Continue"
                                    }
                                }
                                if !rs.conflicted.is_empty() {
                                    ul { class: "merge-conflicts",
                                        for f in rs.conflicted.iter().cloned() {
                                            li { key: "{f}",
                                                code { "{f}" }
                                                {
                                                    let f_ours = f.clone();
                                                    let f_theirs = f.clone();
                                                    rsx! {
                                                        button {
                                                            class: "merge-resolve",
                                                            disabled: busy,
                                                            title: "Keep HEAD's version of this file and stage it",
                                                            onclick: move |_| {
                                                                let path = current_repo.read().clone();
                                                                let file = f_ours.clone();
                                                                net_busy.set(true);
                                                                spawn(async move {
                                                                    let r = fetch::post_resolve(&path, &file, "ours").await;
                                                                    net_busy.set(false);
                                                                    if let Err(e) = r {
                                                                        net_result.set(Some(Err(e)));
                                                                    }
                                                                    state.restart();
                                                                });
                                                            },
                                                            "Use ours"
                                                        }
                                                        button {
                                                            class: "merge-resolve",
                                                            disabled: busy,
                                                            title: "Take the incoming version of this file and stage it",
                                                            onclick: move |_| {
                                                                let path = current_repo.read().clone();
                                                                let file = f_theirs.clone();
                                                                net_busy.set(true);
                                                                spawn(async move {
                                                                    let r = fetch::post_resolve(&path, &file, "theirs").await;
                                                                    net_busy.set(false);
                                                                    if let Err(e) = r {
                                                                        net_result.set(Some(Err(e)));
                                                                    }
                                                                    state.restart();
                                                                });
                                                            },
                                                            "Use theirs"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => rsx! {},
                }
            }

            {
                // Result of the last network op (fetch/pull/push). Plain
                // <pre> so the user sees the same multi-line stderr they'd
                // see in a terminal. A button on the right dismisses it.
                let result = net_result.read().clone();
                let busy = *net_busy.read();
                match (busy, result) {
                    (true, _) => rsx! {
                        div { class: "net-banner muted",
                            "Working…"
                        }
                    },
                    (false, Some(Ok(r))) => rsx! {
                        div { class: "net-banner ok",
                            div { class: "net-banner-head",
                                strong { "{r.op}" }
                                if !r.remote.is_empty() {
                                    " · "
                                    code { "{r.remote}" }
                                }
                                button {
                                    class: "net-banner-dismiss",
                                    onclick: move |_| net_result.set(None),
                                    "×"
                                }
                            }
                            pre { class: "net-banner-body", "{r.summary}" }
                        }
                    },
                    (false, Some(Err(e))) => rsx! {
                        div { class: "net-banner err",
                            div { class: "net-banner-head",
                                strong { "failed" }
                                button {
                                    class: "net-banner-dismiss",
                                    onclick: move |_| net_result.set(None),
                                    "×"
                                }
                            }
                            pre { class: "net-banner-body", "{e}" }
                        }
                    },
                    (false, None) => rsx! {},
                }
            }

            div { class: "split",
                aside { class: "sidebar",
                    {
                        // Workspaces — only shown when the server was
                        // launched with `--root <dir>` AND it found at
                        // least one repo. Otherwise the block hides
                        // entirely so single-repo deployments don't
                        // see an empty section.
                        let repos_state = repos.read_unchecked().clone();
                        let items: Vec<_> = repos_state
                            .as_ref()
                            .and_then(|r| r.as_ref().ok())
                            .cloned()
                            .unwrap_or_default();
                        if items.is_empty() {
                            rsx! {}
                        } else {
                            let n = items.len();
                            let active = current_repo.read().clone();
                            rsx! {
                                section { class: "side-block",
                                    div { class: "side-title",
                                        span { "Workspaces" }
                                        span { class: "side-count", "{n}" }
                                    }
                                    ul { class: "workspace-list",
                                        for repo in items {
                                            {
                                                let path = repo.path.clone();
                                                let is_active = path == active;
                                                let branch = repo
                                                    .head_ref
                                                    .clone()
                                                    .unwrap_or_else(|| "(detached)".into());
                                                let target = path.clone();
                                                let cls = if is_active {
                                                    "workspace-item active"
                                                } else {
                                                    "workspace-item"
                                                };
                                                rsx! {
                                                    li { key: "{path}",
                                                        button {
                                                            class: "{cls}",
                                                            title: "{path}",
                                                            onclick: move |_| {
                                                                current_repo.set(target.clone());
                                                            },
                                                            span { class: "workspace-name", "{repo.name}" }
                                                            span { class: "workspace-branch", "{branch}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Branches" }
                            {render_branch_count(&branches.read_unchecked())}
                        }
                        {render_branches(
                            &branches.read_unchecked(),
                            branches,
                            current_repo,
                            new_branch,
                        )}
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Remotes" }
                            {render_remote_count(&remotes.read_unchecked())}
                        }
                        {render_remotes(&remotes.read_unchecked(), remotes)}
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Tags" }
                            {render_tag_count(&tags.read_unchecked())}
                        }
                        {render_tags(&tags.read_unchecked(), tags)}
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Stashes" }
                            {render_stash_count(&stashes.read_unchecked())}
                        }
                        {render_stashes(&stashes.read_unchecked(), stashes, current_repo)}
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Staged" }
                            {render_staged_count(&staged.read_unchecked())}
                        }
                        {render_staged(
                            &staged.read_unchecked(),
                            staged,
                            current_repo,
                        )}
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Working tree" }
                            {render_status_count(&status.read_unchecked())}
                        }
                        {render_status(
                            &status.read_unchecked(),
                            status,
                            selected_oid,
                            selected_file,
                            selected_blob,
                            current_repo,
                        )}
                    }
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Files at HEAD" }
                            {render_tree_count(&tree.read_unchecked())}
                        }
                        {render_tree(
                            &tree.read_unchecked(),
                            tree,
                            selected_oid,
                            selected_file,
                            selected_blob,
                        )}
                    }
                }

                main { class: "main",
                    {render_summary_card(&summary.read_unchecked(), summary)}

                    {
                        let count = staged.read_unchecked()
                            .as_ref()
                            .and_then(|r| r.as_ref().ok())
                            .map(|v| v.len())
                            .unwrap_or(0);
                        render_commit_form(
                            commit_msg,
                            commit_err,
                            commit_author,
                            count,
                            current_repo,
                        )
                    }

                    section { class: "main-block scroll-log",
                        div { class: "block-toolbar",
                            h2 { style: "margin: 0;", "History" }
                            label { class: "all-branches-toggle",
                                title: "Walk every ref tip — local + remote-tracking branches — instead of just HEAD's ancestors.",
                                input {
                                    r#type: "checkbox",
                                    checked: *log_all.read(),
                                    oninput: move |e| log_all.set(e.value() == "true"),
                                }
                                span { "All branches" }
                            }
                            input {
                                class: "history-search",
                                r#type: "search",
                                placeholder: "Filter by subject / author / oid prefix",
                                value: "{log_query}",
                                spellcheck: "false",
                                autocapitalize: "off",
                                autocomplete: "off",
                                oninput: move |e| log_query.set(e.value()),
                            }
                        }
                        div { class: "log-scroll",
                            {
                                let show_graph = log_query.read().trim().is_empty();
                                render_log(
                                    &log.read_unchecked(),
                                    log,
                                    selected_oid,
                                    selected_file,
                                    selected_blob,
                                    show_graph,
                                )
                            }
                        }
                    }

                    section { class: "main-block",
                        div { class: "block-toolbar",
                            h2 { style: "margin: 0;",
                                if selected_blob.read().is_some() { "File viewer" }
                                else if selected_file.read().is_some() { "Working tree change" }
                                else { "Commit detail" }
                            }
                            {
                                // Merge / cherry-pick buttons appear only
                                // when a commit is selected (not a blob /
                                // working-tree file). They target HEAD's
                                // current branch — we read the branch
                                // name out of the summary signal for the
                                // label so the user knows where the work
                                // is landing.
                                let oid = selected_oid.read().clone();
                                let on_commit = oid.is_some()
                                    && selected_file.read().is_none()
                                    && selected_blob.read().is_none();
                                if let (true, Some(oid)) = (on_commit, oid) {
                                    let short: String = oid.chars().take(8).collect();
                                    let current_branch = summary
                                        .read_unchecked()
                                        .as_ref()
                                        .and_then(|r| r.as_ref().ok())
                                        .and_then(|s| s.head_ref.clone())
                                        .unwrap_or_else(|| "HEAD".into());
                                    let busy = *net_busy.read();
                                    let oid_for_merge = oid.clone();
                                    let oid_for_pick = oid.clone();
                                    let oid_for_rebase = oid.clone();
                                    let oid_for_revert = oid.clone();
                                    let oid_for_reset = oid.clone();
                                    let short_for_reset = short.clone();
                                    rsx! {
                                        div { class: "commit-actions",
                                            button {
                                                class: "commit-action",
                                                disabled: busy,
                                                title: "git merge {short} (creates a merge commit unless ff is possible)",
                                                onclick: move |_| {
                                                    let path = current_repo.read().clone();
                                                    let target = oid_for_merge.clone();
                                                    net_busy.set(true);
                                                    net_result.set(None);
                                                    spawn(async move {
                                                        let r = fetch::post_merge(&path, &target, false).await;
                                                        net_busy.set(false);
                                                        net_result.set(Some(r));
                                                        state.restart();
                                                    });
                                                },
                                                "Merge into {current_branch}"
                                            }
                                            button {
                                                class: "commit-action",
                                                disabled: busy,
                                                title: "git cherry-pick {short}",
                                                onclick: move |_| {
                                                    let path = current_repo.read().clone();
                                                    let target = oid_for_pick.clone();
                                                    net_busy.set(true);
                                                    net_result.set(None);
                                                    spawn(async move {
                                                        let r = fetch::post_cherry_pick(&path, &target).await;
                                                        net_busy.set(false);
                                                        net_result.set(Some(r));
                                                        state.restart();
                                                    });
                                                },
                                                "Cherry-pick"
                                            }
                                            button {
                                                class: "commit-action",
                                                disabled: busy,
                                                title: "git rebase {short} (replay {current_branch} onto this commit)",
                                                onclick: move |_| {
                                                    let path = current_repo.read().clone();
                                                    let upstream = oid_for_rebase.clone();
                                                    net_busy.set(true);
                                                    net_result.set(None);
                                                    spawn(async move {
                                                        let r = fetch::post_rebase(&path, &upstream).await;
                                                        net_busy.set(false);
                                                        net_result.set(Some(r));
                                                        state.restart();
                                                    });
                                                },
                                                "Rebase onto"
                                            }
                                            button {
                                                class: "commit-action",
                                                disabled: busy,
                                                title: "git revert {short} (creates a new commit that inverts this one)",
                                                onclick: move |_| {
                                                    let path = current_repo.read().clone();
                                                    let oid = oid_for_revert.clone();
                                                    net_busy.set(true);
                                                    net_result.set(None);
                                                    spawn(async move {
                                                        let r = fetch::post_revert(&path, &oid).await;
                                                        net_busy.set(false);
                                                        net_result.set(Some(r));
                                                        state.restart();
                                                    });
                                                },
                                                "Revert"
                                            }
                                            select {
                                                class: "reset-mode",
                                                value: "{reset_mode}",
                                                title: "Reset mode — soft keeps stage+worktree, mixed keeps worktree only, hard discards everything.",
                                                onchange: move |e| reset_mode.set(e.value()),
                                                option { value: "soft", "soft" }
                                                option { value: "mixed", "mixed" }
                                                option { value: "hard", "hard" }
                                            }
                                            button {
                                                class: "commit-action",
                                                disabled: busy,
                                                title: "git reset --{reset_mode} {short_for_reset}",
                                                onclick: move |_| {
                                                    let path = current_repo.read().clone();
                                                    let target = oid_for_reset.clone();
                                                    let mode = reset_mode.read().clone();
                                                    if mode == "hard"
                                                        && !sidebar::browser_confirm(
                                                            "Hard reset discards all uncommitted changes in the worktree and index. Continue?",
                                                        )
                                                    {
                                                        return;
                                                    }
                                                    net_busy.set(true);
                                                    net_result.set(None);
                                                    spawn(async move {
                                                        let r = fetch::post_reset(&path, &target, &mode).await;
                                                        net_busy.set(false);
                                                        match r {
                                                            Ok(()) => net_result.set(None),
                                                            Err(e) => net_result.set(Some(Err(e))),
                                                        }
                                                        state.restart();
                                                    });
                                                },
                                                "Reset"
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {}
                                }
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
                            diff,
                            &working_diff_stale.read_unchecked(),
                            working_diff,
                            working_diff,
                            &blob_view_stale.read_unchecked(),
                            blob_view,
                            &blame_view_stale.read_unchecked(),
                            blob_query,
                            selected_oid,
                            selected_file,
                            selected_blob,
                            *side_by_side.read(),
                            current_repo,
                            hunk_picker,
                            net_busy,
                            net_result,
                        )}
                    }
                }
            }

            footer { class: "footbar",
                "API · "
                a { href: "/api/health", target: "_blank", "health" }
                " · "
                a { href: "/api/repo/summary?path={repo_q}", target: "_blank", "summary" }
                " · "
                a { href: "/api/repo/log?path={repo_q}&limit={LOG_LIMIT}", target: "_blank", "log" }
                " · "
                a { href: "/api/repo/status?path={repo_q}", target: "_blank", "status" }
                " · "
                a { href: "/api/repo/branches?path={repo_q}", target: "_blank", "branches" }
            }
        }
    }
}
