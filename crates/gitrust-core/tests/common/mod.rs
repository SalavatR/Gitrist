//! Minimal git fixture for integration tests. Each `TestRepo` is a fresh
//! `tempfile::TempDir` initialised with a known branch name and committer
//! identity so commits succeed regardless of the host's global gitconfig.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

pub struct TestRepo {
    dir: TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("create tempdir");
        let path = dir.path();
        run(
            path,
            "git",
            &["-c", "init.defaultBranch=master", "init", "-q"],
        );
        run(path, "git", &["config", "user.name", "Test"]);
        run(path, "git", &["config", "user.email", "test@example.com"]);
        run(path, "git", &["config", "commit.gpgsign", "false"]);
        Self { dir }
    }

    /// Bare repo, suitable as a push/fetch target for other TestRepos
    /// via `git remote add origin <path>`. No identity / no worktree.
    pub fn new_bare() -> Self {
        let dir = TempDir::new().expect("create tempdir");
        let path = dir.path();
        run(
            path,
            "git",
            &["-c", "init.defaultBranch=master", "init", "--bare", "-q"],
        );
        Self { dir }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Write `contents` to `rel`, creating parent dirs as needed.
    pub fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let p = self.path().join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir -p");
        }
        std::fs::write(&p, contents).expect("write file");
        p
    }

    /// Run `git` in the repo and return stdout. Panics on non-zero exit so
    /// failures surface as test failures with a useful message.
    pub fn git(&self, args: &[&str]) -> String {
        run(self.path(), "git", args)
    }
}

fn run(cwd: &Path, prog: &str, args: &[&str]) -> String {
    let out = Command::new(prog)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("spawn {prog}: {e}"));
    if !out.status.success() {
        panic!(
            "{prog} {args:?} failed (status {})\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout).unwrap_or_default()
}
