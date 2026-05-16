# REST API reference

All endpoints live under `/api`. Paths outside `/api` are served from
the WASM bundle directory (`--web-dist`). All responses are JSON.

The `path` query parameter on repo endpoints is an **absolute** path
to a working directory; the server runs `gix::open(path)` on it.

## Errors

Failed responses carry a structured envelope. The HTTP status maps
to one of the categories below; the JSON body always has `error`
(the raw message) and `code` (a short, stable category), plus an
optional `hint` for cases where we can guess what went wrong.

```json
{
  "error": "/nope does not appear to be a git repository",
  "code":  "repo_not_found",
  "hint":  "Check that the path points at a working tree or `.git` directory."
}
```

| status | code               | meaning                                                        |
| ------ | ------------------ | -------------------------------------------------------------- |
| 401    | (no body)          | Missing or wrong `Authorization` / `?token=`.                  |
| 403    | `permission_denied`| Filesystem refused access; surfaced from git.                  |
| 404    | `repo_not_found`   | `path` is not a git working tree.                              |
| 409    | `branch_unmerged`  | `git branch -d` refused; retry with `force: true`.             |
| 409    | `worktree_dirty`   | Checkout would overwrite uncommitted changes.                  |
| 409    | `already_exists`   | Target name (branch, file) is taken.                           |
| 400    | `bad_oid`          | Caller supplied a malformed or unknown commit oid.             |
| 400    | `generic`          | Catch-all — `error` carries the raw message; `hint` is absent. |

Codes are derived heuristically from the underlying `anyhow` /
git error string. The UI matches on `code` first when it wants to
take a specific action (e.g. show a "force delete" confirm dialog),
and falls back to `error · hint` for display.

## `GET /api/health`

Liveness/version probe.

```sh
curl http://127.0.0.1:3737/api/health
```

```json
{ "status": "ok", "version": "0.1.0" }
```

## `GET /api/repo/summary?path=<path>`

Identifies the repository at `path`.

```sh
curl 'http://127.0.0.1:3737/api/repo/summary?path=/home/me/myrepo'
```

```json
{
  "path": "/home/me/myrepo",
  "git_dir": "/home/me/myrepo/.git",
  "head_ref": "main",
  "head_oid": "85ea44373cc77f401b5ea4fc665c08e8c026fbe4",
  "is_detached": false
}
```

- `head_ref` — symbolic ref name (shortened, no `refs/heads/` prefix),
  or `null` when HEAD is detached or unborn.
- `head_oid` — full hex SHA-1, or `null` when there are no commits yet.
- `is_detached` — true when HEAD points directly at an oid.

## `GET /api/repo/log?path=<path>&limit=<N>`

Walks ancestors of HEAD. Default limit is 50, capped at 500.

```sh
curl 'http://127.0.0.1:3737/api/repo/log?path=/home/me/myrepo&limit=10'
```

```json
[
  {
    "oid": "85ea44373cc77f401b5ea4fc665c08e8c026fbe4",
    "short_oid": "85ea4437",
    "summary": "feat: bootstrap workspace",
    "body": "Five-crate Rust workspace foundation…",
    "parents": ["a5f16b79f0369228866b8ac86902bc840329ccae"],
    "author_name": "Salavat",
    "author_email": "s@example.com",
    "time_unix": 1778270159
  }
]
```

- `summary` — first line of the commit message (the title).
- `body` — everything after the title's blank-line separator. Empty
  string if the commit has no body.
- `parents` — ordered list of parent commit oids. Empty for the root
  commit; one entry for normal commits; multiple for merges.
- `time_unix` — committer time, seconds since epoch.

## `GET /api/repo/status?path=<path>`

Lists working-tree changes — the diff between the index and the
worktree.

```sh
curl 'http://127.0.0.1:3737/api/repo/status?path=/home/me/myrepo'
```

```json
[
  { "path": "src/main.rs", "kind": "modified" },
  { "path": "src/new.rs",  "kind": "untracked" },
  { "path": "Cargo.toml",  "kind": "added" }
]
```

`kind` is one of:

- `modified` — tracked file changed since the index was last written.
- `added` — index entry marked intent-to-add.
- `untracked` — file present on disk, not in the index.
- `conflict` — merge or rebase conflict.

Renames and copies are detected by gix but skipped from the response
in this version.

