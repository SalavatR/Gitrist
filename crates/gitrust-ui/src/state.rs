//! Client-side state: types passed between components and localStorage
//! persistence for repo path and view-mode preferences.

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct BlobSelection {
    pub oid: String,
    pub path: String,
}

pub(crate) const DEFAULT_REPO: &str = "/home/salavat/gitrust";
pub(crate) const LOG_LIMIT: usize = 50;
pub(crate) const STATUS_POLL_INTERVAL_MS: u32 = 2_000;
pub(crate) const REFS_POLL_INTERVAL_MS: u32 = 10_000;

#[cfg(target_arch = "wasm32")]
const REPO_STORAGE_KEY: &str = "gitrust.repo";
#[cfg(target_arch = "wasm32")]
const VIEW_MODE_STORAGE_KEY: &str = "gitrust.view_mode";

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
