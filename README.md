# gitrust

A self-hosted Rust GUI git client. One binary serves the user's local
repositories through both a browser-based UI and (planned) a native
desktop window — both shells talk to the same in-process server, so
they always see the same state.

## Status

Functional read/write git client with a browser UI and an optional
native window. Roughly v0.3 territory.

What works today:

- `gitrust serve` boots an axum server with a REST API and serves a
  Dioxus WebAssembly UI. `gitrust app` (built with
  `--features desktop`) opens a wry+tao window instead of relying on
  a browser; without a display it falls back to printing the URL.
  The native window carries a `muda` menu bar (File → Open Repo…,
  View → Reload, Help → About) and Cmd/Ctrl-R / -Q / -W shortcuts.
- Reads cover: `summary`, `log` (with substring search across
  subject / body / author / oid prefix), `status`, `staged`,
  `branches`, `tags`, `remotes`, `tree`, `blob`, `blame`, `diff`,
  `diff/working`, `commit?oid=` (all under `/api/repo/`). Any local
  repo by absolute path.
- Writes (bearer-token gated): `stage`, `unstage`, `discard`,
  `commit` (with body + author override), branch create / delete
  (with force fallback) / rename / checkout, and stash save /
  list / pop / drop.
- Web shell shows: repository summary card, commit-history table
  with a colored graph column rendering branches and merges, an
  in-history substring filter, a sidebar with branches / remotes /
  tags / stashes / staged / working-tree / files-at-HEAD blocks
  (each clickable to open the matching diff or blob), a file
  viewer with tree-sitter syntax highlighting (Rust, JSON, HTML,
  CSS, TS/JS, Python, TOML, Lua, Markdown — including inline `code`,
  *emphasis*, **strong** via a two-pass merge) and a per-line blame
  column, per-commit diff with rename detection, working-tree diff
  per file, switchable unified / side-by-side view, auto-collapse
  on large diffs, light / dark theme toggle, and an in-file find
  box.
- Auth: 32-byte token generated on first launch into
  `$XDG_CONFIG_HOME/gitrust/token`; UI prompts for it on first
  load, every other endpoint than `/api/health` requires it.
- Errors carry a structured envelope `{ error, code, hint? }` with
  category-mapped HTTP statuses (`repo_not_found`,
  `branch_unmerged`, `worktree_dirty`, `permission_denied`,
  `bad_oid`, `already_exists`, `generic`).
- Live updates via WebSocket: the server watches the worktree with
  `notify`, pushes debounced event kinds over `/api/repo/events`,
  and the UI restarts the affected resources without polling delay.
- Single-binary release: `cargo build --release` bakes the WASM
  bundle in via `include_dir!`, so `./target/release/gitrust serve`
  is self-contained. Tagged releases (`vX.Y.Z`) attach macOS and
  Windows binaries to the GitHub Release via Actions.

Deferred:

- Network ops: `fetch`, `pull`, `push`. Need credentials handling
  (SSH agent + HTTPS keychain) and a streaming progress channel.
- Merge / rebase / cherry-pick and a conflict-resolution UI.
- Reset / revert.
- Hunk-level (and line-level) staging, à la `git add -p`.
- Tag create / delete, file-history (log filtered to a single path),
  arbitrary-ref diff.
- Multi-repo browser (`gitrust serve --root <dir>` listing workspaces).
- Pre-built Linux binary (needs a webkit2gtk-4.1 ABI pin
  strategy across distros).

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

Six crates under `crates/`:

| Crate            | Role                                                                                |
| ---------------- | ----------------------------------------------------------------------------------- |
| `gitrust-types`  | Wire-shape structs shared by server and UI. Pure serde, target-independent.         |
| `gitrust-core`   | Git operations via [gix](https://crates.io/crates/gix); returns wire types.         |
| `gitrust-server` | axum router serving `/api/*`, the WASM bundle, and the `/api/repo/events` WS.       |
| `gitrust-ui`     | Dioxus components, split into `state`/`fetch`/`ws`/`sidebar`/`main_panel`/`diff`.   |
| `gitrust-web`    | wasm32 entry binary that mounts the UI.                                             |
| `gitrust`        | main binary with `serve` and `app` subcommands.                                     |

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