The list is sorted lexicographically by path.

## `GET /api/repo/staged?path=<path>`

Lists the diff between the index and HEAD — what `git diff --cached`
shows. Complements `/api/repo/status`, which only reports
worktree-vs-index. Use both together to render staged and unstaged
files in separate sidebar blocks.

```sh
curl 'http://127.0.0.1:3737/api/repo/staged?path=/home/me/myrepo'
```

```json
[
  { "path": "src/main.rs", "kind": "modified" },
  { "path": "Cargo.toml",  "kind": "added" }
]
```

`kind` is `added` | `modified` | `deleted`. Renames and copies are
collapsed to `modified` for the new path. On an unborn HEAD every
index entry is reported as `added`. Sorted lexicographically.

## `GET /api/repo/branches?path=<path>`

Lists local branches (`refs/heads/*`).

```sh
curl 'http://127.0.0.1:3737/api/repo/branches?path=/home/me/myrepo'
```

```json
[
  { "name": "main",     "oid": "a4343b78dedb1664…", "is_head": true  },
  { "name": "feature1", "oid": "11223344aabbccdd…", "is_head": false }
]
```

- `name` — short branch name (no `refs/heads/` prefix).
- `oid` — full hex SHA-1 of the commit the branch points at, or
  `null` for an unresolved symbolic ref.
- `is_head` — true for the branch HEAD currently points at. False
  for every branch when HEAD is detached.

The list is sorted lexicographically by name. Remote-tracking
branches and tags are exposed via `/api/repo/remotes` and
`/api/repo/tags` (below).

## `GET /api/repo/tags?path=<path>`

Lists tags (`refs/tags/*`).

```sh
curl 'http://127.0.0.1:3737/api/repo/tags?path=/home/me/myrepo'
```

```json
[
  { "name": "v1.0",      "oid": "abc123…", "annotated": false },
  { "name": "v2.0-rc1",  "oid": "def456…", "annotated": true  }
]
```

- `oid` — peeled commit oid for both lightweight and annotated tags.
- `annotated` — true when the tag points at a tag object that wraps
  a commit (i.e. `git tag -a` / `-s`); false for lightweight tags.

## `GET /api/repo/remotes?path=<path>`

Lists remote-tracking branches (`refs/remotes/<remote>/<branch>`).
The per-remote `HEAD` pseudo-ref is filtered out.

```sh
curl 'http://127.0.0.1:3737/api/repo/remotes?path=/home/me/myrepo'
```

```json
[
  { "name": "origin/main",    "remote": "origin",   "oid": "abc123…" },
  { "name": "upstream/main",  "remote": "upstream", "oid": "def456…" }
]
```

`name` is the short form (no `refs/remotes/` prefix); `remote` is
the leading segment so the UI can group entries by remote.

## `GET /api/repo/tree?path=<path>`

Returns the recursive file tree at HEAD as a nested list. Folders
come first, then files, alphabetical within each level.

```sh
curl 'http://127.0.0.1:3737/api/repo/tree?path=/home/me/myrepo'
```

```json
[
  {
    "name": "src",
    "path": "src",
    "kind": "tree",
    "oid": "abc123…",
    "children": [
      {
        "name": "main.rs",
        "path": "src/main.rs",
        "kind": "blob",
        "oid": "def456…"
      }
    ]
  },
  {
    "name": "Cargo.toml",
    "path": "Cargo.toml",
    "kind": "blob",
    "oid": "789abc…"
  }
]
```

