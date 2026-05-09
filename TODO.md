# TODO

Roadmap for `gitrust`. Strikethrough = done. Order roughly by priority
within each section.

## Diff viewer

- [x] `GET /api/repo/diff?path=…&oid=…` — tree-diff via `gix-diff`,
      blob-diff via `imara-diff` (re-exported through `gix::diff::blob`).
- [x] UI: click commit → "Commit detail" section with per-file diff.
- [x] Context lines around hunks (default 3 above / 3 below) plus
      hunk headers in `@@ -a,b +c,d @@` form.
- [x] Per-line numbers for both old and new files (gutter view).
- [x] Binary file detection (NUL byte in first 8 KiB) → `is_binary:
      true` and `hunks: []`.
- [ ] Hunk merging when two adjacent hunks' context windows overlap
      (today they're emitted independently and the redundant context
      lines repeat).
- [x] `GET /api/repo/diff/working?path=…&file=…` — patch for a
      single working-tree file (modified, untracked, or deleted)
      between index and worktree. UI: status entries in the sidebar
      are clickable; selection drives the same Detail panel that
      shows commit diffs.
- [ ] Rename-aware blob diff for `Change::Rewrite` (currently
      reported as `renamed` but `hunks: []`).
- [ ] Syntax highlighting via `syntect` or `tree-sitter`.
- [ ] Per-file collapse/expand for very large diffs; lazy load above
      a threshold.
- [ ] Side-by-side view as an alternative to the unified gutter.

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
