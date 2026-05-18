pub mod highlight;

use std::path::Path;

use gix::bstr::ByteSlice;
use gix::diff::blob::{Algorithm, InternedInput, Token as IDToken};

pub use gitrust_types::{
    BlameLine, BlameView, BlobLine, BlobView, BranchInfo, CommitDiff, CommitInfo, ConflictBlock,
    ConflictView, DiffHunk, DiffLine, FileDiff, NetworkOpResult, OpProgress, RemoteBranchInfo,
    RepoEntry, RepoState, RepoSummary, StashEntry, StatusEntry, TagInfo, Token, TreeEntry,
};

fn build_commit_info(repo: &gix::Repository, oid: gix::ObjectId) -> anyhow::Result<CommitInfo> {
    let oid_str = oid.to_string();
    let commit = repo.find_object(oid)?.try_into_commit()?;
    let author = commit.author()?;
    let message = commit.message()?;
    let summary = message.summary().to_string();
    let body = message.body.map(|b| b.to_string()).unwrap_or_default();
    let parents: Vec<String> = commit.parent_ids().map(|id| id.to_string()).collect();
    let time = commit.time()?;
    Ok(CommitInfo {
        short_oid: oid_str.chars().take(8).collect(),
        oid: oid_str,
        summary,
        body,
        parents,
        author_name: author.name.to_string(),
        author_email: author.email.to_string(),
        time_unix: time.seconds,
    })
}

/// Default walk depth for `scan_root`. Five levels covers the usual
/// `~/projects/<group>/<repo>/...` layout without descending into deep
/// monorepos.
pub const DEFAULT_SCAN_DEPTH: usize = 5;

/// Walk `root` recursively up to `max_depth` and collect every git
/// working tree found. Stops descending whenever it hits a `.git/`
/// directory (so the contents of `.git/objects` are never scanned),
/// and skips symlinked subdirectories so the scan can't loop. Each
/// entry carries the path, the last path component as a display name,
/// and a snapshot of `HEAD` for the UI workspace switcher.
///
/// On `gix::open` errors for an individual entry (e.g. a directory
/// that's a `.git` but doesn't open cleanly) the entry is dropped
/// rather than aborting the whole scan — partial results are more
/// useful than a single broken repo killing discovery.
pub fn scan_root(root: &Path, max_depth: usize) -> anyhow::Result<Vec<RepoEntry>> {
    if !root.is_dir() {
        anyhow::bail!("scan root is not a directory: {}", root.display());
    }
    let mut found: Vec<RepoEntry> = Vec::new();
    scan_dir(root, max_depth, &mut found);
    // Sort by display name so the UI ordering is stable and obvious.
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

fn scan_dir(dir: &Path, depth_left: usize, out: &mut Vec<RepoEntry>) {
    // A directory is a git working tree if it contains a `.git` entry
    // (either a real dir or a `gitdir:` file for worktrees).
    let git_marker = dir.join(".git");
    if git_marker.exists() {
        if let Ok(summary) = summarize_repo(dir) {
            let name = dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| dir.display().to_string());
            out.push(RepoEntry {
                path: dir.display().to_string(),
                name,
                head_ref: summary.head_ref,
                head_oid: summary.head_oid,
            });
        }
        // Don't descend into a repo — nested submodules / fixtures
        // would balloon the listing.
        return;
    }
    if depth_left == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        // Skip symlinks so a recursive symlink can't trap the walker.
        let Ok(meta) = entry.file_type() else {
            continue;
        };
        if meta.is_symlink() || !meta.is_dir() {
            continue;
        }
        if let Some(name) = p.file_name().and_then(|n| n.to_str())
            && (name.starts_with('.') || name == "node_modules" || name == "target")
        {
            // Skip dotfiles (incl. `.git` if a stray one), package
            // caches, build artifacts — none of these host repos.
            continue;
        }
        scan_dir(&p, depth_left - 1, out);
    }
}

pub fn summarize_repo(path: &Path) -> anyhow::Result<RepoSummary> {
    let repo = gix::open(path)?;
    let head_name = repo.head_name()?.map(|n| n.shorten().to_string());
    let head_oid = repo.head_id().ok().map(|id| id.to_string());
    let is_detached = head_name.is_none() && head_oid.is_some();
    Ok(RepoSummary {
        path: path.display().to_string(),
        git_dir: repo.git_dir().display().to_string(),
        head_ref: head_name,
        head_oid,
        is_detached,
    })
}

/// Walk commit history and return up to `limit` matching commits.
///
/// `all = false` walks HEAD's ancestors (`git log` default). `all = true`
/// walks every ref tip — local branches plus remote-tracking branches —
/// merged into one stream sorted newest-first by commit time (`git log
/// --all`). Tips are deduped so a commit reachable from multiple refs
/// appears once.
///
/// When `query` is `Some(non-empty)`, a commit only counts if the
/// lowercased query appears in its summary, body, author name, or as
/// a prefix of its oid. Walking is capped at `MAX_LOG_WALK` so a rare
/// query doesn't iterate the entire history.
pub fn log_commits(
    path: &Path,
    limit: usize,
    query: Option<&str>,
    all: bool,
) -> anyhow::Result<Vec<CommitInfo>> {
    use gix::revision::walk::Sorting;
    use gix::traverse::commit::simple::CommitTimeOrder;

    const MAX_LOG_WALK: usize = 5_000;

    let needle = query
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());

    let repo = gix::open(path)?;

    let tips: Vec<gix::ObjectId> = if all {
        let mut t: Vec<gix::ObjectId> = Vec::new();
        let refs = repo.references()?;
        for r in refs.local_branches()? {
            let r = r.map_err(|e| anyhow::anyhow!("{e}"))?;
            if let Some(id) = r.try_id() {
                t.push(id.detach());
            }
        }
        for r in refs.remote_branches()? {
            let r = r.map_err(|e| anyhow::anyhow!("{e}"))?;
            if let Some(id) = r.try_id() {
                t.push(id.detach());
            }
        }
        // Ensure HEAD's tip is in the set even on detached HEAD, where
        // no local ref points at it.
        if let Ok(head) = repo.head_id() {
            t.push(head.detach());
        }
        t.sort();
        t.dedup();
        t
    } else {
        vec![repo.head_id()?.detach()]
    };

    if tips.is_empty() {
        return Ok(Vec::new());
    }

    let walk = repo
        .rev_walk(tips)
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .all()?;
    let mut commits = Vec::with_capacity(limit.min(64));
    for item in walk.take(MAX_LOG_WALK) {
        if commits.len() >= limit {
            break;
        }
        let info = item?;
        let entry = build_commit_info(&repo, info.id)?;
        let keep = match &needle {
            Some(q) => {
                entry.summary.to_lowercase().contains(q)
                    || entry.body.to_lowercase().contains(q)
                    || entry.author_name.to_lowercase().contains(q)
                    || entry.oid.starts_with(q)
            }
            None => true,
        };
        if keep {
            commits.push(entry);
        }
    }
    Ok(commits)
}

