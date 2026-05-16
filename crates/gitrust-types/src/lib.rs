//! Wire types shared between the server (gitrust-core / gitrust-server) and
//! the WASM client (gitrust-ui / gitrust-web). Pure data, no logic — kept
//! target-independent so it compiles for both `aarch64-unknown-linux-gnu`
//! and `wasm32-unknown-unknown`.

use serde::{Deserialize, Serialize};

/// One token inside a syntax-highlighted line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub text: String,
    pub class: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoSummary {
    pub path: String,
    pub git_dir: String,
    pub head_ref: Option<String>,
    pub head_oid: Option<String>,
    pub is_detached: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusEntry {
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub oid: Option<String>,
    pub is_head: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagInfo {
    pub name: String,
    pub oid: Option<String>,
    pub annotated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteBranchInfo {
    pub name: String,
    pub remote: String,
    pub oid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffLine {
    /// "ctx" | "add" | "del"
    pub kind: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tokens: Option<Vec<Token>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    /// Previous path for `renamed` / `copied` files; `None` otherwise.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub old_path: Option<String>,
    /// "added" | "deleted" | "modified" | "renamed" | "copied"
    pub kind: String,
    pub is_binary: bool,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitDiff {
    pub commit: CommitInfo,
    pub files: Vec<FileDiff>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    /// "tree" | "blob" | "symlink" | "submodule"
    pub kind: String,
    pub oid: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<TreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlobLine {
    pub number: u32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tokens: Option<Vec<Token>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlobView {
    pub path: String,
    pub oid: String,
    pub size: u64,
    pub is_binary: bool,
    pub lines: Vec<BlobLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlameLine {
    pub line_number: u32,
    pub text: String,
    pub oid: String,
    pub short_oid: String,
    pub author_name: String,
    pub time_unix: i64,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlameView {
    pub path: String,
    pub lines: Vec<BlameLine>,
}