- `kind` — `tree` (folder), `blob` (file), `symlink`, or `submodule`.
- `path` — full path from repository root.
- `oid` — object id (the tree or blob's oid).
- `children` — nested entries; only present (and only non-empty) on
  trees. For blobs the field is omitted.

## `GET /api/repo/blob?path=<path>&oid=<blob-oid>&file=<rel-path>`

Returns the content of one blob with per-line tree-sitter
highlighting (when the file extension maps to a supported language).
The `file` parameter is what drives language detection — pass the
relative path you'd see in the tree (e.g. `crates/gitrust-core/src/lib.rs`),
not just the basename.

```sh
curl 'http://127.0.0.1:3737/api/repo/blob?path=/home/me/myrepo&oid=abc123…&file=src/main.rs'
```

```json
{
  "path": "src/main.rs",
  "oid": "abc123…",
  "size": 1234,
  "is_binary": false,
  "lines": [
    {
      "number": 1,
      "text": "use std::path::Path;",
      "tokens": [
        { "text": "use", "class": "keyword" },
        { "text": " std", "class": "" },
        { "text": "::", "class": "punctuation.delimiter" }
      ]
    }
  ]
}
```

- `size` — blob size in bytes.
- `is_binary` — set the same way as the diff endpoints (NUL byte in
  first 8 KiB). Binary blobs come back with `lines: []`.
- `lines[].number` — 1-indexed line number.
- `lines[].tokens` — same shape as in the diff endpoint, optional
  per the same language-detection rule.

## `GET /api/repo/diff?path=<path>&oid=<commit-oid>`

Diff of a commit against its first parent (against the empty tree for
a root commit). The response includes the commit's metadata so the
UI doesn't have to fetch it separately.

```sh
curl 'http://127.0.0.1:3737/api/repo/diff?path=/home/me/myrepo&oid=85ea44…'
```

```json
{
  "commit": {
    "oid": "85ea44…",
    "short_oid": "85ea4437",
    "summary": "feat: bootstrap workspace",
    "body": "…",
    "parents": ["…"],
    "author_name": "Salavat",
    "author_email": "s@example.com",
    "time_unix": 1778270159
  },
  "files": [
    {
      "path": "src/main.rs",
      "kind": "added",
      "is_binary": false,
      "hunks": [
        {
          "old_start": 10, "old_count": 7,
          "new_start": 10, "new_count": 8,
          "lines": [
            { "kind": "ctx", "old_line": 10, "new_line": 10, "text": "use std::path::Path;" },
            { "kind": "del", "old_line": 11, "new_line": null, "text": "let x = 1;" },
            { "kind": "add", "old_line": null, "new_line": 11, "text": "let x = 2;" }
          ]
        }
      ]
    }
  ]
}
```

- `files[].kind` — `added` | `deleted` | `modified` | `renamed` | `copied`.
  Renames and copies are detected by gix's tree-diff (default 50%
  similarity threshold) and carry their full hunks against the source
  blob. The previous path is in `old_path`.
- `files[].old_path` — present only on `renamed` and `copied` entries.
  The original path of the file inside the parent commit's tree.
- `files[].is_binary` — true if either side has a NUL byte in the
  first 8 KiB. Binary files come back with `hunks: []`.
- `hunks[]` — unified-diff style hunks with three lines of context
  before and after each change. Hunk headers in standard
  `@@ -old_start,old_count +new_start,new_count @@` form.
- `hunks[].lines[].kind` — `ctx` | `add` | `del`. Context lines have
  both `old_line` and `new_line`; additions have only `new_line`;
  deletions have only `old_line`. All line numbers are 1-indexed.
- `hunks[].lines[].tokens` — optional `[{text, class}]` for syntax
  highlighting (server-side, tree-sitter). Class names follow the
  tree-sitter "highlight name" convention (`keyword`, `string`,
  `comment`, `function`, `type`, `variable.parameter`, etc.).
  Concatenating `text` over all tokens yields the same content as
  the line's `text` field. Field is omitted when the file extension
  doesn't map to a supported grammar (currently rust, json, html,
  css, typescript, tsx, javascript, python, toml, lua, markdown).
  Markdown is block-only for now: headings, lists and fenced code
  blocks are tokenised; inline `code`, *emphasis* and **strong**
  aren't surfaced — pending a manual two-grammar merge.

Tree-level entries (directories) are filtered from the response;
only file changes appear.

## `GET /api/repo/blame?path=<path>&file=<rel-path>`

Line-by-line attribution of `file` at HEAD, parsed from `git blame
--porcelain`. Each line carries the commit it last touched plus
enough metadata to render an annotation column (short oid, author
name, summary, committer time).

```sh
curl 'http://127.0.0.1:3737/api/repo/blame?path=/home/me/myrepo&file=src/main.rs' \
  -H "Authorization: Bearer $TOKEN"
```

```json
{
  "path": "src/main.rs",
  "lines": [
    {
      "line_number": 1,
      "text": "use std::path::Path;",
      "oid": "85ea44373cc77f401b5ea4fc665c08e8c026fbe4",
      "short_oid": "85ea4437",
      "author_name": "Salavat",
      "time_unix": 1778270159,
      "summary": "feat: bootstrap workspace"
    }
  ]
}
```

