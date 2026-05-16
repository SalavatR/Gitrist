//! Integration tests for the public read API of `gitrust-core`. Each test
//! builds a small synthetic repo with `git` CLI and asserts the wire-type
//! output. Run with `cargo test -p gitrust-core`.

#[path = "common/mod.rs"]
mod common;
use common::TestRepo;

#[test]
fn summarize_reports_head_on_fresh_commit() {
    let r = TestRepo::new();
    r.write("a.txt", "x\n");
    r.git(&["add", "a.txt"]);
    r.git(&["commit", "-q", "-m", "initial"]);

    let s = gitrust_core::summarize_repo(r.path()).expect("summarize");
    assert_eq!(s.head_ref.as_deref(), Some("master"));
    assert!(s.head_oid.as_ref().is_some_and(|o| o.len() == 40));
    assert!(!s.is_detached);
}

#[test]
fn log_returns_commits_newest_first_and_respects_limit() {
    let r = TestRepo::new();
    for i in 1..=3 {
        r.write("a", &format!("{i}\n"));
        r.git(&["add", "a"]);
        r.git(&["commit", "-q", "-m", &format!("commit-{i}")]);
    }

    let all = gitrust_core::log_commits(r.path(), 10).expect("log");
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].summary, "commit-3");
    assert_eq!(all[2].summary, "commit-1");

    let two = gitrust_core::log_commits(r.path(), 2).expect("log limit");
    assert_eq!(two.len(), 2);
}

#[test]
fn status_reports_modified_and_untracked() {
    let r = TestRepo::new();
    r.write("tracked.txt", "v1\n");
    r.git(&["add", "tracked.txt"]);
    r.git(&["commit", "-q", "-m", "init"]);

    r.write("tracked.txt", "v2\n");
    r.write("untracked.txt", "new\n");

    let st = gitrust_core::list_status(r.path()).expect("status");
    let by_path: std::collections::BTreeMap<_, _> =
        st.into_iter().map(|e| (e.path, e.kind)).collect();
    assert_eq!(
        by_path.get("tracked.txt").map(String::as_str),
        Some("modified")
    );
    assert_eq!(
        by_path.get("untracked.txt").map(String::as_str),
        Some("untracked")
    );
}

#[test]
fn list_branches_marks_head() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.git(&["branch", "feature"]);

    let bs = gitrust_core::list_branches(r.path()).expect("branches");
    let names: Vec<&str> = bs.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"master"));
    assert!(names.contains(&"feature"));
    let heads: Vec<&str> = bs
        .iter()
        .filter(|b| b.is_head)
        .map(|b| b.name.as_str())
        .collect();
    assert_eq!(heads, vec!["master"]);
}

#[test]
fn list_tags_distinguishes_lightweight_and_annotated() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.git(&["tag", "v1"]);
    r.git(&["tag", "-a", "v2", "-m", "release"]);

    let tags = gitrust_core::list_tags(r.path()).expect("tags");
    let by_name: std::collections::BTreeMap<_, _> =
        tags.into_iter().map(|t| (t.name, t.annotated)).collect();
    assert_eq!(by_name.get("v1").copied(), Some(false));
    assert_eq!(by_name.get("v2").copied(), Some(true));
}

#[test]
fn list_remote_branches_finds_tracking_refs() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    let oid = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    // Fabricate a tracking ref without needing a real remote.
    r.git(&["update-ref", "refs/remotes/origin/master", &oid]);
    r.git(&["update-ref", "refs/remotes/origin/feature", &oid]);

    let rs = gitrust_core::list_remote_branches(r.path()).expect("remotes");
    assert_eq!(rs.len(), 2);
    assert!(rs.iter().all(|r| r.remote == "origin"));
    let names: std::collections::BTreeSet<&str> = rs.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains("origin/master") || names.contains("master"));
    assert!(names.contains("origin/feature") || names.contains("feature"));
}

#[test]
fn list_tree_returns_nested_blobs() {
    let r = TestRepo::new();
    r.write("top.txt", "1");
    r.write("dir/inner.txt", "2");
    r.git(&["add", "."]);
    r.git(&["commit", "-q", "-m", "init"]);

    let tree = gitrust_core::list_tree(r.path()).expect("tree");
    let top = tree.iter().find(|t| t.name == "top.txt").expect("top.txt");
    assert_eq!(top.kind, "blob");
    assert_eq!(top.path, "top.txt");
    let dir = tree.iter().find(|t| t.name == "dir").expect("dir");
    assert_eq!(dir.kind, "tree");
    assert_eq!(dir.children.len(), 1);
    assert_eq!(dir.children[0].name, "inner.txt");
    assert_eq!(dir.children[0].path, "dir/inner.txt");
}

#[test]
fn show_blob_returns_numbered_lines() {
    let r = TestRepo::new();
    r.write("sample.txt", "line1\nline2\nline3\n");
    r.git(&["add", "sample.txt"]);
    r.git(&["commit", "-q", "-m", "init"]);

    let tree = gitrust_core::list_tree(r.path()).expect("tree");
    let oid = &tree
        .iter()
        .find(|t| t.name == "sample.txt")
        .expect("sample.txt entry")
        .oid;

    let blob = gitrust_core::show_blob(r.path(), oid, "sample.txt").expect("blob");
    assert!(!blob.is_binary);
    assert_eq!(blob.lines.len(), 3);
    assert_eq!(blob.lines[0].number, 1);
    assert_eq!(blob.lines[0].text, "line1");
    assert_eq!(blob.lines[2].text, "line3");
}

