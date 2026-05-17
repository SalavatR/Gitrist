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

    let all = gitrust_core::log_commits(r.path(), 10, None, false).expect("log");
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].summary, "commit-3");
    assert_eq!(all[2].summary, "commit-1");

    let two = gitrust_core::log_commits(r.path(), 2, None, false).expect("log limit");
    assert_eq!(two.len(), 2);
}

#[test]
fn log_filters_on_query_against_summary_and_author() {
    let r = TestRepo::new();
    for (i, msg) in ["feat: bootstrap", "fix: tweak parser", "feat: shiny thing"]
        .iter()
        .enumerate()
    {
        r.write("a", &i.to_string());
        r.git(&["add", "a"]);
        r.git(&["commit", "-q", "-m", msg]);
    }
    let only_feats =
        gitrust_core::log_commits(r.path(), 10, Some("feat"), false).expect("filtered");
    assert_eq!(only_feats.len(), 2);
    assert!(only_feats.iter().all(|c| c.summary.contains("feat")));

    // Case-insensitive substring match.
    let parser = gitrust_core::log_commits(r.path(), 10, Some("PARSER"), false).expect("ci filter");
    assert_eq!(parser.len(), 1);
    assert!(parser[0].summary.contains("parser"));

    // Empty query falls through to unfiltered.
    let all = gitrust_core::log_commits(r.path(), 10, Some("   "), false).expect("empty q");
    assert_eq!(all.len(), 3);
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

    let oid = gitrust_core::commit(r.path(), "second", None).expect("commit");
    assert_eq!(oid.len(), 40);

    let log = gitrust_core::log_commits(r.path(), 10, None, false).unwrap();
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
fn list_staged_reports_renames_with_old_path() {
    let r = TestRepo::new();
    r.write("old-name.txt", "x\n");
    r.git(&["add", "old-name.txt"]);
    r.git(&["commit", "-q", "-m", "seed"]);
    // git mv re-stages the rename as a single index entry.
    r.git(&["mv", "old-name.txt", "new-name.txt"]);

    let staged = gitrust_core::list_staged(r.path()).expect("list_staged");
    let entry = staged
        .iter()
        .find(|e| e.path == "new-name.txt")
        .expect("renamed entry");
    assert_eq!(entry.kind, "renamed");
    assert_eq!(entry.old_path.as_deref(), Some("old-name.txt"));
    // The old path should not appear as a separate "deleted" entry.
    assert!(!staged.iter().any(|e| e.path == "old-name.txt"));
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
fn blame_attributes_lines_to_their_authoring_commits() {
    let r = TestRepo::new();
    // Two commits, each adding a distinct line — blame should attribute
    // each line to the commit that introduced it.
    r.write("notes.txt", "first\n");
    r.git(&["add", "notes.txt"]);
    r.git(&["commit", "-q", "-m", "introduce first"]);
    let oid_one = r.git(&["rev-parse", "HEAD"]).trim().to_string();

    r.write("notes.txt", "first\nsecond\n");
    r.git(&["add", "notes.txt"]);
    r.git(&["commit", "-q", "-m", "add second"]);
    let oid_two = r.git(&["rev-parse", "HEAD"]).trim().to_string();

    let view = gitrust_core::blame_file(r.path(), "notes.txt").expect("blame_file");
    assert_eq!(view.path, "notes.txt");
    assert_eq!(view.lines.len(), 2);

    assert_eq!(view.lines[0].line_number, 1);
    assert_eq!(view.lines[0].text, "first");
    assert_eq!(view.lines[0].oid, oid_one);
    assert_eq!(view.lines[0].author_name, "Test");
    assert_eq!(view.lines[0].summary, "introduce first");

    assert_eq!(view.lines[1].line_number, 2);
    assert_eq!(view.lines[1].text, "second");
    assert_eq!(view.lines[1].oid, oid_two);
    assert_eq!(view.lines[1].summary, "add second");
    assert!(view.lines[1].time_unix > 0);
}

#[test]
fn commit_rejects_empty_message() {
    let r = TestRepo::new();
    r.write("seed.txt", "x\n");
    r.git(&["add", "seed.txt"]);
    let err = gitrust_core::commit(r.path(), "   ", None).expect_err("empty message");
    assert!(err.to_string().contains("empty"));
}

#[test]
fn commit_honours_author_override() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    let oid = gitrust_core::commit(r.path(), "first", Some("Ghost Writer <ghost@example.com>"))
        .expect("commit");
    let info = gitrust_core::commit_info(r.path(), &oid).expect("commit_info");
    assert_eq!(info.author_name, "Ghost Writer");
    assert_eq!(info.author_email, "ghost@example.com");
    assert_eq!(info.summary, "first");
}

#[test]
fn commit_info_resolves_by_oid() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "first"]);
    let oid = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    let info = gitrust_core::commit_info(r.path(), &oid).expect("commit_info");
    assert_eq!(info.oid, oid);
    assert_eq!(info.summary, "first");
    assert_eq!(info.author_name, "Test");
    assert!(info.parents.is_empty());
}

#[test]
fn create_branch_with_switch_lands_on_new_head() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);

    gitrust_core::create_branch(r.path(), "feature", None, true).expect("create+switch");
    let cur = r.git(&["symbolic-ref", "--short", "HEAD"]);
    assert_eq!(cur.trim(), "feature");

    let bs = gitrust_core::list_branches(r.path()).expect("list");
    let names: std::collections::BTreeSet<&str> = bs.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains("master"));
    assert!(names.contains("feature"));
}

