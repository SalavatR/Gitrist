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