#[test]
fn diff_commit_returns_per_file_hunks_for_modification() {
    let r = TestRepo::new();
    r.write("a.txt", "v1\n");
    r.git(&["add", "a.txt"]);
    r.git(&["commit", "-q", "-m", "first"]);
    r.write("a.txt", "v2\n");
    r.git(&["add", "a.txt"]);
    r.git(&["commit", "-q", "-m", "second"]);

    let head_oid = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    let d = gitrust_core::diff_commit(r.path(), &head_oid).expect("diff_commit");
    assert_eq!(d.commit.summary, "second");
    assert_eq!(d.files.len(), 1);
    let f = &d.files[0];
    assert_eq!(f.path, "a.txt");
    assert_eq!(f.kind, "modified");
    assert!(!f.is_binary);
    let kinds: Vec<&str> = f
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter().map(|l| l.kind.as_str()))
        .collect();
    assert!(kinds.contains(&"del"));
    assert!(kinds.contains(&"add"));
}

#[test]
fn diff_working_shows_modified_file() {
    let r = TestRepo::new();
    r.write("a.txt", "before\n");
    r.git(&["add", "a.txt"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.write("a.txt", "after\n");

    let d = gitrust_core::diff_working(r.path(), "a.txt").expect("diff_working");
    assert_eq!(d.path, "a.txt");
    assert_eq!(d.kind, "modified");
    assert!(!d.is_binary);
    assert!(!d.hunks.is_empty());
}

#[test]
fn stage_moves_untracked_to_index() {
    let r = TestRepo::new();
    r.write("seed.txt", "s\n");
    r.git(&["add", "seed.txt"]);
    r.git(&["commit", "-q", "-m", "seed"]);
    r.write("new.txt", "hello\n");

    let pre = gitrust_core::list_status(r.path()).unwrap();
    assert!(
        pre.iter()
            .any(|e| e.path == "new.txt" && e.kind == "untracked")
    );

    gitrust_core::stage_files(r.path(), &["new.txt".to_string()]).expect("stage");

    let staged = r.git(&["ls-files", "--cached"]);
    assert!(
        staged.lines().any(|l| l == "new.txt"),
        "new.txt should be in the index, got: {staged:?}"
    );
}

#[test]
fn unstage_drops_index_entry_for_added_file() {
    let r = TestRepo::new();
    r.write("seed.txt", "s\n");
    r.git(&["add", "seed.txt"]);
    r.git(&["commit", "-q", "-m", "seed"]);
    r.write("new.txt", "hello\n");
    r.git(&["add", "new.txt"]);

    gitrust_core::unstage_files(r.path(), &["new.txt".to_string()]).expect("unstage");

    let staged = r.git(&["ls-files", "--cached"]);
    assert!(
        !staged.lines().any(|l| l == "new.txt"),
        "new.txt should be out of the index, got: {staged:?}"
    );
    // And it's back to untracked in the worktree.
    let st = gitrust_core::list_status(r.path()).unwrap();
    assert!(
        st.iter()
            .any(|e| e.path == "new.txt" && e.kind == "untracked")
    );
}

#[test]
fn commit_creates_new_commit_with_staged_changes() {
    let r = TestRepo::new();
    r.write("seed.txt", "v1\n");
    r.git(&["add", "seed.txt"]);
    r.git(&["commit", "-q", "-m", "first"]);

    r.write("seed.txt", "v2\n");
    gitrust_core::stage_files(r.path(), &["seed.txt".to_string()]).unwrap();

    let oid = gitrust_core::commit(r.path(), "second").expect("commit");
    assert_eq!(oid.len(), 40);

    let log = gitrust_core::log_commits(r.path(), 10).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].summary, "second");
    assert_eq!(log[0].oid, oid);
}

#[test]
fn list_staged_reports_added_and_modified_entries() {
    let r = TestRepo::new();
    r.write("seed.txt", "v1\n");
    r.git(&["add", "seed.txt"]);
    r.git(&["commit", "-q", "-m", "init"]);

    // Stage a new file and a modification of the existing one.
    r.write("new.txt", "n\n");
    r.write("seed.txt", "v2\n");
    r.git(&["add", "new.txt", "seed.txt"]);

    let staged = gitrust_core::list_staged(r.path()).expect("list_staged");
    let by_path: std::collections::BTreeMap<_, _> =
        staged.into_iter().map(|e| (e.path, e.kind)).collect();
    assert_eq!(by_path.get("new.txt").map(String::as_str), Some("added"));
    assert_eq!(
        by_path.get("seed.txt").map(String::as_str),
        Some("modified")
    );
}

#[test]
fn list_staged_handles_unborn_head() {
    let r = TestRepo::new();
    r.write("a.txt", "x\n");
    r.git(&["add", "a.txt"]);
    // No commit yet — HEAD is unborn.
    let staged = gitrust_core::list_staged(r.path()).expect("list_staged on unborn HEAD");
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].path, "a.txt");
    assert_eq!(staged[0].kind, "added");
}

#[test]
fn commit_rejects_empty_message() {
    let r = TestRepo::new();
    r.write("seed.txt", "x\n");
    r.git(&["add", "seed.txt"]);
    let err = gitrust_core::commit(r.path(), "   ").expect_err("empty message");
    assert!(err.to_string().contains("empty"));
}