pub fn list_status(path: &Path) -> anyhow::Result<Vec<StatusEntry>> {
    use gix::status::index_worktree::Item;
    use gix::status::plumbing::index_as_worktree::EntryStatus;

    let repo = gix::open(path)?;
    let platform = repo.status(gix::progress::Discard)?;
    let iter = platform.into_index_worktree_iter(Vec::<gix::bstr::BString>::new())?;

    let mut out = Vec::new();
    for item in iter {
        let item = item?;
        match item {
            Item::Modification {
                rela_path, status, ..
            } => {
                let kind = match status {
                    EntryStatus::Conflict { .. } => "conflict",
                    EntryStatus::Change(_) => "modified",
                    EntryStatus::IntentToAdd => "added",
                    EntryStatus::NeedsUpdate(_) => continue,
                };
                out.push(StatusEntry {
                    path: rela_path.to_str_lossy().into_owned(),
                    kind: kind.into(),
                    old_path: None,
                });
            }
            Item::DirectoryContents { entry, .. } => {
                out.push(StatusEntry {
                    path: entry.rela_path.to_str_lossy().into_owned(),
                    kind: "untracked".into(),
                    old_path: None,
                });
            }
            Item::Rewrite {
                source,
                dirwalk_entry,
                copy,
                ..
            } => {
                let new_path = dirwalk_entry.rela_path.to_str_lossy().into_owned();
                let old_path = source.rela_path().to_str_lossy().into_owned();
                out.push(StatusEntry {
                    path: new_path,
                    kind: if copy { "copied" } else { "renamed" }.into(),
                    old_path: Some(old_path),
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

pub fn list_tags(path: &Path) -> anyhow::Result<Vec<TagInfo>> {
    let repo = gix::open(path)?;
    let refs = repo.references()?;
    let mut tags = Vec::new();
    for r in refs.tags()? {
        let mut r = r.map_err(|e| anyhow::anyhow!("{e}"))?;
        let full = r.name().as_bstr().to_string();
        let short = full.strip_prefix("refs/tags/").unwrap_or(&full).to_string();
        let direct_id = r.try_id().map(|id| id.to_string());
        let peeled = r.peel_to_id().ok().map(|id| id.to_string());
        let annotated = direct_id.is_some() && peeled.as_ref() != direct_id.as_ref();
        let oid = peeled.or(direct_id);
        tags.push(TagInfo {
            name: short,
            oid,
            annotated,
        });
    }
    tags.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(tags)
}

pub fn list_remote_branches(path: &Path) -> anyhow::Result<Vec<RemoteBranchInfo>> {
    let repo = gix::open(path)?;
    let refs = repo.references()?;
    let mut remotes = Vec::new();
    for r in refs.remote_branches()? {
        let r = r.map_err(|e| anyhow::anyhow!("{e}"))?;
        let full = r.name().as_bstr().to_string();
        let short = full
            .strip_prefix("refs/remotes/")
            .unwrap_or(&full)
            .to_string();
        // Skip the per-remote HEAD pseudo-ref (e.g. refs/remotes/origin/HEAD).
        if short.ends_with("/HEAD") {
            continue;
        }
        let remote = short
            .split_once('/')
            .map(|(r, _)| r.to_string())
            .unwrap_or_default();
        let oid = r.try_id().map(|id| id.to_string());
        remotes.push(RemoteBranchInfo {
            name: short,
            remote,
            oid,
        });
    }
    remotes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(remotes)
}

pub fn list_branches(path: &Path) -> anyhow::Result<Vec<BranchInfo>> {
    let repo = gix::open(path)?;
    let head_full = repo.head_name()?.map(|n| n.as_bstr().to_string());

    let mut branches = Vec::new();
    let refs = repo.references()?;
    for r in refs.local_branches()? {
        let r = r.map_err(|e| anyhow::anyhow!("{e}"))?;
        let full = r.name().as_bstr().to_string();
        let short = full
            .strip_prefix("refs/heads/")
            .unwrap_or(&full)
            .to_string();
        let oid = r.try_id().map(|id| id.to_string());
        let is_head = head_full.as_deref() == Some(full.as_str());
        branches.push(BranchInfo {
            name: short,
            oid,
            is_head,
        });
    }
    branches.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(branches)
}

pub fn list_tree(repo_path: &Path) -> anyhow::Result<Vec<TreeEntry>> {
    let repo = gix::open(repo_path)?;
    let head_tree = repo.head_tree()?;
    walk_tree(&repo, &head_tree, "")
}

fn walk_tree(
    repo: &gix::Repository,
    tree: &gix::Tree<'_>,
    prefix: &str,
) -> anyhow::Result<Vec<TreeEntry>> {
    let mut entries: Vec<TreeEntry> = Vec::new();
    for entry_res in tree.iter() {
        let entry = entry_res?;
        let name = entry.filename().to_str_lossy().into_owned();
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let mode = entry.mode();
        let oid = entry.id().to_string();
        let kind: &str = if mode.is_tree() {
            "tree"
        } else if mode.is_link() {
            "symlink"
        } else if mode.is_commit() {
            "submodule"
        } else {
            "blob"
        };
        let children = if mode.is_tree() {
            let subtree = repo.find_object(entry.id().detach())?.try_into_tree()?;
            walk_tree(repo, &subtree, &path)?
        } else {
            Vec::new()
        };
        entries.push(TreeEntry {
            name,
            path,
            kind: kind.into(),
            oid,
            children,
        });
    }
    // Folders first, then files; alphabetical within each.
    entries.sort_by(|a, b| {
        let a_dir = a.kind == "tree";
        let b_dir = b.kind == "tree";
        b_dir.cmp(&a_dir).then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

pub fn show_blob(repo_path: &Path, oid: &str, file: &str) -> anyhow::Result<BlobView> {
    let repo = gix::open(repo_path)?;
    let blob_oid = gix::ObjectId::from_hex(oid.as_bytes())?;
    let obj = repo.find_object(blob_oid)?;
    let bytes = obj.data.clone();
    let size = bytes.len() as u64;
    let is_binary = is_binary(&bytes);

    let lines = if is_binary {
        Vec::new()
    } else {
        let lang = highlight::detect_language(file);
        let token_lines: Vec<Vec<Token>> = lang
            .and_then(|l| highlight::highlight_per_line(&bytes, l))
            .unwrap_or_default();

        let split: Vec<&[u8]> = bytes.split(|&b| b == b'\n').collect();
        let drop_last = bytes.ends_with(b"\n") && split.last().is_some_and(|l| l.is_empty());
        let take = if drop_last {
            split.len() - 1
        } else {
            split.len()
        };

        let mut out: Vec<BlobLine> = Vec::with_capacity(take);
        for (idx, line_bytes) in split.iter().enumerate().take(take) {
            let text = String::from_utf8_lossy(line_bytes).into_owned();
            let tokens = token_lines.get(idx).cloned();
            out.push(BlobLine {
                number: idx as u32 + 1,
                text,
                tokens,
            });
        }
        out
    };

    Ok(BlobView {
        path: file.to_string(),
        oid: blob_oid.to_string(),
        size,
        is_binary,
        lines,
    })
}

pub fn diff_commit(path: &Path, oid: &str) -> anyhow::Result<CommitDiff> {
    let repo = gix::open(path)?;
    let target_oid = gix::ObjectId::from_hex(oid.as_bytes())?;
    let commit_info = build_commit_info(&repo, target_oid)?;
    let commit = repo.find_object(target_oid)?.try_into_commit()?;
    let new_tree = commit.tree()?;
    let old_tree = match commit.parent_ids().next() {
        Some(parent_id) => repo.find_object(parent_id)?.try_into_commit()?.tree()?,
        None => repo.empty_tree(),
    };
    let files = diff_two_trees(&repo, &old_tree, &new_tree)?;
    Ok(CommitDiff {
        commit: commit_info,
        files,
    })
}

/// Diff between any two refs (branches, tags, oids, or anything `git`'s
/// rev-parse understands). Returns the per-file unified diff list in
/// the same shape `diff_commit` uses, so the UI can reuse the same
/// renderer. Rename/copy detection is on by default.
pub fn diff_refs(path: &Path, from: &str, to: &str) -> anyhow::Result<Vec<FileDiff>> {
    let from = from.trim();
    let to = to.trim();
    if from.is_empty() || to.is_empty() {
        anyhow::bail!("diff_refs needs non-empty `from` and `to` refs");
    }
    let repo = gix::open(path)?;
    let from_id = repo
        .rev_parse_single(from.as_bytes())
        .map_err(|e| anyhow::anyhow!("resolving `{from}`: {e}"))?
        .detach();
    let to_id = repo
        .rev_parse_single(to.as_bytes())
        .map_err(|e| anyhow::anyhow!("resolving `{to}`: {e}"))?
        .detach();
    let from_tree = repo.find_object(from_id)?.try_into_commit()?.tree()?;
    let to_tree = repo.find_object(to_id)?.try_into_commit()?.tree()?;
    diff_two_trees(&repo, &from_tree, &to_tree)
}

/// Per-file diff between two trees. Shared by `diff_commit` (parent-vs-
/// commit) and `diff_refs` (a-vs-b). Rename / copy detection on, tree-
/// only entries filtered out (we report file changes).
fn diff_two_trees(
    repo: &gix::Repository,
    old_tree: &gix::Tree<'_>,
    new_tree: &gix::Tree<'_>,
) -> anyhow::Result<Vec<FileDiff>> {
    use gix::object::tree::diff::{Action, Change};

    let mut files: Vec<FileDiff> = Vec::new();
    let mut platform = old_tree.changes()?;
    platform.options(|opts| {
        opts.track_rewrites(Some(gix::diff::Rewrites::default()));
    });
    platform.for_each_to_obtain_tree(new_tree, |change| -> Result<Action, anyhow::Error> {
        let file = match &change {
            Change::Addition { id, entry_mode, .. } if !entry_mode.is_tree() => {
                let obj = repo.find_object(id.detach())?;
                Some(make_file_diff(
                    change.location().to_str_lossy().into_owned(),
                    None,
                    "added",
                    &[],
                    &obj.data,
                ))
            }
            Change::Deletion { id, entry_mode, .. } if !entry_mode.is_tree() => {
                let obj = repo.find_object(id.detach())?;
                Some(make_file_diff(
                    change.location().to_str_lossy().into_owned(),
                    None,
                    "deleted",
                    &obj.data,
                    &[],
                ))
            }
            Change::Modification {
                id,
                previous_id,
                entry_mode,
                ..
            } if !entry_mode.is_tree() => {
                let new_obj = repo.find_object(id.detach())?;
                let old_obj = repo.find_object(previous_id.detach())?;
                Some(make_file_diff(
                    change.location().to_str_lossy().into_owned(),
                    None,
                    "modified",
                    &old_obj.data,
                    &new_obj.data,
                ))
            }
            Change::Rewrite {
                source_id,
                source_location,
                id,
                location,
                entry_mode,
                copy,
                ..
            } if !entry_mode.is_tree() => {
                let new_obj = repo.find_object(id.detach())?;
                let old_obj = repo.find_object(source_id.detach())?;
                let kind = if *copy { "copied" } else { "renamed" };
                Some(make_file_diff(
                    location.to_str_lossy().into_owned(),
                    Some(source_location.to_str_lossy().into_owned()),
                    kind,
                    &old_obj.data,
                    &new_obj.data,
                ))
            }
            _ => None,
        };
        if let Some(fd) = file {
            files.push(fd);
        }
        Ok(Action::Continue(()))
    })?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

fn make_file_diff(
    path: String,
    old_path: Option<String>,
    kind: &str,
    old_bytes: &[u8],
    new_bytes: &[u8],
) -> FileDiff {
    let is_binary = is_binary(old_bytes) || is_binary(new_bytes);
    let hunks = if is_binary {
        Vec::new()
    } else {
        let lang = highlight::detect_language(&path);
        let old_tokens = lang
            .and_then(|l| highlight::highlight_per_line(old_bytes, l))
            .unwrap_or_default();
        let new_tokens = lang
            .and_then(|l| highlight::highlight_per_line(new_bytes, l))
            .unwrap_or_default();
        compute_hunks(old_bytes, new_bytes, &old_tokens, &new_tokens)
    };
    FileDiff {
        path,
        old_path,
        kind: kind.into(),
        is_binary,
        hunks,
    }
}

pub fn diff_working(repo_path: &Path, file: &str) -> anyhow::Result<FileDiff> {
    let repo = gix::open(repo_path)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("bare repository has no working tree"))?;
    let abs_file = workdir.join(file);

    let worktree_bytes: Option<Vec<u8>> = match std::fs::read(&abs_file) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };

    let index = repo.index_or_empty()?;
    let path_bstr = gix::bstr::BStr::new(file.as_bytes());
    let index_bytes: Option<Vec<u8>> = if let Some(entry) = index.entry_by_path(path_bstr) {
        let obj = repo.find_object(entry.id)?;
        Some(obj.data.clone())
    } else {
        None
    };

    let (old, new, kind) = match (index_bytes, worktree_bytes) {
        (Some(idx), Some(wd)) => (idx, wd, "modified"),
        (None, Some(wd)) => (Vec::new(), wd, "untracked"),
        (Some(idx), None) => (idx, Vec::new(), "deleted"),
        (None, None) => {
            return Err(anyhow::anyhow!(
                "file not present in index or working tree: {file}"
            ));
        }
    };

    Ok(make_file_diff(file.to_string(), None, kind, &old, &new))
}

fn compute_hunks(
    old: &[u8],
    new: &[u8],
    old_tokens: &[Vec<Token>],
    new_tokens: &[Vec<Token>],
) -> Vec<DiffHunk> {
    let input: InternedInput<&[u8]> = InternedInput::new(old, new);
    let diff = gix::diff::blob::Diff::compute(Algorithm::Histogram, &input);
    let context_len: u32 = 3;
    let before_len = input.before.len() as u32;
    let after_len = input.after.len() as u32;

    let pick_old = |line_no: u32| -> Option<Vec<Token>> {
        old_tokens.get(line_no.checked_sub(1)? as usize).cloned()
    };
    let pick_new = |line_no: u32| -> Option<Vec<Token>> {
        new_tokens.get(line_no.checked_sub(1)? as usize).cloned()
    };

    // Group adjacent imara hunks whose context windows overlap into one
    // merged display hunk so we don't emit duplicate context lines.
    struct Group {
        old_start: u32,
        old_end: u32,
        new_start: u32,
        new_end: u32,
        changes: Vec<gix::diff::blob::Hunk>,
    }

    let mut groups: Vec<Group> = Vec::new();
    for h in diff.hunks() {
        let old_start = h.before.start.saturating_sub(context_len);
        let old_end = (h.before.end + context_len).min(before_len);
        let new_start = h.after.start.saturating_sub(context_len);
        let new_end = (h.after.end + context_len).min(after_len);

        if let Some(last) = groups.last_mut()
            && old_start <= last.old_end
        {
            last.old_end = last.old_end.max(old_end);
            last.new_end = last.new_end.max(new_end);
            last.changes.push(h);
            continue;
        }
        groups.push(Group {
            old_start,
            old_end,
            new_start,
            new_end,
            changes: vec![h],
        });
    }

    let mut hunks: Vec<DiffHunk> = Vec::new();
    for g in groups {
        let mut lines: Vec<DiffLine> = Vec::new();
        let mut old_no = g.old_start + 1;
        let mut new_no = g.new_start + 1;
        let mut cursor_old = g.old_start;

        for change in &g.changes {
            // Inter-change (or pre-first) context: lines unchanged in both
            // sides between previous cursor and the start of this change.
            for tok in &input.before[cursor_old as usize..change.before.start as usize] {
                lines.push(DiffLine {
                    kind: "ctx".into(),
                    old_line: Some(old_no),
                    new_line: Some(new_no),
                    text: token_text(&input, *tok),
                    tokens: pick_old(old_no),
                });
                old_no += 1;
                new_no += 1;
            }
            // Removed lines (this change's `before` range).
            for tok in &input.before[change.before.start as usize..change.before.end as usize] {
                lines.push(DiffLine {
                    kind: "del".into(),
                    old_line: Some(old_no),
                    new_line: None,
                    text: token_text(&input, *tok),
                    tokens: pick_old(old_no),
                });
                old_no += 1;
            }
            // Added lines (this change's `after` range).
            for tok in &input.after[change.after.start as usize..change.after.end as usize] {
                lines.push(DiffLine {
                    kind: "add".into(),
                    old_line: None,
                    new_line: Some(new_no),
                    text: token_text(&input, *tok),
                    tokens: pick_new(new_no),
                });
                new_no += 1;
            }
            cursor_old = change.before.end;
        }

        // Trailing context lines after the last change.
        for tok in &input.before[cursor_old as usize..g.old_end as usize] {
            lines.push(DiffLine {
                kind: "ctx".into(),
                old_line: Some(old_no),
                new_line: Some(new_no),
                text: token_text(&input, *tok),
                tokens: pick_old(old_no),
            });
            old_no += 1;
            new_no += 1;
        }

        let len_before = g.old_end - g.old_start;
        let len_after = g.new_end - g.new_start;

        hunks.push(DiffHunk {
            old_start: g.old_start + 1,
            old_count: len_before,
            new_start: g.new_start + 1,
            new_count: len_after,
            lines,
        });
    }

    hunks
}

fn token_text(input: &InternedInput<&[u8]>, token: IDToken) -> String {
    let bytes: &[u8] = input.interner[token];
    let trimmed = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    String::from_utf8_lossy(trimmed).into_owned()
}

// ─── Write operations ───────────────────────────────────────────────────
//
// These shell out to the system `git` CLI rather than driving gix's
// index API directly. Rationale: gix's write-side surface for index
// manipulation isn't fully stable yet, and staging UX has to feel
// indistinguishable from `git add` / `git reset` for users. Once gix
// settles its index/commit APIs, these become a drop-in replacement.

fn run_git(repo: &Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("spawning `git {}`: {e}", args.join(" ")))?;
    if !out.status.success() {
        // Some commands print failure detail on stdout instead of stderr
        // (notably `git merge` — its "CONFLICT (content): Merge conflict in …"
        // line goes to stdout). Fold both streams into the error so the
        // category-classifier downstream sees the real wording.
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let detail = match (stderr.trim(), stdout.trim()) {
            ("", "") => "(no output)".to_string(),
            ("", s) => s.to_string(),
            (e, "") => e.to_string(),
            (e, s) => format!("{e}\n{s}"),
        };
        anyhow::bail!(
            "git {} failed (status {}): {}",
            args.join(" "),
            out.status,
            detail
        );
    }
    Ok(out)
}

/// List entries in the index that differ from HEAD — what `git diff
/// --cached` reports. Complements `list_status`, which only covers the
/// worktree side, so the UI can show "staged" and "unstaged" entries
/// in separate sidebar blocks.
pub fn list_staged(path: &Path) -> anyhow::Result<Vec<StatusEntry>> {
    // Unborn HEAD: there's no committed tree to diff against, so every
    // index entry is logically "added".
    let head_present = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !head_present {
        let out = run_git(path, &["ls-files", "--cached"])?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut entries: Vec<StatusEntry> = stdout
            .lines()
            .map(|l| StatusEntry {
                path: l.to_string(),
                kind: "added".into(),
                old_path: None,
            })
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        return Ok(entries);
    }

    let out = run_git(path, &["diff", "--cached", "--name-status"])?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.splitn(3, '\t');
        let status = parts.next().unwrap_or("");
        // R<score> / C<score> are followed by old-path \t new-path; for
        // everything else there's just one path.
        let (path, old_path) = if status.starts_with('R') || status.starts_with('C') {
            let old = parts.next();
            let new = parts.next();
            match (old, new) {
                (Some(o), Some(n)) => (n.to_string(), Some(o.to_string())),
                _ => continue,
            }
        } else {
            match parts.next() {
                Some(p) => (p.to_string(), None),
                None => continue,
            }
        };
        let kind = match status.chars().next() {
            Some('A') => "added",
            Some('D') => "deleted",
            Some('R') => "renamed",
            Some('C') => "copied",
            _ => "modified",
        };
        entries.push(StatusEntry {
            path,
            kind: kind.into(),
            old_path,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// `git add -- <files>` — move worktree blobs into the index.
pub fn stage_files(repo: &Path, files: &[String]) -> anyhow::Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let mut args = vec!["add", "--"];
    for f in files {
        args.push(f.as_str());
    }
    run_git(repo, &args).map(|_| ())
}

/// `git restore -- <files>` — revert each path in the worktree to the
/// version currently in the index. No-op on clean files; errors on
/// paths that don't have an index entry (i.e. untracked files).
pub fn discard_files(repo: &Path, files: &[String]) -> anyhow::Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let mut args = vec!["restore", "--"];
    for f in files {
        args.push(f.as_str());
    }
    run_git(repo, &args).map(|_| ())
}

/// `git checkout <target>` — `target` can be a branch name, a tag, or
/// a commit oid. Refuses (via git) when the worktree would be
/// clobbered by the switch; the error bubbles up so the UI can
/// surface it.
pub fn checkout(repo: &Path, target: &str) -> anyhow::Result<()> {
    if target.trim().is_empty() {
        anyhow::bail!("checkout target must not be empty");
    }
    run_git(repo, &["checkout", target]).map(|_| ())
}

/// Create a new branch named `name`. When `from` is `Some(rev)` the
/// branch starts at that revision; otherwise from current HEAD. When
/// `switch` is true, checks out the new branch in the same step
/// (`git checkout -b`).
pub fn create_branch(
    repo: &Path,
    name: &str,
    from: Option<&str>,
    switch: bool,
) -> anyhow::Result<()> {
    if name.trim().is_empty() {
        anyhow::bail!("branch name must not be empty");
    }
    let mut args: Vec<&str> = if switch {
        vec!["checkout", "-b", name]
    } else {
        vec!["branch", name]
    };
    if let Some(f) = from.filter(|s| !s.trim().is_empty()) {
        args.push(f);
    }
    run_git(repo, &args).map(|_| ())
}

/// `git branch -d <name>` (safe) or `git branch -D <name>` (force).
/// Safe delete refuses to drop an unmerged branch — the UI handles
/// that error by offering a "force delete" confirm dialog that
/// re-issues with `force = true`.
pub fn delete_branch(repo: &Path, name: &str, force: bool) -> anyhow::Result<()> {
    if name.trim().is_empty() {
        anyhow::bail!("branch name must not be empty");
    }
    let flag = if force { "-D" } else { "-d" };
    run_git(repo, &["branch", flag, name]).map(|_| ())
}

/// `git branch -m <old> <new>`. Refuses (via git) when `new` already
/// exists.
pub fn rename_branch(repo: &Path, old: &str, new: &str) -> anyhow::Result<()> {
    if old.trim().is_empty() || new.trim().is_empty() {
        anyhow::bail!("branch names must not be empty");
    }
    if old == new {
        return Ok(());
    }
    run_git(repo, &["branch", "-m", old, new]).map(|_| ())
}

/// List entries in the stash. Newest first, matching `git stash list`'s
/// own ordering (`stash@{0}` is the most recent push).
pub fn stash_list(path: &Path) -> anyhow::Result<Vec<StashEntry>> {
    let out = run_git(path, &["stash", "list", "--format=%gd|%s|%ct"])?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.splitn(3, '|');
        let ref_name = parts.next().unwrap_or("").to_string();
        let message = parts.next().unwrap_or("").to_string();
        let time_unix: i64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        if ref_name.is_empty() {
            continue;
        }
        let index = parse_stash_index(&ref_name).unwrap_or(0);
        entries.push(StashEntry {
            index,
            ref_name,
            message,
            time_unix,
        });
    }
    Ok(entries)
}

fn parse_stash_index(ref_name: &str) -> Option<usize> {
    ref_name
        .strip_prefix("stash@{")
        .and_then(|s| s.strip_suffix('}'))
        .and_then(|s| s.parse().ok())
}

/// `git stash push -m <message>` (or just `git stash push` when
/// `message` is `None` / empty). Returns even when the worktree was
/// clean and nothing was stashed — the caller can re-list to confirm.
pub fn stash_save(repo: &Path, message: Option<&str>) -> anyhow::Result<()> {
    let mut args: Vec<&str> = vec!["stash", "push"];
    if let Some(m) = message.filter(|s| !s.trim().is_empty()) {
        args.push("-m");
        args.push(m);
    }
    run_git(repo, &args).map(|_| ())
}

/// `git stash pop stash@{index}` — apply and remove the stash. Fails
/// when the apply hits a conflict; the stash stays in the list and the
/// worktree is partially merged.
pub fn stash_pop(repo: &Path, index: usize) -> anyhow::Result<()> {
    let r = format!("stash@{{{index}}}");
    run_git(repo, &["stash", "pop", &r]).map(|_| ())
}

/// `git stash drop stash@{index}` — discard the stash without applying.
pub fn stash_drop(repo: &Path, index: usize) -> anyhow::Result<()> {
    let r = format!("stash@{{{index}}}");
    run_git(repo, &["stash", "drop", &r]).map(|_| ())
}

/// `git reset HEAD -- <files>` — drop the index entries back to whatever
/// HEAD has for these paths (or remove them entirely for new files).
pub fn unstage_files(repo: &Path, files: &[String]) -> anyhow::Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let mut args = vec!["reset", "-q", "HEAD", "--"];
    for f in files {
        args.push(f.as_str());
    }
    run_git(repo, &args).map(|_| ())
}

/// Line-by-line attribution of `file` at HEAD via `git blame
/// --porcelain`. Uncommitted lines (e.g. unstaged modifications) come
/// back with the all-zero oid that git uses as a sentinel; the UI
/// renders them with a "not committed yet" treatment.
pub fn blame_file(repo: &Path, file: &str) -> anyhow::Result<BlameView> {
    let out = run_git(repo, &["blame", "--porcelain", "--", file])?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    parse_blame_porcelain(&stdout, file.to_string())
}

fn parse_blame_porcelain(text: &str, path: String) -> anyhow::Result<BlameView> {
    use std::collections::HashMap;

    #[derive(Default, Clone)]
    struct CommitMeta {
        author: String,
        time: i64,
        summary: String,
    }

    let mut commits: HashMap<String, CommitMeta> = HashMap::new();
    let mut lines: Vec<BlameLine> = Vec::new();

    let mut iter = text.lines().peekable();
    while let Some(header) = iter.next() {
        // <oid> <orig-line> <final-line> [<num-in-group>]
        let mut parts = header.split_whitespace();
        let oid = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("blame: missing oid in header `{header}`"))?
            .to_string();
        let _orig = parts.next();
        let final_line: u32 = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("blame: missing final-line in header `{header}`"))?
            .parse()?;

        let mut meta = CommitMeta::default();
        let mut saw_author = false;
        let mut text_line = String::new();
        while let Some(next) = iter.peek() {
            if let Some(content) = next.strip_prefix('\t') {
                text_line = content.to_string();
                iter.next();
                break;
            }
            let line = iter.next().unwrap();
            if let Some(val) = line.strip_prefix("author ") {
                meta.author = val.to_string();
                saw_author = true;
            } else if let Some(val) = line.strip_prefix("author-time ") {
                meta.time = val.parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("summary ") {
                meta.summary = val.to_string();
            }
            // Drop the rest (author-mail, author-tz, committer-*, previous, filename, boundary).
        }

        // First mention of a commit carries all header fields; later mentions
        // only repeat the oid + tab-content line, so cache and look up.
        if saw_author {
            commits.insert(oid.clone(), meta.clone());
        } else if let Some(cached) = commits.get(&oid) {
            meta = cached.clone();
        }

        lines.push(BlameLine {
            line_number: final_line,
            text: text_line,
            short_oid: oid.chars().take(8).collect(),
            oid,
            author_name: meta.author,
            time_unix: meta.time,
            summary: meta.summary,
        });
    }

    Ok(BlameView { path, lines })
}

/// Look up a single commit by oid. Same `CommitInfo` shape that
/// `log_commits` produces, but resolved directly rather than walking
/// the ancestor graph — useful for permalinks like `?oid=abc123`.
pub fn commit_info(path: &Path, oid: &str) -> anyhow::Result<CommitInfo> {
    let repo = gix::open(path)?;
    let oid = gix::ObjectId::from_hex(oid.as_bytes())
        .map_err(|e| anyhow::anyhow!("invalid oid `{oid}`: {e}"))?;
    build_commit_info(&repo, oid)
}

/// Create a commit with the currently-staged index. Returns the new
/// HEAD oid. `author` is the optional `--author=<Name <email>>`
/// override; pass `None` to use the repo's gitconfig identity. The
/// commit body, if any, is just newlines inside `message` (git
/// treats the first line as the subject).
pub fn commit(repo: &Path, message: &str, author: Option<&str>) -> anyhow::Result<String> {
    if message.trim().is_empty() {
        anyhow::bail!("commit message must not be empty");
    }
    let mut args: Vec<String> = vec!["commit".into(), "-q".into(), "-m".into(), message.into()];
    if let Some(a) = author.filter(|s| !s.trim().is_empty()) {
        args.push(format!("--author={a}"));
    }
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git(repo, &args_ref)?;
    let out = run_git(repo, &["rev-parse", "HEAD"])?;
    let oid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(oid)
}

/// Snapshot of any in-progress merge / cherry-pick / rebase / revert.
/// `kind = "clean"` when nothing is mid-flight; otherwise the
/// conflicted paths and a human-readable subject are surfaced so the
/// UI can show a banner with one-click Abort / Continue / Skip.
pub fn repo_state(path: &Path) -> anyhow::Result<RepoState> {
    let repo = gix::open(path)?;
    let git_dir = repo.git_dir();
    let kind = if git_dir.join("rebase-merge").is_dir() || git_dir.join("rebase-apply").is_dir() {
        "rebasing"
    } else if git_dir.join("MERGE_HEAD").exists() {
        "merging"
    } else if git_dir.join("REVERT_HEAD").exists() {
        "reverting"
    } else if git_dir.join("CHERRY_PICK_HEAD").exists() {
        "cherry-picking"
    } else {
        "clean"
    };
    if kind == "clean" {
        return Ok(RepoState {
            kind: kind.into(),
            subject: None,
            conflicted: Vec::new(),
        });
    }
    let subject = match kind {
        "rebasing" => {
            // `rebase-merge` is the modern layout. `head-name` is the ref
            // being rebased (e.g. `refs/heads/feature`), `onto` is the
            // target oid. Either layout may also have a `message` file
            // for the commit currently being applied.
            let dir = if git_dir.join("rebase-merge").is_dir() {
                git_dir.join("rebase-merge")
            } else {
                git_dir.join("rebase-apply")
            };
            let head_name = std::fs::read_to_string(dir.join("head-name"))
                .ok()
                .map(|s| s.trim().trim_start_matches("refs/heads/").to_string());
            let onto = std::fs::read_to_string(dir.join("onto"))
                .ok()
                .map(|s| s.trim().chars().take(8).collect::<String>());
            match (head_name, onto) {
                (Some(h), Some(o)) => Some(format!("{h} onto {o}")),
                (Some(h), None) => Some(h),
                _ => None,
            }
        }
        _ => std::fs::read_to_string(git_dir.join("MERGE_MSG"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| l.trim().to_string())
            }),
    };
    let conflicted: Vec<String> = list_status(path)?
        .into_iter()
        .filter(|e| e.kind == "conflict")
        .map(|e| e.path)
        .collect();
    Ok(RepoState {
        kind: kind.into(),
        subject,
        conflicted,
    })
}

/// `git merge --abort` — drop the in-progress merge, restore the
/// worktree to the pre-merge commit and remove `MERGE_HEAD`.
pub fn merge_abort(repo: &Path) -> anyhow::Result<()> {
    run_git(repo, &["merge", "--abort"]).map(|_| ())
}

/// `git merge --continue` (equivalent to `git commit --no-edit` after
/// resolving all conflicts) — finalize the merge using the message git
/// stashed in `MERGE_MSG`.
pub fn merge_continue(repo: &Path) -> anyhow::Result<()> {
    run_git(repo, &["commit", "--no-edit", "-q"]).map(|_| ())
}

/// `git cherry-pick --abort`.
pub fn cherry_pick_abort(repo: &Path) -> anyhow::Result<()> {
    run_git(repo, &["cherry-pick", "--abort"]).map(|_| ())
}

/// `git cherry-pick --continue` after resolving all conflicts.
pub fn cherry_pick_continue(repo: &Path) -> anyhow::Result<()> {
    run_git(repo, &["cherry-pick", "--continue"]).map(|_| ())
}

/// `git rebase <upstream>` — replay the current branch's commits on
/// top of `upstream` (a branch name, tag, or commit oid). Conflicts
/// land in `.git/rebase-merge/` and `repo_state` reports
/// `kind = "rebasing"`; the same conflict banner handles abort /
/// continue / skip.
pub fn rebase(repo: &Path, upstream: &str) -> anyhow::Result<NetworkOpResult> {
    let upstream = upstream.trim();
    if upstream.is_empty() {
        anyhow::bail!("rebase target must not be empty");
    }
    let out = run_git(repo, &["rebase", upstream])?;
    Ok(NetworkOpResult {
        op: "rebase".into(),
        remote: upstream.into(),
        summary: format_network_output(&out, "rebase finished"),
    })
}

pub fn rebase_abort(repo: &Path) -> anyhow::Result<()> {
    run_git(repo, &["rebase", "--abort"]).map(|_| ())
}

pub fn rebase_continue(repo: &Path) -> anyhow::Result<()> {
    // `GIT_EDITOR=true` accepts whatever message git would have opened
    // an editor for (the commit being applied retains its original
    // message); without this `git rebase --continue` blocks on stdin.
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rebase", "--continue"])
        .env("GIT_EDITOR", "true")
        .output()
        .map_err(|e| anyhow::anyhow!("spawning `git rebase --continue`: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let detail = match (stderr.trim(), stdout.trim()) {
            ("", "") => "(no output)".to_string(),
            ("", s) => s.to_string(),
            (e, "") => e.to_string(),
            (e, s) => format!("{e}\n{s}"),
        };
        anyhow::bail!("git rebase --continue failed: {detail}");
    }
    Ok(())
}

pub fn rebase_skip(repo: &Path) -> anyhow::Result<()> {
    run_git(repo, &["rebase", "--skip"]).map(|_| ())
}

/// `git revert <oid>` — append a new commit on the current branch
/// that inverts `oid`'s changes. Conflicts land in `REVERT_HEAD` /
/// `MERGE_MSG`; `repo_state.kind` becomes `"reverting"`.
pub fn revert(repo: &Path, oid: &str) -> anyhow::Result<NetworkOpResult> {
    let oid = oid.trim();
    if oid.is_empty() {
        anyhow::bail!("revert oid must not be empty");
    }
    let out = run_git(repo, &["revert", "--no-edit", oid])?;
    Ok(NetworkOpResult {
        op: "revert".into(),
        remote: oid.into(),
        summary: format_network_output(&out, "revert applied"),
    })
}

pub fn revert_abort(repo: &Path) -> anyhow::Result<()> {
    run_git(repo, &["revert", "--abort"]).map(|_| ())
}

pub fn revert_continue(repo: &Path) -> anyhow::Result<()> {
    run_git(repo, &["revert", "--continue", "--no-edit"]).map(|_| ())
}

pub fn revert_skip(repo: &Path) -> anyhow::Result<()> {
    run_git(repo, &["revert", "--skip"]).map(|_| ())
}

/// Diff between `HEAD:<file>` and the index version of `file` — i.e.
/// "what's currently staged". Returns the same `FileDiff` shape as
/// `diff_working`, so the existing renderer can reuse it.
///
/// Implemented via shell-out for simplicity: `git diff --cached --
/// <file>` against the index is the canonical command, and we parse
/// its unified-diff output. The alternative gix-based approach would
/// require lifting more of the tree-diff plumbing out of
/// `diff_commit` — fine to defer until we need richer features.
pub fn diff_index(repo: &Path, file: &str) -> anyhow::Result<FileDiff> {
    // Resolve `HEAD:<file>` to a blob (might not exist if the file is
    // newly added). Then resolve the index entry. Use plain shell-out
    // to keep this simple — staged diffs are usually small.
    let head_bytes = head_blob_bytes(repo, file)?;
    let index_bytes = index_blob_bytes(repo, file)?;
    let (head_bytes, index_bytes) = match (head_bytes, index_bytes) {
        (Some(h), Some(i)) => (h, i),
        (None, Some(i)) => (Vec::new(), i),
        (Some(h), None) => (h, Vec::new()),
        (None, None) => anyhow::bail!("file `{file}` not present in HEAD or index"),
    };
    let kind = match (head_bytes.is_empty(), index_bytes.is_empty()) {
        (true, false) => "added",
        (false, true) => "deleted",
        _ => "modified",
    };
    let is_binary = is_binary(&head_bytes) || is_binary(&index_bytes);
    let hunks = if is_binary {
        Vec::new()
    } else {
        let lang = highlight::detect_language(file);
        let old_tokens = lang
            .and_then(|l| highlight::highlight_per_line(&head_bytes, l))
            .unwrap_or_default();
        let new_tokens = lang
            .and_then(|l| highlight::highlight_per_line(&index_bytes, l))
            .unwrap_or_default();
        compute_hunks(&head_bytes, &index_bytes, &old_tokens, &new_tokens)
    };
    Ok(FileDiff {
        path: file.to_string(),
        old_path: None,
        kind: kind.into(),
        is_binary,
        hunks,
    })
}

fn head_blob_bytes(repo: &Path, file: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let spec = format!("HEAD:{file}");
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["show", &spec])
        .output()
        .map_err(|e| anyhow::anyhow!("spawning `git show HEAD:{file}`: {e}"))?;
    if !out.status.success() {
        // "exists in HEAD" failures: file added, unborn HEAD, etc. Treat
        // as absent rather than erroring.
        return Ok(None);
    }
    Ok(Some(out.stdout))
}

fn index_blob_bytes(repo: &Path, file: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let spec = format!(":{file}");
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["show", &spec])
        .output()
        .map_err(|e| anyhow::anyhow!("spawning `git show :{file}`: {e}"))?;
    if !out.status.success() {
        return Ok(None);
    }
    Ok(Some(out.stdout))
}

/// Symmetric to `stage_hunks`: pick a subset of staged hunks and revert
/// just those from the index, leaving the rest staged. We rebuild the
/// diff between HEAD and the index, filter to the selected indices,
/// serialize, and pipe through `git apply --cached --reverse --recount`.
pub fn unstage_hunks(repo: &Path, file: &str, indices: &[usize]) -> anyhow::Result<()> {
    if indices.is_empty() {
        anyhow::bail!("no hunks selected");
    }
    let diff = diff_index(repo, file)?;
    if diff.is_binary {
        anyhow::bail!("cannot unstage hunks of a binary file");
    }
    if diff.kind != "modified" {
        anyhow::bail!(
            "hunk-level unstaging only supports `modified` files; got `{}`",
            diff.kind
        );
    }
    let mut selected: Vec<&DiffHunk> = Vec::with_capacity(indices.len());
    for &i in indices {
        let h = diff.hunks.get(i).ok_or_else(|| {
            anyhow::anyhow!(
                "hunk index {i} out of range (file has {} hunks)",
                diff.hunks.len()
            )
        })?;
        selected.push(h);
    }
    let patch = serialize_hunks_to_patch(file, &selected);
    apply_reverse_to_index(repo, &patch)
}

fn apply_reverse_to_index(repo: &Path, patch: &str) -> anyhow::Result<()> {
    use std::io::Write;
    let mut child = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["apply", "--cached", "--reverse", "--recount", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("spawning `git apply --cached --reverse`: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(patch.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let detail = match (stderr.trim(), stdout.trim()) {
            ("", "") => "(no output)".to_string(),
            ("", s) => s.to_string(),
            (e, "") => e.to_string(),
            (e, s) => format!("{e}\n{s}"),
        };
        anyhow::bail!("git apply --cached --reverse failed: {detail}");
    }
    Ok(())
}

/// Stage a subset of a modified file's hunks. `indices` references the
/// hunks as they appear in `diff_working(file)`. We re-fetch that diff,
/// filter it down to the selected hunks, serialize the result back to a
/// unified-diff text, and pipe it through `git apply --cached
/// --recount` so only those hunks land in the index — the equivalent of
/// hitting `y` for some hunks in `git add -p`.
///
/// Fails on binary files (no textual diff), on untracked / deleted
/// files (no `modified` shape to subset), and on out-of-range indices.
/// Empty `indices` is also an error so the caller has to be explicit
/// rather than accidentally no-op'ing.
pub fn stage_hunks(repo: &Path, file: &str, indices: &[usize]) -> anyhow::Result<()> {
    if indices.is_empty() {
        anyhow::bail!("no hunks selected");
    }
    let diff = diff_working(repo, file)?;
    if diff.is_binary {
        anyhow::bail!("cannot stage hunks of a binary file");
    }
    if diff.kind != "modified" {
        anyhow::bail!(
            "hunk-level staging only supports `modified` files; got `{}`",
            diff.kind
        );
    }
    let mut selected: Vec<&DiffHunk> = Vec::with_capacity(indices.len());
    for &i in indices {
        let h = diff.hunks.get(i).ok_or_else(|| {
            anyhow::anyhow!(
                "hunk index {i} out of range (file has {} hunks)",
                diff.hunks.len()
            )
        })?;
        selected.push(h);
    }
    let patch = serialize_hunks_to_patch(file, &selected);
    apply_patch_to_index(repo, &patch)
}

/// Build a minimal `git apply`-compatible unified-diff text covering
/// just `hunks` of `file`. We rely on `--recount` downstream so any
/// inter-hunk line drift produced by selecting a sparse subset of
/// hunks gets fixed up by git rather than us re-numbering by hand.
fn serialize_hunks_to_patch(file: &str, hunks: &[&DiffHunk]) -> String {
    let mut s = String::new();
    s.push_str(&format!("diff --git a/{file} b/{file}\n"));
    s.push_str(&format!("--- a/{file}\n"));
    s.push_str(&format!("+++ b/{file}\n"));
    for h in hunks {
        s.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            h.old_start, h.old_count, h.new_start, h.new_count
        ));
        for line in &h.lines {
            let prefix = match line.kind.as_str() {
                "add" => '+',
                "del" => '-',
                _ => ' ',
            };
            s.push(prefix);
            s.push_str(&line.text);
            s.push('\n');
        }
    }
    s
}

