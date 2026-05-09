pub mod highlight;

use std::path::Path;

use gix::bstr::ByteSlice;
use gix::diff::blob::{Algorithm, InternedInput, Token as IDToken};
use serde::{Deserialize, Serialize};

pub use highlight::Token as HighlightToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSummary {
    pub path: String,
    pub git_dir: String,
    pub head_ref: Option<String>,
    pub head_oid: Option<String>,
    pub is_detached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub oid: String,
    pub short_oid: String,
    pub summary: String,
    pub body: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub time_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEntry {
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub oid: Option<String>,
    pub is_head: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: String, // "ctx" | "add" | "del"
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text: String,
    /// Per-token syntax highlighting for `text`. `None` when the language
    /// isn't recognised (or the file is binary). When present, the token
    /// texts concatenate to `text`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tokens: Option<Vec<HighlightToken>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub kind: String, // "added" | "deleted" | "modified" | "renamed"
    pub is_binary: bool,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDiff {
    pub commit: CommitInfo,
    pub files: Vec<FileDiff>,
}

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
                });
            }
            Item::DirectoryContents { entry, .. } => {
                out.push(StatusEntry {
                    path: entry.rela_path.to_str_lossy().into_owned(),
                    kind: "untracked".into(),
                });
            }
            Item::Rewrite { .. } => {}
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
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

pub fn diff_commit(path: &Path, oid: &str) -> anyhow::Result<CommitDiff> {
    use gix::object::tree::diff::{Action, Change};

    let repo = gix::open(path)?;
    let target_oid = gix::ObjectId::from_hex(oid.as_bytes())?;
    let commit_info = build_commit_info(&repo, target_oid)?;
    let commit = repo.find_object(target_oid)?.try_into_commit()?;
    let new_tree = commit.tree()?;
    let old_tree = match commit.parent_ids().next() {
        Some(parent_id) => repo
            .find_object(parent_id)?
            .try_into_commit()?
            .tree()?,
        None => repo.empty_tree(),
    };

    let mut files: Vec<FileDiff> = Vec::new();

    old_tree
        .changes()?
        .for_each_to_obtain_tree(&new_tree, |change| -> Result<Action, anyhow::Error> {
            let entry_mode = match &change {
                Change::Addition { entry_mode, .. }
                | Change::Deletion { entry_mode, .. }
                | Change::Modification { entry_mode, .. } => Some(*entry_mode),
                Change::Rewrite { .. } => None,
            };
            if entry_mode.is_some_and(|m| m.is_tree()) {
                return Ok(Action::Continue(()));
            }

            let location = change.location().to_str_lossy().into_owned();
            let kind: &str = match &change {
                Change::Addition { .. } => "added",
                Change::Deletion { .. } => "deleted",
                Change::Modification { .. } => "modified",
                Change::Rewrite { .. } => "renamed",
            };

            let (old_bytes, new_bytes): (Vec<u8>, Vec<u8>) = match &change {
                Change::Addition { id, .. } => {
                    let obj = repo.find_object(id.detach())?;
                    (Vec::new(), obj.data.clone())
                }
                Change::Deletion { id, .. } => {
                    let obj = repo.find_object(id.detach())?;
                    (obj.data.clone(), Vec::new())
                }
                Change::Modification {
                    id, previous_id, ..
                } => {
                    let new_obj = repo.find_object(id.detach())?;
                    let old_obj = repo.find_object(previous_id.detach())?;
                    (old_obj.data.clone(), new_obj.data.clone())
                }
                Change::Rewrite { .. } => (Vec::new(), Vec::new()),
            };

            let is_binary = is_binary(&old_bytes) || is_binary(&new_bytes);
            let hunks = if is_binary {
                Vec::new()
            } else {
                let lang = highlight::detect_language(&location);
                let old_tokens = lang
                    .and_then(|l| highlight::highlight_per_line(&old_bytes, l))
                    .unwrap_or_default();
                let new_tokens = lang
                    .and_then(|l| highlight::highlight_per_line(&new_bytes, l))
                    .unwrap_or_default();
                compute_hunks(&old_bytes, &new_bytes, &old_tokens, &new_tokens)
            };

            files.push(FileDiff {
                path: location,
                kind: kind.into(),
                is_binary,
                hunks,
            });
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

    let is_binary = is_binary(&old) || is_binary(&new);
    let hunks = if is_binary {
        Vec::new()
    } else {
        let lang = highlight::detect_language(file);
        let old_tokens = lang
            .and_then(|l| highlight::highlight_per_line(&old, l))
            .unwrap_or_default();
        let new_tokens = lang
            .and_then(|l| highlight::highlight_per_line(&new, l))
            .unwrap_or_default();
        compute_hunks(&old, &new, &old_tokens, &new_tokens)
    };

    Ok(FileDiff {
        path: file.to_string(),
        kind: kind.into(),
        is_binary,
        hunks,
    })
}

fn compute_hunks(
    old: &[u8],
    new: &[u8],
    old_tokens: &[Vec<HighlightToken>],
    new_tokens: &[Vec<HighlightToken>],
) -> Vec<DiffHunk> {
    let input: InternedInput<&[u8]> = InternedInput::new(old, new);
    let diff = gix::diff::blob::Diff::compute(Algorithm::Histogram, &input);
    let context_len: u32 = 3;
    let before_len = input.before.len() as u32;
    let after_len = input.after.len() as u32;

    let pick_old = |line_no: u32| -> Option<Vec<HighlightToken>> {
        old_tokens.get(line_no.checked_sub(1)? as usize).cloned()
    };
    let pick_new = |line_no: u32| -> Option<Vec<HighlightToken>> {
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

        if let Some(last) = groups.last_mut() {
            if old_start <= last.old_end {
                last.old_end = last.old_end.max(old_end);
                last.new_end = last.new_end.max(new_end);
                last.changes.push(h);
                continue;
            }
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
    let trimmed = bytes
        .strip_suffix(b"\n")
        .unwrap_or(bytes);
    String::from_utf8_lossy(trimmed).into_owned()
}
