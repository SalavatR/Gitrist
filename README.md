# gitrust

A self-hosted Rust GUI git client. One binary serves the user's local
repositories through both a browser-based UI and (planned) a native
desktop window — both shells talk to the same in-process server, so
they always see the same state.

## Status

Early. Roughly v0.1 territory.

What works today:

- `gitrust serve` boots an axum server with a REST API and serves a
  Dioxus WebAssembly UI as static files.
- Endpoints: `/api/health`, `/api/repo/summary`, `/api/repo/log`,
  `/api/repo/status` against any local git repo by absolute path.
- Web shell shows repository summary, working-tree changes, and recent
  commits. The active repo path is editable via an input at the top —
  all three views refetch when it changes.

Deferred:

- Native (desktop window) shell. The dioxus-desktop / wry path needs
  webkit2gtk + a display server, neither available in the current dev
  environment. Worth tackling on a real desktop.
- Diff viewer, branch ops, staging/commit, write actions.

## Quickstart

```sh
make setup     # rustup target add wasm32 + cargo install wasm-bindgen-cli
make run       # build WASM bundle and start the server on 127.0.0.1:3737
```

Open <http://127.0.0.1:3737>, then type any local git repository's
absolute path into the input at the top of the page and press **Load**.

For a self-contained release binary that bakes the WASM bundle in:

```sh
make web                                    # build the bundle once
cargo build --release                       # server-only, opens in a browser
cargo build --release --features desktop    # server + native window via wry
```

The desktop feature pulls in wry/tao and on Linux requires
`webkit2gtk-4.1` + `libsoup-3.0`. macOS uses WKWebView (built-in),
Windows uses WebView2 (auto-bootstrap). Per-platform install
instructions are in [docs/build.md](docs/build.md).

`make help` lists all targets. Override the bind address with
`ADDR=0.0.0.0:8080 make run`.

## Workspace

Five crates under `crates/`:

| Crate            | Role                                                          |
| ---------------- | ------------------------------------------------------------- |
| `gitrust-core`   | Git operations via [gix](https://crates.io/crates/gix).       |
| `gitrust-server` | axum router serving `/api/*` and the WASM bundle.             |
| `gitrust-ui`     | Dioxus components, target-independent.                        |
| `gitrust-web`    | wasm32 entry binary that mounts the UI.                       |
| `gitrust`        | main binary with `serve` (impl) and `app` (stub) subcommands. |

`gitrust-web` is excluded from `default-members` because `dioxus-web`
only links for `wasm32-unknown-unknown` — running `cargo build` from
the workspace root therefore skips it; the `Makefile` builds it
explicitly with `--target wasm32-unknown-unknown`.

## Docs

- [docs/architecture.md](docs/architecture.md) — design and layering rules.
- [docs/api.md](docs/api.md) — REST API reference with examples.
- [docs/build.md](docs/build.md) — build pipeline and how to extend it.

## License

Dual-licensed under MIT or Apache-2.0 at your option.