/// Pipe `patch` into `git apply --cached --recount -` and surface git's
/// own error wording on failure. Stdin is buffered, not memory-mapped,
/// so large patches are fine.
fn apply_patch_to_index(repo: &Path, patch: &str) -> anyhow::Result<()> {
    use std::io::Write;
    let mut child = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["apply", "--cached", "--recount", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("spawning `git apply --cached`: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(patch.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let detail = match (stderr.trim(), stdout.trim()) {
            ("", "") => "(no output)".to_string(),
            ("", s) => s.to_string(),
            (e, "") => e.to_string(),
            (e, s) => format!("{e}\n{s}"),
        };
        anyhow::bail!("git apply --cached failed: {detail}");
    }
    Ok(())
}

/// `git reset --<mode> <target>` — move HEAD to `target`. `mode` is
/// one of `"soft"` (keep index + worktree), `"mixed"` (reset index,
/// keep worktree — git's default), or `"hard"` (reset everything,
/// destructive). `target` accepts anything `git reset` accepts:
/// branch name, tag, commit oid, `HEAD~N`, etc.
pub fn reset(repo: &Path, target: &str, mode: &str) -> anyhow::Result<()> {
    let target = target.trim();
    if target.is_empty() {
        anyhow::bail!("reset target must not be empty");
    }
    let flag = match mode.trim() {
        "soft" => "--soft",
        "mixed" | "" => "--mixed",
        "hard" => "--hard",
        other => anyhow::bail!("reset mode must be `soft`, `mixed`, or `hard`, got `{other}`"),
    };
    run_git(repo, &["reset", "-q", flag, target]).map(|_| ())
}