#[test]
fn create_branch_without_switch_keeps_head() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);

    gitrust_core::create_branch(r.path(), "feature", None, false).expect("create");
    let cur = r.git(&["symbolic-ref", "--short", "HEAD"]);
    assert_eq!(cur.trim(), "master");
}

#[test]
fn delete_branch_removes_a_merged_branch() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.git(&["branch", "doomed"]);

    gitrust_core::delete_branch(r.path(), "doomed", false).expect("delete");
    let bs = gitrust_core::list_branches(r.path()).expect("list");
    assert!(!bs.iter().any(|b| b.name == "doomed"));
}

#[test]
fn delete_branch_refuses_unmerged() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.git(&["checkout", "-q", "-b", "diverge"]);
    r.write("b", "y");
    r.git(&["add", "b"]);
    r.git(&["commit", "-q", "-m", "ahead"]);
    r.git(&["checkout", "-q", "master"]);

    let err = gitrust_core::delete_branch(r.path(), "diverge", false).expect_err("should refuse");
    assert!(
        err.to_string().to_lowercase().contains("not fully merged")
            || err.to_string().to_lowercase().contains("delete branch"),
        "expected unmerged-branch hint, got `{err}`"
    );
    // Force should drop it regardless.
    gitrust_core::delete_branch(r.path(), "diverge", true).expect("force delete");
    let bs = gitrust_core::list_branches(r.path()).expect("list");
    assert!(!bs.iter().any(|b| b.name == "diverge"));
}

#[test]
fn rename_branch_swaps_name() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.git(&["branch", "old-name"]);

    gitrust_core::rename_branch(r.path(), "old-name", "new-name").expect("rename");
    let bs = gitrust_core::list_branches(r.path()).expect("list");
    let names: std::collections::BTreeSet<&str> = bs.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains("new-name"));
    assert!(!names.contains("old-name"));
}

#[test]
fn rename_branch_refuses_when_target_exists() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.git(&["branch", "alpha"]);
    r.git(&["branch", "beta"]);
    let err = gitrust_core::rename_branch(r.path(), "alpha", "beta").expect_err("collide");
    assert!(
        err.to_string().to_lowercase().contains("already exists")
            || err.to_string().to_lowercase().contains("beta"),
        "expected collision hint, got `{err}`"
    );
}

#[test]
fn checkout_switches_head() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.git(&["branch", "side"]);

    gitrust_core::checkout(r.path(), "side").expect("checkout side");
    assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]).trim(), "side");

    gitrust_core::checkout(r.path(), "master").expect("checkout master");
    assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]).trim(), "master");
}

#[test]
fn markdown_inline_emphasis_gets_classed_via_merge() {
    let src = b"# Heading\n\nThis is **bold** text.\n";
    let lines =
        gitrust_core::highlight::highlight_per_line(src, "markdown").expect("markdown highlight");
    // Line 2 is the paragraph. Block grammar alone wouldn't classify
    // anything inside paragraphs; the inline-merge pass should
    // surface at least one token with a non-empty class (the
    // emphasis markers or the bold word itself).
    let paragraph = &lines[2];
    let has_classed = paragraph.iter().any(|t| !t.class.is_empty());
    assert!(
        has_classed,
        "inline markdown merge should classify part of the bold span, got {paragraph:?}"
    );
    // Concatenated text should still equal the original paragraph,
    // regardless of how the merge split it.
    let joined: String = paragraph.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(joined, "This is **bold** text.");
}

#[test]
fn stash_save_list_pop_round_trip() {
    let r = TestRepo::new();
    r.write("a.txt", "v1\n");
    r.git(&["add", "a.txt"]);
    r.git(&["commit", "-q", "-m", "init"]);
    // Make a worktree change so there's something to stash.
    r.write("a.txt", "v2\n");

    gitrust_core::stash_save(r.path(), Some("WIP test")).expect("save");

    // Listed and message preserved.
    let stashes = gitrust_core::stash_list(r.path()).expect("list");
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].index, 0);
    assert_eq!(stashes[0].ref_name, "stash@{0}");
    assert!(stashes[0].message.contains("WIP test"));
    assert!(stashes[0].time_unix > 0);

    // Worktree was reverted by the stash push.
    let on_disk = std::fs::read_to_string(r.path().join("a.txt")).unwrap();
    assert_eq!(on_disk, "v1\n");

    // Pop restores the worktree change and removes the stash.
    gitrust_core::stash_pop(r.path(), 0).expect("pop");
    let on_disk_after = std::fs::read_to_string(r.path().join("a.txt")).unwrap();
    assert_eq!(on_disk_after, "v2\n");
    let stashes_after = gitrust_core::stash_list(r.path()).expect("list");
    assert!(stashes_after.is_empty());
}

