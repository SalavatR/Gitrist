# TODO

Roadmap for `gitrust`. Strikethrough = done. Order roughly by priority
within each section.

## Diff viewer

- [x] `GET /api/repo/diff?path=…&oid=…` — diff between a commit and
      its first parent (empty tree for the root commit). Backed by
      `gix-diff::tree` + `gix::object::blob::diff::Platform::lines`.
- [x] UI: click a commit row → "Commit detail" section below the log
      shows full body, parents, and per-file diff with green/red
      colored add/delete lines.
- [ ] Context lines around hunks (currently changes only — no
      surrounding unchanged lines). Needs a different gix-diff entry
      point or imara-diff's `UnifiedDiff` printer with a context
      config.
- [ ] `GET /api/repo/diff/working?path=…&file=…` — patch for a
      single working-tree file (modified or untracked); UI shows it
      inline with the status entry that's clicked.
- [ ] Rename-aware blob diff for `Change::Rewrite` (currently the
      file is reported as `renamed` but its `lines` array is empty).
- [ ] Syntax highlighting via `syntect` or `tree-sitter`.
- [ ] Per-file collapse/expand for very large diffs.

## Reads (more history surface)

- [ ] Remote-tracking branches (`refs/remotes/*`) in
      `/api/repo/branches`, grouped by remote.
- [ ] Tags (`refs/tags/*`) — separate endpoint or extend branches.
- [ ] File tree at a commit/HEAD: `/api/repo/tree?path=…&oid=…`.
- [ ] Commit-by-oid: `/api/repo/commit?path=…&oid=…` for permalinks.
- [ ] Blame: `/api/repo/blame?path=…&file=…`.

## Writes (need auth first)

- [ ] Auth: signed cookie generated at first launch, required for all
      write endpoints. Reads stay open (server is `localhost`-only
      by default).
- [ ] `POST /api/repo/stage`, `POST /api/repo/unstage`.
- [ ] `POST /api/repo/commit { message, author? }`.
- [ ] Branch ops: create, rename, delete, checkout.
- [ ] Discard worktree changes for a file.

## UX

- [ ] Persist last-used repo path in `localStorage`; use it on load
      instead of the hardcoded `DEFAULT_REPO`.
- [ ] Recent-repos quick-switch list.
- [ ] Encode current repo path in URL hash so browser back/forward
      and bookmarks work.
- [ ] Stale-while-revalidate: keep last good response visible during
      refetch instead of flipping back to "Loading…".
- [ ] Time column: relative ("3h ago") with full ISO in title.
- [ ] Error states with a retry button.
- [ ] Manual light/dark toggle in addition to `color-scheme: light dark`.

## Native shell (env-blocked here, OK on real desktop)

- [ ] `gitrust app` opens a desktop window via `wry+tao` pointing at
      the embedded server. Server runs in a tokio task; window owns
      the event loop. Needs `webkit2gtk` + a display — not viable in
      this PRoot Android dev chroot.

## Infrastructure

- [ ] `gitrust-types` crate: extract `RepoSummary`, `CommitInfo`,
      `StatusEntry`, `BranchInfo` (and future shapes) from both core
      and ui into a shared, target-independent crate. Trigger when
      duplication starts hurting.
- [ ] Push-based refresh: server watches FS via `notify`; UI gets a
      WebSocket and refetches on change rather than polling.
- [ ] Structured error envelope: `{ error, code, hint? }` instead of
      a free-form message.
- [ ] CI: cargo check (native + wasm32), `fmt --check`, `clippy -D warnings`.
- [ ] Tests: integration tests for API (spawn server, hit it). UI
      snapshot tests if Dioxus exposes a story for that.

## Cleanup / nice-to-have

- [ ] Rename/copy detection in `list_status` (currently `Item::Rewrite`
      is dropped silently).
- [ ] LICENSE-MIT and LICENSE-APACHE files at release time.
- [ ] CONTRIBUTING.md when external contributors land.
