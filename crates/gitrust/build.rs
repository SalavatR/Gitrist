// Ensure crates/gitrust-web/dist/ exists at compile time so the
// `include_dir!` macro in `main.rs` doesn't fail on a fresh checkout
// before `make web` has run. An empty dir is fine — the server detects
// that case at startup and warns the user.
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo");
    let dist = std::path::Path::new(&manifest).join("../gitrust-web/dist");
    if !dist.exists() {
        let _ = std::fs::create_dir_all(&dist);
    }
}