#[test]
fn stash_drop_discards_without_applying() {
    let r = TestRepo::new();
    r.write("a.txt", "v1\n");
    r.git(&["add", "a.txt"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.write("a.txt", "v2\n");
    gitrust_core::stash_save(r.path(), None).expect("save");

    gitrust_core::stash_drop(r.path(), 0).expect("drop");

    let stashes = gitrust_core::stash_list(r.path()).expect("list");
    assert!(stashes.is_empty());
    // Worktree is still on v1 — drop doesn't apply.
    let on_disk = std::fs::read_to_string(r.path().join("a.txt")).unwrap();
    assert_eq!(on_disk, "v1\n");
}

#[test]
fn discard_reverts_worktree_to_index() {
    let r = TestRepo::new();
    r.write("a.txt", "v1\n");
    r.git(&["add", "a.txt"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.write("a.txt", "v2\n");
    // Unstaged worktree change — discard should drop it.
    gitrust_core::discard_files(r.path(), &["a.txt".to_string()]).expect("discard");

    let on_disk = std::fs::read_to_string(r.path().join("a.txt")).unwrap();
    assert_eq!(on_disk, "v1\n");
    let st = gitrust_core::list_status(r.path()).unwrap();
    assert!(!st.iter().any(|e| e.path == "a.txt"));
}

#[test]
fn commit_info_rejects_garbage_oid() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "first"]);
    let err = gitrust_core::commit_info(r.path(), "not-a-hex-oid").expect_err("bad oid");
    assert!(
        err.to_string().to_lowercase().contains("invalid")
            || err.to_string().to_lowercase().contains("oid"),
        "expected helpful oid error, got `{err}`"
    );
}

#[test]
fn push_publishes_to_a_bare_remote() {
    let bare = TestRepo::new_bare();
    let work = TestRepo::new();
    work.write("a.txt", "v1\n");
    work.git(&["add", "a.txt"]);
    work.git(&["commit", "-q", "-m", "init"]);
    work.git(&[
        "remote",
        "add",
        "origin",
        &bare.path().display().to_string(),
    ]);

    let result =
        gitrust_core::push(work.path(), Some("origin"), Some("master"), false, true).expect("push");
    assert_eq!(result.op, "push");
    assert_eq!(result.remote, "origin");
    assert!(!result.summary.is_empty());

    // The bare side now resolves master to the same oid the worktree has.
    let local_head = work.git(&["rev-parse", "HEAD"]);
    let bare_master = run_git_str(bare.path(), &["rev-parse", "master"]);
    assert_eq!(local_head.trim(), bare_master.trim());
}

#[test]
fn fetch_advances_remote_tracking_ref_after_a_separate_push() {
    let bare = TestRepo::new_bare();

    // Cloner A: seed the bare with an initial commit.
    let alpha = TestRepo::new();
    alpha.write("seed", "1\n");
    alpha.git(&["add", "seed"]);
    alpha.git(&["commit", "-q", "-m", "alpha first"]);
    alpha.git(&[
        "remote",
        "add",
        "origin",
        &bare.path().display().to_string(),
    ]);
    alpha.git(&["push", "-q", "-u", "origin", "master"]);

    // Cloner B: also tracks the bare; will fetch what alpha pushes next.
    let beta = TestRepo::new();
    beta.git(&[
        "remote",
        "add",
        "origin",
        &bare.path().display().to_string(),
    ]);
    beta.git(&["fetch", "-q", "origin"]);
    let before = beta.git(&["rev-parse", "origin/master"]).trim().to_string();

    // Alpha lands a new commit on origin.
    alpha.write("seed", "2\n");
    alpha.git(&["add", "seed"]);
    alpha.git(&["commit", "-q", "-m", "alpha second"]);
    alpha.git(&["push", "-q", "origin", "master"]);

    // Beta does the fetch under test.
    let result = gitrust_core::fetch(beta.path(), Some("origin")).expect("fetch");
    assert_eq!(result.op, "fetch");
    assert_eq!(result.remote, "origin");

    let after = beta.git(&["rev-parse", "origin/master"]).trim().to_string();
    assert_ne!(before, after, "fetch should have updated origin/master");
    let alpha_head = alpha.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(after, alpha_head);
}

#[test]
fn pull_fast_forwards_when_remote_advanced() {
    let bare = TestRepo::new_bare();

    let alpha = TestRepo::new();
    alpha.write("seed", "1\n");
    alpha.git(&["add", "seed"]);
    alpha.git(&["commit", "-q", "-m", "alpha first"]);
    alpha.git(&[
        "remote",
        "add",
        "origin",
        &bare.path().display().to_string(),
    ]);
    alpha.git(&["push", "-q", "-u", "origin", "master"]);

    // Beta clones the bare so its master tracks origin/master.
    let beta_tempdir = tempfile::TempDir::new().expect("tempdir");
    run_git_str(
        beta_tempdir.path(),
        &["clone", "-q", &bare.path().display().to_string(), "."],
    );
    // Honor the same identity convention as TestRepo for follow-up commits
    // (none here, but symmetrical with the rest of the fixture).
    run_git_str(beta_tempdir.path(), &["config", "user.name", "Test"]);
    run_git_str(
        beta_tempdir.path(),
        &["config", "user.email", "test@example.com"],
    );
    let beta_path = beta_tempdir.path();
    let before = run_git_str(beta_path, &["rev-parse", "HEAD"])
        .trim()
        .to_string();

    // Alpha pushes a new commit.
    alpha.write("seed", "2\n");
    alpha.git(&["add", "seed"]);
    alpha.git(&["commit", "-q", "-m", "alpha second"]);
    alpha.git(&["push", "-q", "origin", "master"]);
    let alpha_head = alpha.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Beta pulls — fast-forward only is fine because beta has no local commits.
    let result = gitrust_core::pull(beta_path, Some("origin"), true).expect("pull");
    assert_eq!(result.op, "pull");
    let after = run_git_str(beta_path, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    assert_ne!(before, after);
    assert_eq!(after, alpha_head);
}

#[test]
fn pull_ff_only_refuses_diverged_history() {
    let bare = TestRepo::new_bare();

    let alpha = TestRepo::new();
    alpha.write("seed", "1\n");
    alpha.git(&["add", "seed"]);
    alpha.git(&["commit", "-q", "-m", "shared"]);
    alpha.git(&[
        "remote",
        "add",
        "origin",
        &bare.path().display().to_string(),
    ]);
    alpha.git(&["push", "-q", "-u", "origin", "master"]);

    // Beta diverges with its own commit on top of the shared base.
    let beta_tempdir = tempfile::TempDir::new().expect("tempdir");
    run_git_str(
        beta_tempdir.path(),
        &["clone", "-q", &bare.path().display().to_string(), "."],
    );
    run_git_str(beta_tempdir.path(), &["config", "user.name", "Test"]);
    run_git_str(
        beta_tempdir.path(),
        &["config", "user.email", "test@example.com"],
    );
    std::fs::write(beta_tempdir.path().join("local.txt"), "beta\n").unwrap();
    run_git_str(beta_tempdir.path(), &["add", "local.txt"]);
    run_git_str(
        beta_tempdir.path(),
        &["commit", "-q", "-m", "beta diverges"],
    );

    // Alpha lands an incompatible commit on origin.
    alpha.write("seed", "2\n");
    alpha.git(&["add", "seed"]);
    alpha.git(&["commit", "-q", "-m", "alpha advances"]);
    alpha.git(&["push", "-q", "origin", "master"]);

    let err = gitrust_core::pull(beta_tempdir.path(), Some("origin"), true)
        .expect_err("ff-only must refuse a non-ff merge");
    assert!(
        err.to_string().to_lowercase().contains("non-fast-forward")
            || err
                .to_string()
                .to_lowercase()
                .contains("not possible to fast-forward"),
        "expected ff-only refusal, got `{err}`"
    );
}

#[test]
fn log_all_walks_branches_outside_head_ancestry() {
    let r = TestRepo::new();
    r.write("a", "1");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "shared base"]);

    // Diverge: create `side` with its own commit, then switch back to
    // master and land an unrelated commit. The two branches now share
    // only the base; HEAD's log shows master's commit but not side's.
    r.git(&["checkout", "-q", "-b", "side"]);
    r.write("a", "side");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "side advances"]);
    r.git(&["checkout", "-q", "master"]);
    r.write("a", "master");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "master advances"]);

    let head_only = gitrust_core::log_commits(r.path(), 50, None, false).expect("head log");
    let head_summaries: Vec<&str> = head_only.iter().map(|c| c.summary.as_str()).collect();
    assert!(head_summaries.contains(&"master advances"));
    assert!(head_summaries.contains(&"shared base"));
    assert!(
        !head_summaries.contains(&"side advances"),
        "HEAD walk should not include side branch's tip"
    );

    let all_branches = gitrust_core::log_commits(r.path(), 50, None, true).expect("all log");
    let all_summaries: Vec<&str> = all_branches.iter().map(|c| c.summary.as_str()).collect();
    assert!(all_summaries.contains(&"master advances"));
    assert!(all_summaries.contains(&"side advances"));
    assert!(all_summaries.contains(&"shared base"));
}