/// Resolve a single conflicted file to one side of the merge:
/// `side = "ours"` keeps the current branch's version, `"theirs"`
/// takes the incoming side. After replacing the worktree copy, the
/// file is staged (`git add`) so a subsequent `merge --continue` or
/// `cherry-pick --continue` sees it resolved.
pub fn resolve_file(repo: &Path, file: &str, side: &str) -> anyhow::Result<()> {
    let flag = match side.trim() {
        "ours" => "--ours",
        "theirs" => "--theirs",
        other => anyhow::bail!("resolve side must be `ours` or `theirs`, got `{other}`"),
    };
    run_git(repo, &["checkout", flag, "--", file])?;
    run_git(repo, &["add", "--", file])?;
    Ok(())
}

/// Parse a conflicted file from the worktree into a `ConflictView`.
/// Recognises both 2-way (`<<<<<<< / ======= / >>>>>>>`) and 3-way
/// (`<<<<<<< / ||||||| / ======= / >>>>>>>`, set by `merge.conflictStyle
/// = diff3`) marker layouts. Returns an empty `blocks` list when the
/// file has no markers — useful for the UI to detect "all hunks
/// resolved" without a separate endpoint.
pub fn parse_conflicts(repo: &Path, file: &str) -> anyhow::Result<ConflictView> {
    let full = repo.join(file);
    let content = std::fs::read_to_string(&full)
        .map_err(|e| anyhow::anyhow!("reading `{}`: {e}", full.display()))?;
    let mut blocks: Vec<ConflictBlock> = Vec::new();
    let mut section = Section::None;
    let mut current: Option<ConflictBlock> = None;
    for (i, line) in content.lines().enumerate() {
        let lineno = (i + 1) as u32;
        if let Some(rest) = line.strip_prefix("<<<<<<<") {
            current = Some(ConflictBlock {
                index: blocks.len(),
                start_line: lineno,
                end_line: lineno,
                ours: Vec::new(),
                base: None,
                theirs: Vec::new(),
                ours_label: rest.trim().to_string(),
                theirs_label: String::new(),
            });
            section = Section::Ours;
        } else if line.starts_with("|||||||") && current.is_some() {
            if let Some(b) = current.as_mut() {
                b.base = Some(Vec::new());
            }
            section = Section::Base;
        } else if line == "=======" && current.is_some() {
            section = Section::Theirs;
        } else if let Some(rest) = line.strip_prefix(">>>>>>>") {
            if let Some(mut b) = current.take() {
                b.end_line = lineno;
                b.theirs_label = rest.trim().to_string();
                blocks.push(b);
            }
            section = Section::None;
        } else if let Some(b) = current.as_mut() {
            match section {
                Section::Ours => b.ours.push(line.to_string()),
                Section::Base => {
                    if let Some(base) = b.base.as_mut() {
                        base.push(line.to_string());
                    }
                }
                Section::Theirs => b.theirs.push(line.to_string()),
                Section::None => {}
            }
        }
    }
    Ok(ConflictView {
        path: file.to_string(),
        blocks,
    })
}

