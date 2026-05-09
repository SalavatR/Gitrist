# REST API reference

All endpoints live under `/api`. Paths outside `/api` are served from
the WASM bundle directory (`--web-dist`). All responses are JSON.

The `path` query parameter on repo endpoints is an **absolute** path
to a working directory; the server runs `gix::open(path)` on it.

## Errors

Any endpoint that fails returns HTTP 400 with:

```json
{ "error": "<message>" }
```

The message comes from the underlying `anyhow::Error` chain — usually
a gix error such as `"<path>" does not appear to be a git repository`.

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
branches (`refs/remotes/*`) and tags are not yet exposed.

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

- `files[].kind` — `added` | `deleted` | `modified` | `renamed`. Renames
  carry the `renamed` kind from gix's tree-diff but no `hunks` yet —
  rename-aware blob diff is on the TODO.
- `files[].is_binary` — true if either side has a NUL byte in the
  first 8 KiB. Binary files come back with `hunks: []`.
- `hunks[]` — unified-diff style hunks with three lines of context
  before and after each change. Hunk headers in standard
  `@@ -old_start,old_count +new_start,new_count @@` form.
- `hunks[].lines[].kind` — `ctx` | `add` | `del`. Context lines have
  both `old_line` and `new_line`; additions have only `new_line`;
  deletions have only `old_line`. All line numbers are 1-indexed.

Tree-level entries (directories) are filtered from the response;
only file changes appear.

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
