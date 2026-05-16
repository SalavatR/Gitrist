pub mod highlight;

use std::path::Path;

use gix::bstr::ByteSlice;
use gix::diff::blob::{Algorithm, InternedInput, Token as IDToken};

pub use gitrust_types::{
    BlameLine, BlameView, BlobLine, BlobView, BranchInfo, CommitDiff, CommitInfo, DiffHunk,
    DiffLine, FileDiff, RemoteBranchInfo, RepoSummary, StatusEntry, TagInfo, Token, TreeEntry,
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

pub fn log_commits(path: &Path, limit: usize) -> anyhow::Result<Vec<CommitInfo>> {
    let repo = gix::open(path)?;
    let head_id = repo.head_id()?;
    let walk = head_id.ancestors().all()?;
    let mut commits = Vec::with_capacity(limit.min(64));
    for item in walk.take(limit) {
        let info = item?;
        commits.push(build_commit_info(&repo, info.id)?);
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
    use gix::object::tree::diff::{Action, Change};

    let repo = gix::open(path)?;
    let target_oid = gix::ObjectId::from_hex(oid.as_bytes())?;
    let commit_info = build_commit_info(&repo, target_oid)?;
    let commit = repo.find_object(target_oid)?.try_into_commit()?;
    let new_tree = commit.tree()?;
    let old_tree = match commit.parent_ids().next() {
        Some(parent_id) => repo.find_object(parent_id)?.try_into_commit()?.tree()?,
        None => repo.empty_tree(),
    };

    let mut files: Vec<FileDiff> = Vec::new();

    let mut platform = old_tree.changes()?;
    platform.options(|opts| {
        opts.track_rewrites(Some(gix::diff::Rewrites::default()));
    });
    platform.for_each_to_obtain_tree(&new_tree, |change| -> Result<Action, anyhow::Error> {
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
    Ok(CommitDiff {
        commit: commit_info,
        files,
    })
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
        let stderr = String::from_utf8_lossy(&out.stderr);
        let trimmed = stderr.trim();
        anyhow::bail!(
            "git {} failed (status {}): {}",
            args.join(" "),
            out.status,
            if trimmed.is_empty() {
                "(no stderr)"
            } else {
                trimmed
            }
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