#[derive(Copy, Clone)]
enum Section {
    None,
    Ours,
    Base,
    Theirs,
}

/// Resolve a single conflict block by replacing its `<<<<<<< / =======
/// / >>>>>>>` span with the chosen side. `side` accepts `"ours"`,
/// `"theirs"`, `"both-ours-first"`, or `"both-theirs-first"`. When the
/// resolution leaves the file with no remaining conflict markers, we
/// `git add` it so a subsequent `merge --continue` / `cherry-pick
/// --continue` sees a clean staged copy; otherwise the file stays
/// unstaged for the UI to walk through the next block.
pub fn resolve_conflict_hunk(
    repo: &Path,
    file: &str,
    index: usize,
    side: &str,
) -> anyhow::Result<()> {
    let view = parse_conflicts(repo, file)?;
    let block = view
        .blocks
        .get(index)
        .ok_or_else(|| anyhow::anyhow!("hunk index {index} out of range"))?;
    let replacement: Vec<String> = match side.trim() {
        "ours" => block.ours.clone(),
        "theirs" => block.theirs.clone(),
        "both-ours-first" => {
            let mut v = block.ours.clone();
            v.extend(block.theirs.clone());
            v
        }
        "both-theirs-first" => {
            let mut v = block.theirs.clone();
            v.extend(block.ours.clone());
            v
        }
        other => anyhow::bail!(
            "resolve side must be `ours`, `theirs`, `both-ours-first`, or `both-theirs-first`, got `{other}`"
        ),
    };

    let full = repo.join(file);
    let content = std::fs::read_to_string(&full)
        .map_err(|e| anyhow::anyhow!("reading `{}`: {e}", full.display()))?;
    // The line iterator drops the trailing newline; rebuild line-by-line
    // and re-add the original terminator if there was one.
    let mut out: Vec<String> = Vec::new();
    let start = block.start_line as usize;
    let end = block.end_line as usize;
    for (i, line) in content.lines().enumerate() {
        let lineno = i + 1;
        if lineno < start || lineno > end {
            out.push(line.to_string());
        } else if lineno == start {
            for r in &replacement {
                out.push(r.clone());
            }
        }
    }
    let trailing_newline = content.ends_with('\n');
    let mut joined = out.join("\n");
    if trailing_newline {
        joined.push('\n');
    }
    std::fs::write(&full, joined.as_bytes())?;

    // Re-parse to see if anything is left. Stage only when clean —
    // git refuses `merge --continue` while any conflict remains, but
    // we don't want to stage a still-conflicting file either.
    let after = parse_conflicts(repo, file)?;
    if after.blocks.is_empty() {
        run_git(repo, &["add", "--", file])?;
    }
    Ok(())
}

