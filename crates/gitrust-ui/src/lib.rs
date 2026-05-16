//! Dioxus UI for gitrust. Mounted by `gitrust-web` in the browser; also
//! compiles on native targets (with stub fetchers) so `cargo check` is
//! cheap for non-WASM contributors.

use dioxus::prelude::*;
use gitrust_types::{BlobView, CommitDiff, FileDiff};

mod diff;
mod fetch;
mod main_panel;
mod sidebar;
mod state;
mod time_fmt;
#[cfg(target_arch = "wasm32")]
mod ws;

use fetch::{
    fetch_auth_token, fetch_blob, fetch_branches, fetch_diff, fetch_diff_working, fetch_log,
    fetch_remotes, fetch_staged, fetch_status, fetch_summary, fetch_tags, fetch_tree,
};
use main_panel::{render_commit_form, render_detail, render_log, render_summary_card};
use sidebar::{
    render_branch_count, render_branches, render_remote_count, render_remotes, render_staged,
    render_staged_count, render_status, render_status_count, render_tag_count, render_tags,
    render_tree, render_tree_count,
};
use state::{
    BlobSelection, LOG_LIMIT, REFS_POLL_INTERVAL_MS, STATUS_POLL_INTERVAL_MS, ThemeMode,
    apply_theme, initial_repo, initial_side_by_side, initial_theme, persist_repo,
    persist_side_by_side, recent_repos, record_recent_repo,
};
use time_fmt::sleep_ms;

#[component]
pub fn App() -> Element {
    let initial = initial_repo();
    let mut current_repo = use_signal(|| initial.clone());
    let mut draft_repo = use_signal(|| initial.clone());
    let selected_oid = use_signal(|| None::<String>);
    let selected_file = use_signal(|| None::<String>);
    let selected_blob = use_signal(|| None::<BlobSelection>);
    let mut side_by_side = use_signal(initial_side_by_side);
    let mut recent = use_signal(recent_repos);
    let mut theme = use_signal(initial_theme);
    let mut auth_token = use_signal(|| None::<String>);
    let commit_msg = use_signal(String::new);
    let commit_err = use_signal(|| None::<String>);

    use_future(move || async move {
        if let Ok(t) = fetch_auth_token().await {
            auth_token.set(Some(t));
        }
        // Silent failure: writes will surface a 401 if the token never
        // landed. Reads keep working either way.
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
        async move { fetch_log(&path, LOG_LIMIT).await }
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
                        for r in recent.read().iter().cloned() {
                            option { key: "{r}", value: "{r}" }
                        }
                    }
                    button { r#type: "submit", "Load" }
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
            }

            div { class: "split",
                aside { class: "sidebar",
                    section { class: "side-block",
                        div { class: "side-title",
                            span { "Branches" }
                            {render_branch_count(&branches.read_unchecked())}
                        }
                        {render_branches(&branches.read_unchecked(), branches)}
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
                            span { "Staged" }
                            {render_staged_count(&staged.read_unchecked())}
                        }
                        {render_staged(
                            &staged.read_unchecked(),
                            staged,
                            current_repo,
                            auth_token,
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
                            auth_token,
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
                        render_commit_form(commit_msg, commit_err, count, current_repo, auth_token)
                    }

                    section { class: "main-block",
                        h2 { "History" }
                        {render_log(
                            &log.read_unchecked(),
                            log,
                            selected_oid,
                            selected_file,
                            selected_blob,
                        )}
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
                            diff,
                            &working_diff_stale.read_unchecked(),
                            working_diff,
                            &blob_view_stale.read_unchecked(),
                            blob_view,
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