- `line_number` — 1-indexed.
- `oid` — the commit that last modified this line. For lines that
  are present in the worktree but not yet committed (e.g. unstaged
  modifications), this is the all-zero sentinel git uses.
- `short_oid` — first 8 hex chars; for click-to-commit affordances.
- `summary` / `author_name` / `time_unix` — copied from the commit
  header; same shape as `/api/repo/log` entries.

Renames and copies are followed by default (this is just what
`git blame` does); the response always carries the new line numbers
in the current file.

## `GET /api/repo/diff/working?path=<path>&file=<rel-path>`

Diff of one working-tree file against its index version. Returns the
same `FileDiff` shape used inside `/api/repo/diff`'s `files[]`, so the
UI can reuse the same renderer.

```sh
curl 'http://127.0.0.1:3737/api/repo/diff/working?path=/home/me/myrepo&file=src/main.rs'
```

```json
{
  "path": "src/main.rs",
  "kind": "modified",
  "is_binary": false,
  "hunks": [
    {
      "old_start": 10, "old_count": 5,
      "new_start": 10, "new_count": 7,
      "lines": [ … ]
    }
  ]
}
```

`kind` is determined by which sides exist:

- `modified` — file is in the index *and* on disk (and content differs).
- `untracked` — only on disk; rendered as all-add against the empty
  index side.
- `deleted` — in the index but missing from disk.
- `added` — index entry marked intent-to-add but no committed blob
  yet; not currently emitted by `list_status` but supported here for
  symmetry.

Same hunk shape (context lines, line-number gutter, binary
detection) as the commit diff.

## Auth

Every endpoint except `/api/health` requires a token. It's generated
on first launch and written to `$XDG_CONFIG_HOME/gitrust/token`
(mode `0600` on unix); subsequent launches reuse the same value.
The server prints the token to **stderr** during boot:

```
  gitrust ready at http://127.0.0.1:3737
  paste the access token below into the browser:

    0f1c…7a
```

Two channels accept the token:

- `Authorization: Bearer <token>` — what the in-browser UI uses for
  all `fetch` requests and what plain HTTP clients (curl, reqwest)
  should send.
- `?token=<token>` query string — the browser WebSocket API can't
  set custom headers, so `/api/repo/events` accepts the token in
  the URL. Plain HTTP also accepts it via query for symmetry.

A missing or wrong token returns `401 Unauthorized`. There is no
`/api/auth/token` endpoint — the user enters the token manually
into the UI's sign-in form.

## Write endpoints

`POST /api/repo/stage`, `/api/repo/unstage`, and `/api/repo/commit`
share the same auth gate as everything else above (it just makes
no sense to talk about writes without it).

### `POST /api/repo/stage`

```sh
curl -X POST http://127.0.0.1:3737/api/repo/stage \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"path":"/home/me/myrepo","files":["src/main.rs","README.md"]}'
```

Returns `204 No Content` on success. Empty `files` is a no-op,
not an error.

### `POST /api/repo/unstage`

