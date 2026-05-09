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
