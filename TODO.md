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
- [x] Hunk merging when two adjacent hunks' context windows overlap.
      Adjacent imara hunks with overlapping `±3` windows are folded
      into one display hunk; output now matches `git diff -U3` hunk
      counts on the same commit.
- [x] `GET /api/repo/diff/working?path=…&file=…` — patch for a
      single working-tree file (modified, untracked, or deleted)
      between index and worktree. UI: status entries in the sidebar
      are clickable; selection drives the same Detail panel that
      shows commit diffs.
- [x] Rename-aware blob diff for `Change::Rewrite`. Tree-diff is now
      run with `track_rewrites(Some(Rewrites::default()))`; renamed
      and copied files carry full hunks against their source blob and
      a new `FileDiff.old_path` field. UI shows `old → new` in the
      file header (old path strikethrough'd in muted color).
- [x] Syntax highlighting via `tree-sitter` + `tree-sitter-highlight`,
      server-side. Tokens travel through the API as
      `DiffLine.tokens: Option<Vec<{text, class}>>`. Languages: rust,
      json, html, css, typescript, tsx, javascript, python, toml, lua,
      markdown (block-only — inline emphasis/code/links not yet wired,
      see below).
- [x] Markdown inline highlighting via a two-pass merge. The block
      grammar still runs (heading hashes, list bullets, fences) and
      the inline grammar runs over the whole document in a second
      pass — `merge_md_passes` walks both token streams char-by-char
      per line and prefers the inline class wherever non-empty. The
      injection-chain alternative still doesn't surface inline
      events through our tree-sitter-highlight version, so this is
      the practical workaround.
- [x] Per-file collapse/expand for very large diffs (auto-collapse
      above 300 lines, manual via the file-header chevron).
- [x] Side-by-side view as an alternative to the unified gutter.
      Toolbar toggle, persisted in `localStorage["gitrust.view_mode"]`.

## Reads (more history surface)

- [x] Remote-tracking branches (`refs/remotes/*`). Separate endpoint
      `/api/repo/remotes`, sidebar block. Per-remote HEAD filtered.
- [x] Tags (`refs/tags/*`). Separate endpoint `/api/repo/tags`,
      sidebar block. Annotated vs lightweight detected via
      `peel_to_id` comparison.
- [x] File tree at HEAD: `/api/repo/tree?path=…` returns nested
      `TreeEntry` with kind/oid/children. UI sidebar shows a
      browsable file-tree block; folders are `<details>` you can
      open/close, files are leaves with kind-aware glyphs.
- [x] File viewer: clicking a file in the tree opens it in the
      Detail panel with line-numbered, tree-sitter-highlighted
      content. New `/api/repo/blob` endpoint serves blobs with
      per-line tokens. Mutually exclusive with commit / working-tree
      selections — clicking one always clears the others.
- [x] Commit-by-oid: `GET /api/repo/commit?path=…&oid=…` returns
      the same `CommitInfo` shape as `/api/repo/log` entries but
      resolved directly via gix — useful for permalinks without
      re-walking history.
- [x] Blame: `/api/repo/blame?path=…&file=…` shells out to `git
      blame --porcelain` and returns `BlameLine` per row
      (line_number, text, oid, short_oid, author_name,
      time_unix, summary). UI's file viewer pairs every row in
      `BlobView.lines` with the matching blame entry by line
      number and renders an annotation column to the left of
      the line number gutter. Uncommitted lines come back with
      git's all-zero oid sentinel and get an "uncommitted"
      treatment.

## Writes (need auth first)

- [x] Auth: 32-byte random token generated at first launch, written
      to `$XDG_CONFIG_HOME/gitrust/token` (mode 0600 on unix). The
      UI fetches it via `GET /api/auth/token` and rides it as
      `Authorization: Bearer <token>` on all writes. Reads stay
      open (server is `localhost`-only by default).
- [x] `POST /api/repo/stage`, `POST /api/repo/unstage` — JSON body
      `{ path, files }`. Sidebar "Working tree" gets a `+` button
      per file and "Staged" gets `−` buttons; both hidden behind
      the bearer gate.
- [x] `POST /api/repo/commit { path, message }`. Returns `{ oid }`.
      Author identity comes from the repo's gitconfig. UI exposes
      it as a textarea + Commit button between the summary card
      and the history table; disabled when nothing is staged.
- [x] Branch ops: create (with optional `switch=true` for
      `git checkout -b`), delete (safe + `force` field for
      `git branch -D`), rename, and checkout. Sidebar branch list
      grows hover-revealed `✎` (rename via `prompt()`), `→`
      (switch), and `×` (delete) buttons. Delete tries safe first
      and falls back to a browser `confirm()` "force delete?"
      prompt on the unmerged-branch error. Plus a `New branch`
      input + Create button at the bottom that does
      create-and-switch in one shot.
- [x] Discard worktree changes for a file (`git restore <file>`).
      `POST /api/repo/discard` and a `↺` button next to each
      "Working tree" entry, hover-revealed alongside the stage `+`.
- [x] Commit body / author override. Body works via newlines in
      `message` (git uses the first line as the subject). Author is
      an optional `author` field on POST commit, forwarded as
      `--author=<Name <email>>`. UI exposes it as a small
      "Author override — Name <email>" input below the message
      textarea; empty value falls through to the repo's gitconfig
      identity.

## UX

- [x] Persist last-used repo path in `localStorage` (`gitrust.repo`);
      first load reads it, falling back to `DEFAULT_REPO` only on
      a fresh install.
- [x] Recent-repos quick-switch list — every time `current_repo`
      changes, the path is prepended (deduped, capped at 8) to
      `gitrust.repos` and rendered as a `<datalist>` next to the
      repo input. Browser-native suggestions.
- [x] Encode current repo path in URL hash so browser back/forward
      and bookmarks work. `persist_repo` writes both localStorage
      and `window.location.hash` (URL-encoded).
- [x] Stale-while-revalidate: mirror Signals shadow the three
      main-panel resources; clicks on commits/files/blobs hold the
      previous content visible until the new fetch lands.
- [x] Time column: relative ("3h ago") in the log; absolute RFC3339
      goes into the `title` attribute for hover. Detail-panel still
      shows absolute since you've drilled in for precision.
- [x] Error states with a retry button. Each `render_*` with an
      error branch takes the underlying `Resource<…>` by value and
      renders an inline `Retry` button that calls `.restart()` on
      click.
- [x] Manual light/dark toggle in topbar. Cycles Auto → Light →
      Dark; choice persists in `localStorage` and reflects on
      `<html data-theme=…>`. Auto falls back to
      `prefers-color-scheme`.

## Native shell

- [x] `gitrust app` opens a desktop window via `wry+tao` pointing at
      the embedded server. Server runs in a tokio task; window owns
      the event loop. Embedded WASM bundle via `include_dir!` so the
      release binary is self-contained.
- [x] Cross-platform fallback. When the `desktop` feature isn't
      compiled in, or `DISPLAY`/`WAYLAND_DISPLAY` aren't set on
      Linux, or wry init fails, prints the URL and serves
      indefinitely so the user opens it in a browser.
- [x] CI matrix building default + `desktop`-feature configurations
      on ubuntu / macos / windows. Linux desktop job installs
      webkit2gtk-4.1 + libsoup-3.0.
- [x] Native menu bar via `muda`: File → Quit (Cmd-Q / Ctrl-Q)
      and View → Reload (Cmd-R / Ctrl-R). Platform glue: macOS
      `init_for_nsapp`, Linux `init_for_gtk_window` against
      tao's `WindowExtUnix::gtk_window()`, Windows
      `init_for_hwnd`. Errors during init are non-fatal — the
      keyboard handler below still serves as fallback. File →
      Open Repo and a full About item are still pending.
- [x] System keyboard shortcuts handled inside `gitrust app`'s
      event loop: Cmd-R / Ctrl-R reloads the webview, Cmd-Q /
      Ctrl-Q and Cmd-W / Ctrl-W exit. Modifier key follows the
      `cfg!(target_os = "macos")` convention so the shortcuts
      feel right per platform.
- [x] Pre-built release binaries via a dedicated GitHub Actions
      release workflow. Push `vX.Y.Z` and the matrix builds
      `gitrust --features desktop` for `aarch64-apple-darwin`,
      `x86_64-apple-darwin`, and `x86_64-pc-windows-msvc`, packages
      each with README + LICENSE files, and attaches the archive to
      the GitHub Release. `workflow_dispatch` runs the same matrix
      but only as workflow artifacts. A Linux binary is still
      pending — needs the webkit2gtk-4.1 ABI pin to be useful
      across distros.

## Infrastructure

- [x] `gitrust-types` crate: wire-shape structs
      (`RepoSummary`, `CommitInfo`, `StatusEntry`, `BranchInfo`,
      `FileDiff`, `BlobView`, …) shared between server and UI.
      Pure serde, target-independent.
- [x] Push-based refresh: server watches FS via `notify` and pushes
      debounced event kinds (`head_changed`, `refs_changed`,
      `index_changed`, `worktree_changed`) over `/api/repo/events`
      WebSocket. UI opens one socket per repo (keyed on
      `current_repo` through `use_resource`, so swapping the repo
      drops the previous socket) and dispatches each kind to the
      affected `use_resource.restart()`s. The watcher walks the
      worktree manually with a skip-list (`target/`, `node_modules/`,
      `.direnv/`, `.venv/`, `.git/objects/`, `.git/lfs/`) so it
      doesn't blow past inotify's `max_user_watches`. Reconnects
      with exponential backoff (500 ms → 30 s) on disconnect. The
      previous 2 s/10 s polling stays as a silent fallback.
- [ ] Structured error envelope: `{ error, code, hint? }` instead of
      a free-form message.
- [x] CI: cargo check (native + wasm32), `fmt --check`, `clippy -D
      warnings`. GitHub Actions on Linux/macOS/Windows + GitLab
      mirror on Linux.
- [x] Tests: `gitrust-core` has integration tests for every read-API
      function (10 tests, fresh repo per case via tempdir + `git`
      CLI). `gitrust-server` adds 7 HTTP tests through `reqwest`
      against a spawned axum server on an ephemeral port, plus a
      WebSocket test that opens `/api/repo/events`, touches a
      worktree file, and asserts a `worktree_changed` frame
      arrives within the debounce window. UI snapshot tests not
      yet (no Dioxus story support that I've found).

## Cleanup / nice-to-have

- [x] Rename/copy detection in `list_status` and `list_staged`.
      `Item::Rewrite` from gix now becomes a "renamed"/"copied"
      `StatusEntry` with `old_path` populated; the porcelain
      parser for `list_staged` parses `R<score>` / `C<score>`
      lines into the same shape. Sidebar shows `old → new` next
      to the badge, styled like the diff viewer's rename row.
- [x] LICENSE-MIT and LICENSE-APACHE files at the repo root,
      matching the `MIT OR Apache-2.0` workspace declaration.
      Also picked up by the release workflow's archive packager.
- [x] CONTRIBUTING.md with the quickstart, the CI gate
      commands, commit-style notes, and the dual-license
      contribution clause.
