use std::convert::Infallible;
use std::path::Path;

use gix::bstr::ByteSlice;
use serde::{Deserialize, Serialize};

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
    pub added: bool,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub kind: String, // "added" | "deleted" | "modified" | "renamed"
    pub lines: Vec<DiffLine>,
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
    use gix::object::blob::diff::lines::Change as LineChange;
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

    let mut cache = repo.diff_resource_cache_for_tree_diff()?;
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

            let mut lines: Vec<DiffLine> = Vec::new();
            if !matches!(change, Change::Rewrite { .. }) {
                if let Ok(mut dp) = change.diff(&mut cache) {
                    let _ = dp.lines(|lc| -> Result<(), Infallible> {
                        match lc {
                            LineChange::Addition { lines: ls } => {
                                for l in ls {
                                    lines.push(DiffLine {
                                        added: true,
                                        text: l.to_string(),
                                    });
                                }
                            }
                            LineChange::Deletion { lines: ls } => {
                                for l in ls {
                                    lines.push(DiffLine {
                                        added: false,
                                        text: l.to_string(),
                                    });
                                }
                            }
                            LineChange::Modification {
                                lines_before,
                                lines_after,
                            } => {
                                for l in lines_before {
                                    lines.push(DiffLine {
                                        added: false,
                                        text: l.to_string(),
                                    });
                                }
                                for l in lines_after {
                                    lines.push(DiffLine {
                                        added: true,
                                        text: l.to_string(),
                                    });
                                }
                            }
                        }
                        Ok(())
                    });
                }
            }

            files.push(FileDiff {
                path: location,
                kind: kind.into(),
                lines,
            });
            Ok(Action::Continue(()))
        })?;

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(CommitDiff {
        commit: commit_info,
        files,
    })
}
