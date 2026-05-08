# Architecture

## Vision

gitrust is a self-hosted GUI git client. A single Rust binary runs as
a local server, owns the repositories on the host machine, and exposes
a REST API. Two UI surfaces consume that API:

- A **web shell** — Dioxus components compiled to WebAssembly, served
  by the same binary, opened in any browser.
- A **native shell** — planned. A desktop window (via dioxus-desktop
  or wry+tao) loading the same WASM bundle from `localhost`.

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
                    │ gitrust-server │   axum, REST API, ServeDir
                    └────────┬───────┘
                             │
                    ┌────────▼───────┐
                    │  gitrust-core  │   gix-backed git operations
                    └────────────────┘

                    ┌────────────────┐
                    │  gitrust-web   │   wasm32 entry, dioxus::launch
                    └────────┬───────┘
                             │
                    ┌────────▼───────┐
                    │  gitrust-ui    │   Dioxus components
                    └────────────────┘
```

Note that `gitrust-server` does **not** depend on `gitrust-ui`. The
server doesn't render anything itself; it just serves the pre-built
WASM bundle from disk via `tower_http::services::ServeDir`. There is
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

Both `gitrust-core` (server side) and `gitrust-ui` (client side)
define mirror Rust types for the API payload — `RepoSummary`,
`CommitInfo`, `StatusEntry`. They are intentionally duplicated rather
than extracted into a shared crate, because a shared crate would have
to compile for both wasm32 and native. As long as the field names
match, `serde` deserialization keeps the wire format consistent.

A future `gitrust-types` crate is the right move once the duplication
grows painful — at that point the shared crate becomes a small,
target-independent set of `Serialize`/`Deserialize` structs with no
git deps.

## State and data flow

Data flow is one-directional: UI → fetch → server → gix → response.
There is no write API yet, no server-side state beyond the in-flight
request, and no caching. Each `use_resource` call in the UI triggers
a fresh fetch.

The selected repository path lives in a `dioxus::Signal` in the
top-level `App` component. Three `use_resource`s read it; each
re-runs when the signal value changes.

## Where things will grow

- **Write actions** (commit, branch create, checkout). These need
  authentication of some flavor — even single-user, self-hosted, the
  binary should not expose write actions to whoever can `curl
  localhost`. A signed cookie set on first launch is enough.
- **Multiple repos at once**. Currently the path is a query param on
  every request. A future `gitrust serve --root /path/to/repos` could
  let the UI list workspaces by name rather than path.
- **Per-repo background refresh**. Right now the UI refetches on
  Signal changes only. Eventually a WebSocket push from the server
  on filesystem events would be cheaper than poll.
