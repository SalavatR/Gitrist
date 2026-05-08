use dioxus::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct RepoSummary {
    path: String,
    git_dir: String,
    head_ref: Option<String>,
    head_oid: Option<String>,
    is_detached: bool,
}

const DEMO_REPO: &str = "/home/salavat/gitrust";

#[component]
pub fn App() -> Element {
    let summary = use_resource(|| async move { fetch_summary(DEMO_REPO).await });
    let mut counter = use_signal(|| 0u32);

    let summary_view = match &*summary.read_unchecked() {
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
                table {
                    tr { td { "path" } td { code { "{path}" } } }
                    tr { td { "git dir" } td { code { "{git_dir}" } } }
                    tr { td { "branch" } td { code { "{branch}" } } }
                    tr { td { "head" } td { code { "{oid_short}" } } }
                }
            }
        }
        Some(Err(e)) => {
            let msg = e.clone();
            rsx! { p { style: "color:#c33;", "Error: {msg}" } }
        }
        None => rsx! { p { "Loading…" } },
    };

    rsx! {
        h1 { "gitrust" }
        p { "Self-hosted Rust GUI git client. This page is "
            code { "gitrust-ui" }
            " compiled to WebAssembly, served by "
            code { "gitrust-server" }
            "."
        }
        p {
            "Liveness: counter "
            code { "{counter}" }
            " "
            button { onclick: move |_| counter += 1, "+1" }
        }
        section {
            h2 { "Repository" }
            {summary_view}
        }
        p {
            "API: "
            a { href: "/api/health", target: "_blank", "/api/health" }
            " · "
            a { href: "/api/repo/summary?path=/home/salavat/gitrust", target: "_blank", "/api/repo/summary" }
            " · "
            a { href: "/api/repo/log?path=/home/salavat/gitrust&limit=10", target: "_blank", "/api/repo/log" }
        }
    }
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

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_summary(_path: &str) -> Result<RepoSummary, String> {
    Err("native build: fetching not implemented".into())
}
