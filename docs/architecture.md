# Architecture

## Vision

gitrust is a self-hosted GUI git client. A single Rust binary runs as
a local server, owns the repositories on the host machine, and exposes
a REST API. Two UI surfaces consume that API:

- A **web shell** — Dioxus components compiled to WebAssembly, served
  by the same binary, opened in any browser.
- A **native shell** — `gitrust app` (with `--features desktop`)
  opens a wry+tao window pointing at the same in-process server,
  with a URL-mode fallback when no display is available.

Both shells talk to the same in-process server, so the source of
truth is always the same — no client-side replicas of git state.

The pattern is the same as `code-server` or JupyterLab: server + UI
bundled into one binary, web view by default, native window optional.

## Workspace layering

Each layer is allowed to depend only on layers below it.

```
                    ┌────────────────┐
                    │    gitrust     │   binary; clap; wires layers
                    └────────┬───────┘
                             │
                    ┌────────▼───────┐
                    │ gitrust-server │   axum, REST API, ServeDir,
                    └────────┬───────┘   /api/repo/events WebSocket
                             │
                    ┌────────▼───────┐
                    │  gitrust-core  │   gix-backed git operations
                    └────────┬───────┘
                             │
                    ┌────────▼───────┐
                    │ gitrust-types  │   wire structs (serde),
                    └────────▲───────┘   target-independent
                             │
                    ┌────────┴───────┐
                    │  gitrust-ui    │   Dioxus components
                    └────────▲───────┘
                             │
                    ┌────────┴───────┐
                    │  gitrust-web   │   wasm32 entry, dioxus::launch
                    └────────────────┘
```

`gitrust-types` sits in the middle: both the server stack
(core → server) and the client stack (ui → web) depend on it, so a
wire-shape change is a single edit that surfaces as a compile error
on both sides.

Note that `gitrust-server` does **not** depend on `gitrust-ui`. The
server doesn't render anything itself; it just serves the pre-built
WASM bundle (from disk via `tower_http::services::ServeDir` in dev
mode, or baked in via `include_dir!` for release builds). There is
no SSR, no server functions.

## The wasm/native split

The web shell compiles to `wasm32-unknown-unknown`. Anything in the
dependency graph for that target must be wasm-compatible. This rules
out:

- `gix` (and any native git binding via FFI),
- anything pulled in via `cc-rs` or system libs (`openssl-sys`,
  `libssh2-sys`, etc.).

So `gitrust-ui` can never depend on `gitrust-core`. UI talks to git
indirectly, through HTTP calls to `gitrust-server`.

For wasm-only deps in `gitrust-ui` (currently just `gloo-net` for
`fetch`), use Cargo target-conditional dependencies:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
gloo-net = "0.6"
```

…and gate the consumer code with `#[cfg(target_arch = "wasm32")]`. The
native target gets a stub. This keeps `cargo check` working on the
native default-members set.

## Types across the wire

Wire shapes live in a dedicated `gitrust-types` crate: pure
`Serialize`/`Deserialize` structs (`RepoSummary`, `CommitInfo`,
`StatusEntry`, `FileDiff`, `BlobView`, …) with no git or browser
deps so it compiles unchanged for both `wasm32-unknown-unknown` and
native targets. Both `gitrust-core` and `gitrust-ui` use these
types directly — renaming a field is a single edit and the type
system catches every consumer on both sides.

`gitrust-core` returns these types directly from its functions; the
server passes them through `axum::Json` without conversion. The UI
deserializes the same structs out of `gloo_net::Request::json`.

## State and data flow

Read flow is one-directional: UI → `fetch` → server → `gix` →
response. The selected repository path lives in a `dioxus::Signal`
on the top-level `App` component; every `use_resource` reads it and
re-runs when the value changes.

Refresh is primarily push-based. The UI opens one WebSocket per
repo to `/api/repo/events`; the server watches the worktree via
`notify` and pushes debounced, deduplicated event kinds
(`head_changed`, `refs_changed`, `index_changed`,
`worktree_changed`) which the UI dispatches to the affected
`use_resource.restart()`s. A short 2 s / 10 s polling pair stays
as a silent fallback for when the WebSocket can't deliver. The
`use_resource` for the WebSocket is itself keyed on the repo path,
so swapping repos closes the previous socket cleanly.

Stale-while-revalidate is wired for the three main-panel
resources (commit diff, working diff, blob view): mirror Signals
hold the last successful value so clicking a new commit / file
doesn't blank the panel while the new fetch lands.

There is no write API yet and no server-side state beyond the
in-flight requests and active fs-watchers.

## Where things will grow

- **Write actions** (commit, branch create, checkout). These need
  authentication of some flavor — even single-user, self-hosted,
  the binary should not expose write actions to whoever can `curl
  localhost`. A signed cookie set on first launch is the current
  sketch.
- **Multiple repos at once**. Currently the path is a query param
  on every request. A future `gitrust serve --root /path/to/repos`
  could let the UI list workspaces by name rather than path.
- **Pre-built binaries** via `cargo-dist` so users don't have to
  install `webkit2gtk-4.1` system packages just to try the
  desktop feature.