/// `git tag [-a -m <message>] <name> [<target>]` — create a lightweight
/// or annotated tag. `target` defaults to HEAD when None/empty.
/// `message: Some(_)` makes it annotated (`-a -m`); None makes it
/// lightweight. Fails with the standard "already exists" wording when
/// `name` is taken.
pub fn create_tag(
    repo: &Path,
    name: &str,
    target: Option<&str>,
    message: Option<&str>,
) -> anyhow::Result<()> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("tag name must not be empty");
    }
    let mut args: Vec<String> = vec!["tag".into()];
    if let Some(m) = message.filter(|s| !s.trim().is_empty()) {
        args.push("-a".into());
        args.push("-m".into());
        args.push(m.into());
    }
    args.push(name.into());
    if let Some(t) = target.filter(|s| !s.trim().is_empty()) {
        args.push(t.into());
    }
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git(repo, &args_ref).map(|_| ())
}

/// `git tag -d <name>` — drop a tag. Fails when the tag doesn't exist.
pub fn delete_tag(repo: &Path, name: &str) -> anyhow::Result<()> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("tag name must not be empty");
    }
    run_git(repo, &["tag", "-d", name]).map(|_| ())
}

/// `git log --follow -n<limit> -- <file>` — history of one file,
/// following renames. Same wire shape as `log_commits` so the UI can
/// reuse the log-row renderer. Implemented via shell-out: gix's
/// rev-walk doesn't expose `--follow` rename-tracking out of the box,
/// and parsing the CLI's record format is straightforward.
///
/// Record format: `%H` (full oid), `%h` (short), `%P` (parents,
/// space-separated), `%an`/`%ae` (author), `%at` (committer time),
/// `%s` (subject), `%b` (body). Fields separated by `\x1f`, records
/// by `\x1e` — both control bytes that can't appear in commit
/// metadata, so no quoting is needed.
pub fn log_file(repo: &Path, file: &str, limit: usize) -> anyhow::Result<Vec<CommitInfo>> {
    let limit_arg = format!("-n{}", limit.min(500));
    let format_arg = "--format=tformat:%H%x1f%h%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s%x1f%b%x1e";
    let out = run_git(
        repo,
        &["log", "--follow", &limit_arg, format_arg, "--", file],
    )?;
    let raw = String::from_utf8_lossy(&out.stdout);
    let mut commits = Vec::new();
    for record in raw.split('\x1e') {
        let record = record.trim_start_matches('\n');
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.split('\x1f').collect();
        if fields.len() < 7 {
            continue;
        }
        let oid = fields[0].to_string();
        let short_oid = fields[1].to_string();
        let parents: Vec<String> = fields[2].split_whitespace().map(String::from).collect();
        let author_name = fields[3].to_string();
        let author_email = fields[4].to_string();
        let time_unix: i64 = fields[5].parse().unwrap_or(0);
        let summary = fields[6].to_string();
        let body = fields.get(7).copied().unwrap_or("").trim_end().to_string();
        commits.push(CommitInfo {
            oid,
            short_oid,
            summary,
            body,
            parents,
            author_name,
            author_email,
            time_unix,
        });
    }
    Ok(commits)
}