Same shape as `/stage`. Returns `204 No Content`. Reverts the
index entries for `files` to whatever HEAD has (or drops them
entirely for paths HEAD doesn't carry yet).

### `POST /api/repo/commit`

```sh
curl -X POST http://127.0.0.1:3737/api/repo/commit \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "path":"/home/me/myrepo",
    "message":"feat: ship the thing\n\nLonger body if you want one.",
    "author":"Ghost Writer <ghost@example.com>"
  }'
```

```json
{ "oid": "85ea44373cc77f401b5ea4fc665c08e8c026fbe4" }
```

Whatever's currently in the index becomes the new commit. Empty
or whitespace-only messages return 400. Newlines inside `message`
are preserved — git treats the first line as the subject and the
rest as the body. `author` (optional) overrides the committer
identity in `Name <email>` form; omit it to inherit from the
repo's gitconfig.

### `GET /api/repo/commit?path=<path>&oid=<commit-oid>`

Returns the same `CommitInfo` shape as `/api/repo/log` entries
for a single commit looked up by oid — useful for permalinks
without re-walking history.

```sh
curl 'http://127.0.0.1:3737/api/repo/commit?path=/home/me/myrepo&oid=85ea44…' \
  -H "Authorization: Bearer $TOKEN"
```

```json
{
  "oid": "85ea44373cc77f401b5ea4fc665c08e8c026fbe4",
  "short_oid": "85ea4437",
  "summary": "feat: bootstrap workspace",
  "body": "…",
  "parents": ["…"],
  "author_name": "Salavat",
  "author_email": "s@example.com",
  "time_unix": 1778270159
}
```

A garbage oid returns 400 with a descriptive error envelope.

All three endpoints return 401 on a missing or wrong `Authorization`
header and 400 with the usual `{ "error": "…" }` envelope on a
git-level failure (no staged changes, dirty index, etc.).

### `POST /api/repo/discard`

Same body shape as `/stage`. Reverts each path in the worktree
to whatever's in the index (`git restore -- <files>`). Untracked
files don't have an index entry; passing them returns 400.

### `POST /api/repo/branches/create`

```json
{ "path": "...", "name": "feature", "from": null, "switch": true }
```

`from` is the start revision — branch name, tag, or commit oid;
`null` (or omitted) means current HEAD. `switch: true` checks
out the new branch in the same step (`git checkout -b`); `false`
just creates it.

### `POST /api/repo/branches/delete`

```json
{ "path": "...", "name": "feature", "force": false }
```

`force: false` (default) is `git branch -d` — refuses to drop
an unmerged branch and surfaces the standard `not fully merged`
error. `force: true` is `git branch -D` and drops it
unconditionally. The UI tries safe first; on the unmerged-branch
error it offers a `confirm()` dialog and re-posts with
`force: true`.

### `POST /api/repo/branches/rename`

```json
{ "path": "...", "old": "feature", "new": "feature-v2" }
```

`git branch -m <old> <new>`. Refuses when `<new>` already
exists.

### `POST /api/repo/checkout`

```json
{ "path": "...", "target": "feature" }
```

`target` may be any rev git understands: branch name, tag, or
commit oid (detached HEAD). Refuses when local changes would be
overwritten.

### `POST /api/repo/pick-folder` (desktop only)

Pops the OS-native folder picker (`NSOpenPanel` on macOS, the
GTK portal on Linux, `IFileDialog` on Windows) and returns the
chosen path. Body is ignored — accepts `null` for symmetry with
the rest of the POST surface.

```json
{ "path": "/home/me/projects/x" }
```

`path` is `null` when the user cancels. Only mounted when the
server was built with `--features desktop` (it's the
\`gitrust app\` flavour); plain `gitrust serve` returns 404.

The dialog is the official way around macOS TCC: when the user
picks a folder, the OS grants the in-process app
read-and-write access for the rest of the session without any
entitlement-plist plumbing on our side.

## `GET /api/repo/events?path=<path>` (WebSocket)

Live filesystem-event stream for one repo. Clients send the standard
HTTP/1.1 Upgrade headers (`Upgrade: websocket`, `Sec-WebSocket-Key`,
etc.); the server responds with 101 Switching Protocols and from
that point pushes JSON text frames whenever the worktree changes.

```sh
# In Python (stdlib only, raw frames):
python3 -c '
import websockets.sync.client as ws
with ws.connect("ws://127.0.0.1:3737/api/repo/events?path=/home/me/myrepo") as s:
    for msg in s:
        print(msg)
'
```

```json
{ "kind": "worktree_changed" }
```

`kind` is one of:

- `head_changed` — `.git/HEAD` was written (e.g. `git checkout`).
- `refs_changed` — a ref under `.git/refs/` (or `.git/packed-refs`)
  was written (commit on the current branch, tag create, fetch).
- `index_changed` — `.git/index` was written (`git add`, `reset`).
- `worktree_changed` — a file outside `.git/` was modified, created
  or removed.

Frames are debounced server-side (150 ms window, deduplicated by
kind) so a single high-level operation like `git commit` produces
at most one frame per kind rather than the dozen raw inotify
events underneath. Noisy paths are filtered out before
classification: `.git/objects/`, `.git/lfs/`, and any `*.lock`.

The watcher walks the worktree at connection time and adds a
per-directory `NonRecursive` watch, skipping `target/`,
`node_modules/`, `.direnv/`, `.venv/` so it doesn't blow past
`max_user_watches` on real-world repos. New directories created
after connection aren't watched until the client reconnects;
clients should treat the WebSocket as best-effort and keep a
short polling fallback for missed events.