#[test]
fn merge_fast_forwards_when_target_is_ahead() {
    let r = TestRepo::new();
    r.write("a", "1");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("a", "2");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "feature advances"]);
    let feature_head = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.git(&["checkout", "-q", "master"]);

    let result = gitrust_core::merge(r.path(), "feature", false).expect("merge");
    assert_eq!(result.op, "merge");
    assert_eq!(result.remote, "feature");
    let new_master = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(new_master, feature_head);
}

#[test]
fn merge_no_ff_creates_merge_commit() {
    let r = TestRepo::new();
    r.write("a", "1");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("a", "2");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "feature advances"]);
    let feature_head = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.git(&["checkout", "-q", "master"]);

    gitrust_core::merge(r.path(), "feature", true).expect("merge --no-ff");
    let new_master = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_ne!(new_master, feature_head, "--no-ff must mint a merge commit");
    let parents = r.git(&["rev-list", "--parents", "-n", "1", "HEAD"]);
    assert_eq!(
        parents.split_whitespace().count(),
        3,
        "merge commit should have two parents in addition to its own oid"
    );
}

#[test]
fn merge_reports_conflict_when_branches_collide() {
    let r = TestRepo::new();
    r.write("a", "base\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("a", "from feature\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "feature edits"]);
    r.git(&["checkout", "-q", "master"]);
    r.write("a", "from master\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "master edits"]);

    let err = gitrust_core::merge(r.path(), "feature", false).expect_err("conflicting merge");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("conflict") || msg.contains("automatic merge failed"),
        "expected conflict wording, got `{err}`"
    );
}