/// `git fetch [remote]` — sync remote-tracking refs without touching
/// HEAD. `remote` empty/None lets git pick the current branch's
/// upstream (or `origin` if it's not configured). Auth (SSH keys,
/// HTTPS credential helpers) comes from the user's existing git
/// config — we run the same `git` binary they would on the CLI.
pub fn fetch(repo: &Path, remote: Option<&str>) -> anyhow::Result<NetworkOpResult> {
    let trimmed = remote.map(str::trim).filter(|s| !s.is_empty());
    let mut args: Vec<&str> = vec!["fetch", "--no-progress"];
    if let Some(r) = trimmed {
        args.push(r);
    }
    let out = run_git(repo, &args)?;
    Ok(NetworkOpResult {
        op: "fetch".into(),
        remote: trimmed.unwrap_or("").into(),
        summary: format_network_output(&out, "fetched (already up to date)"),
    })
}

/// `git pull [--ff-only|--no-rebase] [remote]` — fetch + integrate.
/// `ff_only: true` is the safe default — refuses anything that
/// isn't a clean fast-forward (no merge commit, no rebase, leaves
/// the worktree alone on failure). `ff_only: false` lets git's
/// configured `pull.rebase` setting decide; on conflict the user
/// has to resolve via the CLI.
pub fn pull(repo: &Path, remote: Option<&str>, ff_only: bool) -> anyhow::Result<NetworkOpResult> {
    let trimmed = remote.map(str::trim).filter(|s| !s.is_empty());
    let mut args: Vec<&str> = vec!["pull", "--no-progress"];
    if ff_only {
        args.push("--ff-only");
    }
    if let Some(r) = trimmed {
        args.push(r);
    }
    let out = run_git(repo, &args)?;
    Ok(NetworkOpResult {
        op: "pull".into(),
        remote: trimmed.unwrap_or("").into(),
        summary: format_network_output(&out, "already up to date"),
    })
}

