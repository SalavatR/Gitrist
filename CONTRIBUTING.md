# Contributing to gitrust

Thanks for the interest. This is an early-stage project — patches and
issues are welcome.

## Quickstart

```sh
make setup    # rustup target add wasm32-unknown-unknown + cargo install wasm-bindgen-cli
make web      # build the Dioxus UI to crates/gitrust-web/dist/
make run      # start the server on 127.0.0.1:3737 against the freshly-built UI
```

The server prints the access token at startup; paste it into the
browser when the sign-in screen appears. Subsequent runs reuse the
same token (stored at `$XDG_CONFIG_HOME/gitrust/token`).

See [docs/build.md](docs/build.md) for the full toolchain layout
(wasm-bindgen pinning, desktop deps, etc.).

## Workspace layout

Six crates under `crates/`. The dependency direction is enforced by
the workspace graph — see [docs/architecture.md](docs/architecture.md)
for the layering diagram and the wasm/native split.

| Crate            | Role                                                              |
| ---------------- | ----------------------------------------------------------------- |
| `gitrust-types`  | Wire shapes (`Serialize`/`Deserialize`), shared by server + UI.   |
| `gitrust-core`   | Git operations via `gix` + shell-out to `git` for writes.         |
| `gitrust-server` | axum HTTP API + `/api/repo/events` WebSocket. Auth-gates writes.  |
| `gitrust-ui`     | Dioxus components, split into `state`/`fetch`/`ws`/`sidebar`/…   |
| `gitrust-web`    | wasm32 entry binary that mounts the UI.                           |
| `gitrust`        | CLI binary with `serve` and `app` subcommands.                    |

## Checks before pushing

The CI gate runs these — running them locally avoids the round-trip:

```sh
cargo fmt --all --check
cargo clippy --workspace --exclude gitrust-web --all-targets -- -D warnings
cargo clippy -p gitrust-web --target wasm32-unknown-unknown -- -D warnings
cargo test  --workspace --exclude gitrust-web
cargo check -p gitrust-web --target wasm32-unknown-unknown
```

The `desktop` feature (wry + tao) needs `webkit2gtk-4.1-dev` +
`libsoup-3.0-dev` on Linux; macOS and Windows pick up the system
webview automatically. CI runs `cargo check -p gitrust --features
desktop` on all three OSes.

## Tests

- Read-API and write-API integration tests live in
  `crates/gitrust-core/tests/repo_ops.rs` and
  `crates/gitrust-server/tests/api.rs`. Each test creates a fresh
  git repo in a tempdir via the system `git` CLI, so the binary
  has to be available — true for every CI image we use.
- WebSocket smoke is in `crates/gitrust-server/tests/ws.rs`. It
  opens `/api/repo/events`, touches a file, and asserts a frame
  arrives within the debounce window.
- New API endpoints get a happy-path test plus an auth-gate test
  (a no-token request → 401).

## Commit style

Conventional-commits, lower-case scope:

```
feat(write): branch create / delete / checkout + worktree discard
fix(server): Cache-Control: no-store on all /api responses
docs: refresh README, architecture, and api for current state
test(server): API + WebSocket integration tests
```

A short subject (≤70 chars) plus a body that explains the *why*
rather than the *what* — git diffs already show the latter. The
trailers `Co-Authored-By` are welcome.

## License

By contributing, you agree your contributions are dual-licensed
under MIT or Apache-2.0, matching the rest of the repository.