#[test]
fn cherry_pick_lands_a_commit_from_another_branch() {
    let r = TestRepo::new();
    r.write("a", "1\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("b", "feature-only\n");
    r.git(&["add", "b"]);
    r.git(&["commit", "-q", "-m", "feature: add b"]);
    let target_oid = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.git(&["checkout", "-q", "master"]);

    let result = gitrust_core::cherry_pick(r.path(), &target_oid).expect("cherry-pick");
    assert_eq!(result.op, "cherry-pick");
    assert_eq!(result.remote, target_oid);

    let head_msg = r.git(&["log", "-1", "--format=%s"]).trim().to_string();
    assert_eq!(head_msg, "feature: add b");
    let b_on_master = std::fs::read_to_string(r.path().join("b")).unwrap();
    assert_eq!(b_on_master, "feature-only\n");
}

#[test]
fn cherry_pick_reports_conflict_on_overlapping_edits() {
    let r = TestRepo::new();
    r.write("a", "base\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("a", "feature\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "feature edits a"]);
    let conflicting_oid = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.git(&["checkout", "-q", "master"]);
    r.write("a", "master\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "master edits a"]);

    let err =
        gitrust_core::cherry_pick(r.path(), &conflicting_oid).expect_err("conflicting cherry-pick");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("conflict") || msg.contains("could not apply"),
        "expected conflict wording, got `{err}`"
    );
}

/// Local replica of `run_git` for tests that need to drive a non-TestRepo
/// directory (e.g. a `git clone`d worktree we created via tempfile).
fn run_git_str(cwd: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("spawn git: {e}"));
    if !out.status.success() {
        panic!(
            "git {args:?} failed (status {})\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout).unwrap_or_default()
}

fn setup_conflicting_merge(r: &TestRepo) -> String {
    // Common ancestor + two divergent edits on the same line; returns
    // the oid of the branch we'll merge into HEAD.
    r.write("a", "base\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("a", "from feature\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "feature edits"]);
    let feature_oid = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.git(&["checkout", "-q", "master"]);
    r.write("a", "from master\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "master edits"]);
    feature_oid
}

#[test]
fn repo_state_is_clean_by_default() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);

    let state = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(state.kind, "clean");
    assert!(state.subject.is_none());
    assert!(state.conflicted.is_empty());
}

#[test]
fn repo_state_reports_merging_after_conflict() {
    let r = TestRepo::new();
    setup_conflicting_merge(&r);
    let _ = gitrust_core::merge(r.path(), "feature", false);
    let state = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(state.kind, "merging");
    assert!(
        state
            .subject
            .as_deref()
            .is_some_and(|s| s.contains("feature")),
        "MERGE_MSG subject should mention the target branch; got {:?}",
        state.subject
    );
    assert_eq!(state.conflicted, vec!["a".to_string()]);
}

#[test]
fn merge_abort_returns_to_clean() {
    let r = TestRepo::new();
    setup_conflicting_merge(&r);
    let _ = gitrust_core::merge(r.path(), "feature", false);

    gitrust_core::merge_abort(r.path()).expect("abort");
    let state = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(state.kind, "clean");
    // Worktree restored to pre-merge content.
    let on_disk = std::fs::read_to_string(r.path().join("a")).unwrap();
    assert_eq!(on_disk, "from master\n");
}

#[test]
fn resolve_to_ours_then_merge_continue_lands_a_merge_commit() {
    let r = TestRepo::new();
    setup_conflicting_merge(&r);
    let _ = gitrust_core::merge(r.path(), "feature", false);

    gitrust_core::resolve_file(r.path(), "a", "ours").expect("resolve ours");
    let on_disk = std::fs::read_to_string(r.path().join("a")).unwrap();
    assert_eq!(on_disk, "from master\n", "ours should keep master's edit");
    gitrust_core::merge_continue(r.path()).expect("continue");

    let state = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(state.kind, "clean");
    let parents = r.git(&["rev-list", "--parents", "-n", "1", "HEAD"]);
    assert_eq!(
        parents.split_whitespace().count(),
        3,
        "merge commit should have two parents"
    );
}

#[test]
fn resolve_to_theirs_takes_the_incoming_side() {
    let r = TestRepo::new();
    setup_conflicting_merge(&r);
    let _ = gitrust_core::merge(r.path(), "feature", false);

    gitrust_core::resolve_file(r.path(), "a", "theirs").expect("resolve theirs");
    let on_disk = std::fs::read_to_string(r.path().join("a")).unwrap();
    assert_eq!(on_disk, "from feature\n");
}

#[test]
fn resolve_file_rejects_unknown_side() {
    let r = TestRepo::new();
    setup_conflicting_merge(&r);
    let _ = gitrust_core::merge(r.path(), "feature", false);

    let err = gitrust_core::resolve_file(r.path(), "a", "mine").expect_err("bad side");
    assert!(err.to_string().contains("ours") || err.to_string().contains("theirs"));
}

#[test]
fn cherry_pick_abort_clears_the_in_progress_state() {
    let r = TestRepo::new();
    r.write("a", "base\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("a", "feature\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "feature edits"]);
    let oid = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.git(&["checkout", "-q", "master"]);
    r.write("a", "master\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "master edits"]);

    let _ = gitrust_core::cherry_pick(r.path(), &oid);
    let mid = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(mid.kind, "cherry-picking");
    assert_eq!(mid.conflicted, vec!["a".to_string()]);

    gitrust_core::cherry_pick_abort(r.path()).expect("abort");
    let after = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(after.kind, "clean");
}

