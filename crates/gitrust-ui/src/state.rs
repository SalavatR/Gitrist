//! Client-side state: types passed between components and localStorage
//! persistence for repo path and view-mode preferences.

use dioxus::prelude::GlobalSignal;

/// The access token shared by every authenticated request. The server
/// prints it at startup; the user pastes it into the auth gate. Stored
/// in localStorage so signing in survives a refresh.
pub(crate) static AUTH_TOKEN: GlobalSignal<Option<String>> = GlobalSignal::new(initial_auth_token);

#[cfg(target_arch = "wasm32")]
const TOKEN_STORAGE_KEY: &str = "gitrust.token";

#[cfg(target_arch = "wasm32")]
fn initial_auth_token() -> Option<String> {
    use gloo_storage::Storage;
    gloo_storage::LocalStorage::get::<String>(TOKEN_STORAGE_KEY)
        .ok()
        .filter(|s| !s.is_empty())
}

#[cfg(not(target_arch = "wasm32"))]
fn initial_auth_token() -> Option<String> {
    None
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn persist_auth_token(token: &str) {
    use gloo_storage::Storage;
    let _ = gloo_storage::LocalStorage::set(TOKEN_STORAGE_KEY, token);
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn persist_auth_token(_token: &str) {}

#[cfg(target_arch = "wasm32")]
pub(crate) fn clear_auth_token() {
    use gloo_storage::Storage;
    gloo_storage::LocalStorage::delete(TOKEN_STORAGE_KEY);
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn clear_auth_token() {}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ThemeMode {
    Auto,
    Light,
    Dark,
}

impl ThemeMode {
    /// Step through Auto → Light → Dark → Auto on each toggle click.
    pub(crate) fn next(self) -> Self {
        match self {
            ThemeMode::Auto => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::Auto,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ThemeMode::Auto => "Auto",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn as_str(self) -> &'static str {
        match self {
            ThemeMode::Auto => "auto",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn from_str(s: &str) -> Self {
        match s {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::Auto,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct BlobSelection {
    pub oid: String,
    pub path: String,
}

pub(crate) const DEFAULT_REPO: &str = "/home/salavat/gitrust";
/// How many commits to request per log fetch. The server caps at 500;
/// we ask for the full cap so users can scroll into deep history
/// without paginating. The log block is scrollable so the row count
/// doesn't push the rest of the layout off-screen.
pub(crate) const LOG_LIMIT: usize = 500;
pub(crate) const STATUS_POLL_INTERVAL_MS: u32 = 2_000;
pub(crate) const REFS_POLL_INTERVAL_MS: u32 = 10_000;

#[cfg(target_arch = "wasm32")]
const REPO_STORAGE_KEY: &str = "gitrust.repo";
#[cfg(target_arch = "wasm32")]
const VIEW_MODE_STORAGE_KEY: &str = "gitrust.view_mode";
#[cfg(target_arch = "wasm32")]
const RECENT_REPOS_STORAGE_KEY: &str = "gitrust.repos";
/// Cap on the recent-repos quick-switch list — keep enough for a
/// useful dropdown but small enough to scan at a glance.
#[cfg(target_arch = "wasm32")]
const RECENT_REPOS_MAX: usize = 8;
#[cfg(target_arch = "wasm32")]
const THEME_STORAGE_KEY: &str = "gitrust.theme";
#[cfg(target_arch = "wasm32")]
const LOG_ALL_STORAGE_KEY: &str = "gitrust.log_all";

#[cfg(target_arch = "wasm32")]
pub(crate) fn initial_log_all() -> bool {
    use gloo_storage::Storage;
    gloo_storage::LocalStorage::get::<String>(LOG_ALL_STORAGE_KEY)
        .ok()
        .map(|s| s == "true")
        .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn initial_log_all() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn persist_log_all(all: bool) {
    use gloo_storage::Storage;
    let _ =
        gloo_storage::LocalStorage::set(LOG_ALL_STORAGE_KEY, if all { "true" } else { "false" });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn persist_log_all(_all: bool) {}

#[cfg(target_arch = "wasm32")]
pub(crate) fn initial_repo() -> String {
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
pub(crate) fn initial_repo() -> String {
    DEFAULT_REPO.to_string()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn persist_repo(path: &str) {
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
pub(crate) fn persist_repo(_path: &str) {}

#[cfg(target_arch = "wasm32")]
pub(crate) fn initial_side_by_side() -> bool {
    use gloo_storage::Storage;
    gloo_storage::LocalStorage::get::<String>(VIEW_MODE_STORAGE_KEY)
        .ok()
        .map(|s| s == "side")
        .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn initial_side_by_side() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn persist_side_by_side(side_by_side: bool) {
    use gloo_storage::Storage;
    let val = if side_by_side { "side" } else { "unified" };
    let _ = gloo_storage::LocalStorage::set(VIEW_MODE_STORAGE_KEY, val);
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn persist_side_by_side(_side_by_side: bool) {}

/// Read the recent-repos list from localStorage, freshest first.
/// Returns an empty list on native or when no list has been recorded.
#[cfg(target_arch = "wasm32")]
pub(crate) fn recent_repos() -> Vec<String> {
    use gloo_storage::Storage;
    gloo_storage::LocalStorage::get::<Vec<String>>(RECENT_REPOS_STORAGE_KEY).unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn recent_repos() -> Vec<String> {
    Vec::new()
}

/// Move `path` to the front of the recent list, dedup, and cap the
/// total at `RECENT_REPOS_MAX`. Called every time the active repo
/// changes so the freshest entry is always at index 0.
#[cfg(target_arch = "wasm32")]
pub(crate) fn record_recent_repo(path: &str) {
    use gloo_storage::Storage;
    if path.is_empty() {
        return;
    }
    let mut list: Vec<String> =
        gloo_storage::LocalStorage::get(RECENT_REPOS_STORAGE_KEY).unwrap_or_default();
    list.retain(|p| p != path);
    list.insert(0, path.to_string());
    list.truncate(RECENT_REPOS_MAX);
    let _ = gloo_storage::LocalStorage::set(RECENT_REPOS_STORAGE_KEY, &list);
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn record_recent_repo(_path: &str) {}

/// Read the user's saved theme preference. Auto on first load and
/// when no value was persisted yet.
#[cfg(target_arch = "wasm32")]
pub(crate) fn initial_theme() -> ThemeMode {
    use gloo_storage::Storage;
    gloo_storage::LocalStorage::get::<String>(THEME_STORAGE_KEY)
        .ok()
        .as_deref()
        .map(ThemeMode::from_str)
        .unwrap_or(ThemeMode::Auto)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn initial_theme() -> ThemeMode {
    ThemeMode::Auto
}

/// Persist the theme to localStorage and reflect it in the document's
/// `data-theme` attribute — `light` / `dark` force a palette, `auto`
/// removes the attribute so `prefers-color-scheme` takes over.
#[cfg(target_arch = "wasm32")]
pub(crate) fn apply_theme(mode: ThemeMode) {
    use gloo_storage::Storage;
    let _ = gloo_storage::LocalStorage::set(THEME_STORAGE_KEY, mode.as_str());

    let window = gloo_utils::window();
    if let Some(doc) = window.document()
        && let Some(html) = doc.document_element()
    {
        match mode {
            ThemeMode::Auto => {
                let _ = html.remove_attribute("data-theme");
            }
            other => {
                let _ = html.set_attribute("data-theme", other.as_str());
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn apply_theme(_mode: ThemeMode) {}
