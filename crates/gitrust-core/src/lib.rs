use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSummary {
    pub path: String,
    pub git_dir: String,
}

pub fn summarize_repo(path: &Path) -> anyhow::Result<RepoSummary> {
    let repo = gix::open(path)?;
    Ok(RepoSummary {
        path: path.display().to_string(),
        git_dir: repo.git_dir().display().to_string(),
    })
}