#[test]
fn rebase_replays_branch_onto_upstream() {
    let r = TestRepo::new();
    r.write("a", "1\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);

    // master advances independently; feature carries one commit off the base.
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("b", "feature-only\n");
    r.git(&["add", "b"]);
    r.git(&["commit", "-q", "-m", "feature: add b"]);
    r.git(&["checkout", "-q", "master"]);
    r.write("a", "2\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "master: edit a"]);
    let new_master = r.git(&["rev-parse", "HEAD"]).trim().to_string();

    r.git(&["checkout", "-q", "feature"]);
    gitrust_core::rebase(r.path(), "master").expect("rebase");

    // After rebase feature's parent is master's HEAD.
    let parent = r.git(&["rev-parse", "HEAD~1"]).trim().to_string();
    assert_eq!(parent, new_master);
    // And the worktree carries both files.
    assert_eq!(std::fs::read_to_string(r.path().join("a")).unwrap(), "2\n");
    assert_eq!(
        std::fs::read_to_string(r.path().join("b")).unwrap(),
        "feature-only\n"
    );

    let state = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(state.kind, "clean");
}

#[test]
fn rebase_conflict_reports_rebasing_and_aborts_cleanly() {
    let r = TestRepo::new();
    r.write("a", "base\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("a", "feature\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "feature edits a"]);
    r.git(&["checkout", "-q", "master"]);
    r.write("a", "master\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "master edits a"]);

    r.git(&["checkout", "-q", "feature"]);
    let pre_head = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    let _ = gitrust_core::rebase(r.path(), "master");

    let mid = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(mid.kind, "rebasing");
    assert_eq!(mid.conflicted, vec!["a".to_string()]);

    gitrust_core::rebase_abort(r.path()).expect("abort");
    let after = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(after.kind, "clean");
    // feature's HEAD restored to what it was before the rebase.
    assert_eq!(r.git(&["rev-parse", "HEAD"]).trim(), pre_head);
}

#[test]
fn revert_creates_inverse_commit() {
    let r = TestRepo::new();
    // Each commit touches a distinct file so reverting the middle one
    // doesn't conflict with the later commits.
    r.write("a", "a\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "add a"]);
    r.write("b", "b\n");
    r.git(&["add", "b"]);
    r.git(&["commit", "-q", "-m", "add b"]);
    let to_revert = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.write("c", "c\n");
    r.git(&["add", "c"]);
    r.git(&["commit", "-q", "-m", "add c"]);

    let result = gitrust_core::revert(r.path(), &to_revert).expect("revert");
    assert_eq!(result.op, "revert");
    assert_eq!(result.remote, to_revert);

    let head_msg = r.git(&["log", "-1", "--format=%s"]).trim().to_string();
    assert!(
        head_msg.contains("Revert") && head_msg.contains("add b"),
        "expected Revert message, got `{head_msg}`"
    );
    assert!(!r.path().join("b").exists(), "b should be reverted away");
    assert!(r.path().join("a").exists());
    assert!(r.path().join("c").exists());
    let state = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(state.kind, "clean");
}

#[test]
fn revert_abort_clears_in_progress_state() {
    let r = TestRepo::new();
    r.write("a", "base\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);
    r.write("a", "second\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "second"]);
    let to_revert = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    // Subsequent edit conflicts with reverting "second": the revert
    // would re-introduce "base" but the file is now "third".
    r.write("a", "third\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "third"]);

    let _ = gitrust_core::revert(r.path(), &to_revert);
    let mid = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(mid.kind, "reverting");

    gitrust_core::revert_abort(r.path()).expect("abort");
    let after = gitrust_core::repo_state(r.path()).expect("state");
    assert_eq!(after.kind, "clean");
}

#[test]
fn reset_soft_moves_head_keeps_index_and_worktree() {
    let r = TestRepo::new();
    r.write("a", "v1\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "first"]);
    let first = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.write("a", "v2\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "second"]);

    gitrust_core::reset(r.path(), &first, "soft").expect("reset");
    assert_eq!(r.git(&["rev-parse", "HEAD"]).trim(), first);
    // Soft: worktree still has v2, and the second commit's change is
    // staged in the index (diff --cached against HEAD shows it).
    assert_eq!(std::fs::read_to_string(r.path().join("a")).unwrap(), "v2\n");
    let staged = gitrust_core::list_staged(r.path()).expect("staged");
    assert!(staged.iter().any(|e| e.path == "a"));
}

#[test]
fn reset_hard_discards_worktree_changes() {
    let r = TestRepo::new();
    r.write("a", "v1\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "first"]);
    let first = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.write("a", "v2\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "second"]);

    gitrust_core::reset(r.path(), &first, "hard").expect("reset --hard");
    assert_eq!(r.git(&["rev-parse", "HEAD"]).trim(), first);
    // Hard: worktree reverted to v1, nothing staged.
    assert_eq!(std::fs::read_to_string(r.path().join("a")).unwrap(), "v1\n");
    let staged = gitrust_core::list_staged(r.path()).expect("staged");
    assert!(staged.is_empty());
}

#[test]
fn reset_rejects_unknown_mode() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    let err = gitrust_core::reset(r.path(), "HEAD", "wobbly").expect_err("bad mode");
    let msg = err.to_string();
    assert!(msg.contains("soft") && msg.contains("hard"));
}

#[test]
fn stage_hunks_stages_a_single_selected_hunk() {
    let r = TestRepo::new();
    // 20-line file; commit a baseline.
    let baseline: String = (1..=20).map(|i| format!("line {i}\n")).collect();
    r.write("a", &baseline);
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);

    // Two non-adjacent edits → two hunks: line 3 and line 17.
    let mut edited: Vec<String> = (1..=20).map(|i| format!("line {i}\n")).collect();
    edited[2] = "line 3 EDITED\n".to_string();
    edited[16] = "line 17 EDITED\n".to_string();
    r.write("a", &edited.concat());

    // The working diff should report two hunks.
    let diff = gitrust_core::diff_working(r.path(), "a").expect("diff");
    assert_eq!(diff.hunks.len(), 2);

    // Stage only the second hunk (index 1).
    gitrust_core::stage_hunks(r.path(), "a", &[1]).expect("stage hunk 1");

    // Index now carries hunk 1's edit but not hunk 0's.
    let cached = r.git(&["diff", "--cached", "a"]);
    assert!(cached.contains("line 17 EDITED"), "hunk 1 should be staged");
    assert!(
        !cached.contains("line 3 EDITED"),
        "hunk 0 should NOT be staged"
    );

    // Working tree still has both edits — staging just copies into the
    // index, doesn't move the worktree.
    let on_disk = std::fs::read_to_string(r.path().join("a")).unwrap();
    assert!(on_disk.contains("line 3 EDITED"));
    assert!(on_disk.contains("line 17 EDITED"));
}

#[test]
fn stage_hunks_stages_all_when_every_index_picked() {
    let r = TestRepo::new();
    r.write("a", "1\n2\n3\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.write("a", "1 edit\n2\n3 edit\n");

    let diff = gitrust_core::diff_working(r.path(), "a").expect("diff");
    let all: Vec<usize> = (0..diff.hunks.len()).collect();
    gitrust_core::stage_hunks(r.path(), "a", &all).expect("stage all");

    // Everything staged → worktree-vs-index diff is empty.
    let cached = r.git(&["diff", "--cached", "a"]);
    assert!(cached.contains("1 edit"));
    assert!(cached.contains("3 edit"));
    let unstaged = r.git(&["diff", "a"]);
    assert!(
        unstaged.trim().is_empty(),
        "nothing should remain unstaged, got: {unstaged}"
    );
}

#[test]
fn stage_hunks_rejects_out_of_range_index() {
    let r = TestRepo::new();
    r.write("a", "x\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.write("a", "y\n");
    let err = gitrust_core::stage_hunks(r.path(), "a", &[42]).expect_err("out of range");
    assert!(err.to_string().contains("out of range"));
}

#[test]
fn stage_hunks_rejects_empty_selection() {
    let r = TestRepo::new();
    r.write("a", "x\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.write("a", "y\n");
    let err = gitrust_core::stage_hunks(r.path(), "a", &[]).expect_err("empty");
    assert!(err.to_string().contains("no hunks"));
}

#[test]
fn stage_hunks_rejects_untracked_file() {
    let r = TestRepo::new();
    r.write("a", "x\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    r.write("brand-new", "fresh\n");
    let err = gitrust_core::stage_hunks(r.path(), "brand-new", &[0]).expect_err("untracked");
    let msg = err.to_string();
    assert!(
        msg.contains("modified") || msg.contains("untracked"),
        "expected kind-related error, got `{msg}`"
    );
}

#[test]
fn scan_root_finds_nested_repos_and_skips_dotdirs() {
    let parent = tempfile::TempDir::new().expect("tempdir");
    // Three nested repos at depths 1, 2, and 3.
    let layout: &[(&str, &str)] = &[
        ("alpha", "alpha"),
        ("group/beta", "beta"),
        ("group/sub/gamma", "gamma"),
    ];
    for (rel, _name) in layout {
        let p = parent.path().join(rel);
        std::fs::create_dir_all(&p).unwrap();
        run_git_str(&p, &["init", "--initial-branch=master", "-q"]);
        run_git_str(&p, &["config", "user.name", "Test"]);
        run_git_str(&p, &["config", "user.email", "test@example.com"]);
        std::fs::write(p.join("README"), "seed\n").unwrap();
        run_git_str(&p, &["add", "README"]);
        run_git_str(&p, &["commit", "-q", "-m", "init"]);
    }
    // A `.cache` dotdir holding what looks like a repo should be skipped.
    let cache = parent.path().join(".cache").join("ignored");
    std::fs::create_dir_all(&cache).unwrap();
    run_git_str(&cache, &["init", "--initial-branch=master", "-q"]);

    let found = gitrust_core::scan_root(parent.path(), 5).expect("scan");
    let names: std::collections::BTreeSet<&str> = found.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains("alpha"));
    assert!(names.contains("beta"));
    assert!(names.contains("gamma"));
    assert!(
        !names.contains("ignored"),
        "scan should not descend into dotdirs"
    );
    for entry in &found {
        assert_eq!(entry.head_ref.as_deref(), Some("master"));
        assert!(entry.head_oid.as_ref().is_some_and(|o| o.len() == 40));
    }
}

#[test]
fn scan_root_respects_max_depth() {
    let parent = tempfile::TempDir::new().expect("tempdir");
    let deep = parent.path().join("a/b/c/d/e/repo");
    std::fs::create_dir_all(&deep).unwrap();
    run_git_str(&deep, &["init", "--initial-branch=master", "-q"]);
    run_git_str(&deep, &["config", "user.name", "Test"]);
    run_git_str(&deep, &["config", "user.email", "test@example.com"]);
    std::fs::write(deep.join("README"), "seed\n").unwrap();
    run_git_str(&deep, &["add", "README"]);
    run_git_str(&deep, &["commit", "-q", "-m", "init"]);

    // max_depth=3 reaches a/b/c but not a/b/c/d/e/repo (6 levels deep).
    let shallow = gitrust_core::scan_root(parent.path(), 3).expect("scan");
    assert!(shallow.is_empty(), "scan at depth 3 must not reach 6-deep");

    // max_depth=10 finds it.
    let deep_scan = gitrust_core::scan_root(parent.path(), 10).expect("scan");
    assert_eq!(deep_scan.len(), 1);
    assert_eq!(deep_scan[0].name, "repo");
}

#[test]
fn scan_root_returns_empty_for_a_root_with_no_repos() {
    let parent = tempfile::TempDir::new().expect("tempdir");
    std::fs::create_dir_all(parent.path().join("just/some/dirs")).unwrap();
    let found = gitrust_core::scan_root(parent.path(), 5).expect("scan");
    assert!(found.is_empty());
}

#[test]
fn scan_root_rejects_non_directory() {
    let parent = tempfile::TempDir::new().expect("tempdir");
    let file = parent.path().join("not-a-dir");
    std::fs::write(&file, "x").unwrap();
    let err = gitrust_core::scan_root(&file, 5).expect_err("not a dir");
    assert!(err.to_string().contains("not a directory"));
}

#[test]
fn create_lightweight_tag_then_delete() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);

    gitrust_core::create_tag(r.path(), "v1.0", None, None).expect("create lightweight");
    let tags = gitrust_core::list_tags(r.path()).expect("list");
    let v1 = tags
        .iter()
        .find(|t| t.name == "v1.0")
        .expect("v1.0 present");
    assert!(!v1.annotated, "no -m message → lightweight tag");

    gitrust_core::delete_tag(r.path(), "v1.0").expect("delete");
    let after = gitrust_core::list_tags(r.path()).expect("list");
    assert!(!after.iter().any(|t| t.name == "v1.0"));
}

#[test]
fn create_annotated_tag_with_message_at_specific_commit() {
    let r = TestRepo::new();
    r.write("a", "1\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "first"]);
    let first_oid = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.write("a", "2\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "second"]);

    gitrust_core::create_tag(r.path(), "v0", Some(&first_oid), Some("ship it"))
        .expect("create annotated");
    let tags = gitrust_core::list_tags(r.path()).expect("list");
    let v0 = tags.iter().find(|t| t.name == "v0").expect("v0 present");
    assert!(v0.annotated, "with -m → annotated tag");
    assert_eq!(v0.oid.as_deref(), Some(first_oid.as_str()));
}

#[test]
fn create_tag_rejects_collision() {
    let r = TestRepo::new();
    r.write("a", "x");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    gitrust_core::create_tag(r.path(), "dup", None, None).expect("first create");
    let err = gitrust_core::create_tag(r.path(), "dup", None, None).expect_err("collision");
    assert!(err.to_string().contains("already exists"));
}

#[test]
fn log_file_follows_renames() {
    let r = TestRepo::new();
    r.write("old-name.txt", "v1\n");
    r.git(&["add", "old-name.txt"]);
    r.git(&["commit", "-q", "-m", "intro"]);
    r.git(&["mv", "old-name.txt", "new-name.txt"]);
    r.git(&["commit", "-q", "-m", "rename"]);
    r.write("new-name.txt", "v2\n");
    r.git(&["add", "new-name.txt"]);
    r.git(&["commit", "-q", "-m", "edit"]);

    let hist = gitrust_core::log_file(r.path(), "new-name.txt", 20).expect("log_file");
    let subjects: Vec<&str> = hist.iter().map(|c| c.summary.as_str()).collect();
    assert!(subjects.contains(&"edit"));
    assert!(subjects.contains(&"rename"));
    // --follow means the pre-rename commit is included too.
    assert!(
        subjects.contains(&"intro"),
        "log --follow should reach the pre-rename commit, got {subjects:?}"
    );
}

#[test]
fn log_file_respects_limit() {
    let r = TestRepo::new();
    for i in 1..=5 {
        r.write("a", &format!("{i}\n"));
        r.git(&["add", "a"]);
        r.git(&["commit", "-q", "-m", &format!("c{i}")]);
    }
    let two = gitrust_core::log_file(r.path(), "a", 2).expect("log_file");
    assert_eq!(two.len(), 2);
    assert_eq!(two[0].summary, "c5");
    assert_eq!(two[1].summary, "c4");
}

#[test]
fn diff_refs_returns_changes_between_two_branches() {
    let r = TestRepo::new();
    r.write("a", "base\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "base"]);
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("a", "feature\n");
    r.write("only-on-feature", "x\n");
    r.git(&["add", "a", "only-on-feature"]);
    r.git(&["commit", "-q", "-m", "feature edits"]);

    let files = gitrust_core::diff_refs(r.path(), "master", "feature").expect("diff");
    let by_path: std::collections::BTreeMap<_, _> = files
        .iter()
        .map(|f| (f.path.as_str(), f.kind.as_str()))
        .collect();
    assert_eq!(by_path.get("a"), Some(&"modified"));
    assert_eq!(by_path.get("only-on-feature"), Some(&"added"));
}

#[test]
fn diff_refs_empty_when_endpoints_match() {
    let r = TestRepo::new();
    r.write("a", "x\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    let files = gitrust_core::diff_refs(r.path(), "HEAD", "HEAD").expect("diff");
    assert!(files.is_empty());
}

#[test]
fn diff_refs_rejects_unknown_ref() {
    let r = TestRepo::new();
    r.write("a", "x\n");
    r.git(&["add", "a"]);
    r.git(&["commit", "-q", "-m", "init"]);
    let err = gitrust_core::diff_refs(r.path(), "HEAD", "no-such-ref").expect_err("bad ref");
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("no-such-ref") || msg.contains("resolving"));
}