/// `git push [-u] [--force-with-lease] [remote [refspec]]` — upload
/// objects and update remote refs. `force_with_lease` is the safe
/// flavour of `--force` that refuses to overwrite refs the local
/// side hasn't seen yet. `set_upstream` is `-u`: also write the
/// remote-tracking ref so future `git push` / `git pull` without
/// arguments target this remote+branch.
pub fn push(
    repo: &Path,
    remote: Option<&str>,
    refspec: Option<&str>,
    force_with_lease: bool,
    set_upstream: bool,
) -> anyhow::Result<NetworkOpResult> {
    let trimmed_remote = remote.map(str::trim).filter(|s| !s.is_empty());
    let trimmed_refspec = refspec.map(str::trim).filter(|s| !s.is_empty());
    let mut args: Vec<&str> = vec!["push", "--no-progress"];
    if set_upstream {
        args.push("-u");
    }
    if force_with_lease {
        args.push("--force-with-lease");
    }
    if let Some(r) = trimmed_remote {
        args.push(r);
        if let Some(rs) = trimmed_refspec {
            args.push(rs);
        }
    }
    let out = run_git(repo, &args)?;
    Ok(NetworkOpResult {
        op: "push".into(),
        remote: trimmed_remote.unwrap_or("").into(),
        summary: format_network_output(&out, "everything up to date"),
    })
}

/// `git merge [--no-ff] <target>` — integrate `target` (a branch name,
/// tag, or commit oid) into the current branch. The default produces
/// a fast-forward when possible and a merge commit otherwise. Pass
/// `no_ff: true` to force a merge commit even on fast-forward cases.
/// On conflict the index and worktree are left in the partially-merged
/// state and the error envelope carries git's own conflict message —
/// the user resolves and finalises via the CLI for now (conflict UI is
/// a separate milestone).
pub fn merge(repo: &Path, target: &str, no_ff: bool) -> anyhow::Result<NetworkOpResult> {
    let target = target.trim();
    if target.is_empty() {
        anyhow::bail!("merge target must not be empty");
    }
    let mut args: Vec<&str> = vec!["merge", "--no-progress"];
    if no_ff {
        args.push("--no-ff");
    }
    args.push(target);
    let out = run_git(repo, &args)?;
    Ok(NetworkOpResult {
        op: "merge".into(),
        remote: target.into(),
        summary: format_network_output(&out, "already up to date"),
    })
}

/// `git cherry-pick <oid>` — apply the changes from a single commit on
/// top of the current branch as a new commit. Conflict behaviour is
/// the same as `merge`: index and worktree are left mid-pick, and the
/// caller surfaces the error to the user.
pub fn cherry_pick(repo: &Path, oid: &str) -> anyhow::Result<NetworkOpResult> {
    let oid = oid.trim();
    if oid.is_empty() {
        anyhow::bail!("cherry-pick oid must not be empty");
    }
    let out = run_git(repo, &["cherry-pick", oid])?;
    Ok(NetworkOpResult {
        op: "cherry-pick".into(),
        remote: oid.into(),
        summary: format_network_output(&out, "cherry-pick applied"),
    })
}

/// git emits its progress / informational messages on stderr (success
/// AND failure), so we surface stderr first and fold in stdout when
/// non-empty. `fallback` is used when both streams come back blank —
/// happens on "already up to date" fetches.
fn format_network_output(out: &std::process::Output, fallback: &str) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match (stderr.is_empty(), stdout.is_empty()) {
        (true, true) => fallback.to_string(),
        (true, false) => stdout,
        (false, true) => stderr,
        (false, false) => format!("{stderr}\n{stdout}"),
    }
}
