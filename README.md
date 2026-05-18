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
- Network ops: `fetch`, `pull` (`--ff-only` by default), and
  `push` (`-u` / `--force-with-lease` flags optional). All shell
  out to the user's `git` binary so SSH agents and HTTPS
  credential helpers Just Work. The UI dispatches to async
  endpoints (`POST /api/repo/{fetch,pull,push}-async` →
  `op_id`) and polls `GET /api/repo/op-progress` every 500 ms,
  so the banner streams git's stderr as the op runs instead
  of blocking on the final summary.
- History-movers: `merge` (with `--no-ff` opt-in), `cherry-pick`,
  `rebase`, `revert`, and `reset` (with `soft`/`mixed`/`hard`
  modes), all reachable as buttons in the commit-detail toolbar
  so the user picks a target commit visually instead of retyping
  its oid. Reset prompts a confirm dialog on `hard` mode.
- Hunk-level staging (`git add -p` in a UI). Working-tree diff
  rows get a checkbox per hunk; the "Stage N hunk(s)" button
  ships a subset patch through `git apply --cached --recount`
  so only the ticked hunks land in the index.
- Multi-repo browser. `gitrust serve --root <dir>` (and
  `gitrust app --root <dir>`) scans the directory for git
  worktrees up to five levels deep; the UI shows them as a
  Workspaces sidebar block at the top, clicking switches the
  active repo. Without `--root` the block stays hidden and the
  classic single-repo path-input flow is unchanged.
- Tag CRUD — the Tags sidebar grew a "New tag at HEAD" input
  and per-tag `×` delete (with confirm). Annotated tags via
  the dedicated `/api/repo/tags/create` body field.
- File-history endpoint (`GET /api/repo/log-file?file=…`),
  follows renames via `git log --follow`. UI surface for it
  is queued.
- Arbitrary-ref diff endpoint (`GET /api/repo/diff/refs?from=&to=`),
  same `FileDiff[]` shape as commit diffs. UI surface queued.
- Conflict resolution: when a merge / rebase / cherry-pick /
  revert hits a conflict, a banner above the main panel surfaces
  the in-progress state, the conflicted file list with per-file
  Use-ours / Use-theirs buttons, and Abort / Skip / Continue
  controls (Skip available for rebase / revert). Clicking a
  conflicted file in the sidebar opens a per-block view with
  side-by-side ours/theirs columns and per-block buttons —
  including "Both (ours first)" / "Both (theirs first)" — so
  mixed resolutions can be made without dropping to the CLI.
- History view: log endpoint accepts `all=true` to walk every
  local + remote-tracking branch tip — visible as an "All
  branches" toggle next to the history filter. The log block is
  scrollable on its own so 500-commit history doesn't push the
  diff panel off-screen.
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

- Streaming progress for long fetch/push (current implementation
  blocks until the git CLI completes; UI shows "Working…" the
  whole time).
- Line-level staging — checkbox per `add`/`del` line, currently
  per-hunk only.
- Server-side path clamping when `--root` is set — currently the
  workspace scanner reports discovered repos but the path-based
  endpoints still accept any absolute path. Localhost-only + the
  bearer token make this acceptable for v1.
- UI surface for file-history (`/api/repo/log-file`) and
  arbitrary-ref diff (`/api/repo/diff/refs`) — endpoints exist,
  the visible surface is the next polish pass.
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
