use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use git_broom::app::{
    Branch, CleanupMode, Decision, Protection, delete_branches, scan_selected_modes,
};
use git_broom::keep_store::KeepStore;
use tempfile::TempDir;

#[test]
fn scan_returns_gone_branch_as_deletable() {
    let repo = TestRepo::new();
    repo.create_gone_branch("feature/gone");

    let groups = scan_selected_modes(repo.local_path(), &[CleanupMode::Gone], "origin")
        .expect("scan succeeds");
    let branch = find_branch(&groups, CleanupMode::Gone, "feature/gone");

    assert!(branch.protections.is_empty());
    assert!(branch.is_deletable());
}

#[test]
fn scan_returns_unpushed_branch_as_deletable() {
    let repo = TestRepo::new();
    repo.create_unpushed_branch("feature/local-only");

    let groups = scan_selected_modes(repo.local_path(), &[CleanupMode::Unpushed], "origin")
        .expect("scan succeeds");
    let branch = find_branch(&groups, CleanupMode::Unpushed, "feature/local-only");

    assert!(branch.protections.is_empty());
    assert!(branch.is_deletable());
}

#[test]
fn scan_marks_current_branch_as_protected() {
    let repo = TestRepo::new();
    repo.create_gone_branch("feature/current");
    repo.git_local(["checkout", "feature/current"]);

    let groups = scan_selected_modes(repo.local_path(), &[CleanupMode::Gone], "origin")
        .expect("scan succeeds");
    let branch = find_branch(&groups, CleanupMode::Gone, "feature/current");

    assert_eq!(branch.protections, vec![Protection::Current]);
    assert!(!branch.is_deletable());
}

#[test]
fn scan_marks_other_worktree_branch_as_protected() {
    let repo = TestRepo::new();
    repo.create_gone_branch("feature/worktree");
    let worktree_path = repo.temp_path("linked-worktree");
    repo.git_local([
        "worktree",
        "add",
        worktree_path.to_str().expect("utf8 path"),
        "feature/worktree",
    ]);

    let groups = scan_selected_modes(repo.local_path(), &[CleanupMode::Gone], "origin")
        .expect("scan succeeds");
    let branch = find_branch(&groups, CleanupMode::Gone, "feature/worktree");

    assert_eq!(branch.protections, vec![Protection::Worktree]);
    assert!(!branch.is_deletable());
}

#[test]
fn scan_works_from_detached_head() {
    let repo = TestRepo::new();
    repo.create_gone_branch("feature/detached");
    let main_head = repo
        .git_local_stdout(["rev-parse", "main"])
        .trim()
        .to_string();
    repo.git_local(["checkout", "--detach", &main_head]);

    let groups = scan_selected_modes(repo.local_path(), &[CleanupMode::Gone], "origin")
        .expect("scan succeeds");
    assert!(
        groups[0]
            .branches
            .iter()
            .any(|branch| branch.name == "feature/detached")
    );
}

#[test]
fn scan_keeps_gone_and_unpushed_groups_separate() {
    let repo = TestRepo::new();
    repo.create_gone_branch("feature/gone");
    repo.create_unpushed_branch("feature/local-only");

    let groups = scan_selected_modes(
        repo.local_path(),
        &[CleanupMode::Gone, CleanupMode::Unpushed],
        "origin",
    )
    .expect("scan succeeds");

    assert!(
        find_group(&groups, CleanupMode::Gone)
            .branches
            .iter()
            .any(|branch| branch.name == "feature/gone")
    );
    assert!(
        !find_group(&groups, CleanupMode::Gone)
            .branches
            .iter()
            .any(|branch| branch.name == "feature/local-only")
    );

    assert!(
        find_group(&groups, CleanupMode::Unpushed)
            .branches
            .iter()
            .any(|branch| branch.name == "feature/local-only")
    );
    assert!(
        !find_group(&groups, CleanupMode::Unpushed)
            .branches
            .iter()
            .any(|branch| branch.name == "feature/gone")
    );
}

#[test]
fn dry_run_groups_closed_mode_by_reason() {
    let repo = TestRepo::new();
    repo.create_remote_tracked_branch("feature/closed", "origin");
    repo.create_remote_tracked_branch("feature/merged", "origin");
    repo.create_remote_tracked_branch("feature/open", "origin");
    repo.create_remote_tracked_branch("feature/no-pr", "origin");

    let fake_gh_dir = repo.install_fake_gh(
        r#"[{"number":1,"title":"Closed PR","state":"CLOSED","headRefName":"feature/closed","url":"https://example.test/pr/1"},{"number":2,"title":"Merged PR","state":"MERGED","headRefName":"feature/merged","url":"https://example.test/pr/2"},{"number":3,"title":"Open PR","state":"OPEN","headRefName":"feature/open","url":"https://example.test/pr/3"}]"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_git-broom"))
        .args(["closed", "--dry-run"])
        .current_dir(repo.local_path())
        .env("PATH", path_with_prefix(&fake_gh_dir))
        .output()
        .expect("git-broom runs");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("[closed: closed pull request or no pull request on GitHub]"));
    assert!(stdout.contains("pull request"));
    assert!(stdout.contains("feature/closed"));
    assert!(stdout.contains("https://example.test/pr/1"));
    assert!(stdout.contains("no PR"));
    assert!(stdout.contains("feature/no-pr"));
    assert!(stdout.contains("[merged: pull request merged but remote branch still exists]"));
    assert!(stdout.contains("feature/merged"));
    assert!(stdout.contains("https://example.test/pr/2"));
    assert!(!stdout.contains("feature/open"));
}

#[test]
fn closed_mode_excludes_open_prs_found_via_head_search() {
    let repo = TestRepo::new();
    repo.create_remote_tracked_branch("feature/open", "origin");
    repo.create_remote_tracked_branch("feature/no-pr", "origin");

    let fake_gh_dir = repo.install_fake_gh_with_head_search(
        "feature/open",
        r#"[{"state":"OPEN","headRefName":"feature/open","url":"https://example.test/pr/open"}]"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_git-broom"))
        .args(["closed", "--dry-run"])
        .current_dir(repo.local_path())
        .env("PATH", path_with_prefix(&fake_gh_dir))
        .output()
        .expect("git-broom runs");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.contains("feature/open"));
    assert!(stdout.contains("feature/no-pr"));
    assert!(stdout.contains("no PR"));
}

#[test]
fn closed_mode_excludes_merged_branches_whose_remote_is_already_gone() {
    let repo = TestRepo::new();
    repo.create_remote_tracked_branch("feature/merged-gone", "origin");
    repo.git_local(["push", "origin", "--delete", "feature/merged-gone"]);

    let fake_gh_dir = repo.install_fake_gh_with_head_search(
        "feature/merged-gone",
        r#"[{"state":"MERGED","headRefName":"feature/merged-gone","url":"https://example.test/pr/merged"}]"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_git-broom"))
        .args(["closed", "--dry-run"])
        .current_dir(repo.local_path())
        .env("PATH", path_with_prefix(&fake_gh_dir))
        .output()
        .expect("git-broom runs");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.contains("feature/merged-gone"));
    assert!(!stdout.contains("https://example.test/pr/merged"));
}

#[test]
fn saved_labels_persist_under_git_common_dir_and_reload_on_next_scan() {
    let repo = TestRepo::new();
    repo.create_unpushed_branch("feature/keep");
    repo.create_unpushed_branch("feature/regular");

    let mut keep_store = KeepStore::load(repo.local_path()).expect("keep store loads");
    keep_store
        .replace_mode(CleanupMode::Unpushed, [String::from("feature/keep")])
        .expect("keep store persists");

    let store_path = repo.keep_store_path();
    assert!(store_path.exists());
    assert!(store_path.ends_with(Path::new(".git/git-broom/keep-labels.json")));

    let groups = scan_selected_modes(repo.local_path(), &[CleanupMode::Unpushed], "origin")
        .expect("scan succeeds");
    let group = find_group(&groups, CleanupMode::Unpushed);

    assert_eq!(group.branches[0].name, "feature/keep");
    assert!(group.branches[0].saved);
    assert_eq!(group.branches[1].name, "feature/regular");
    assert!(!group.branches[1].saved);
}

#[test]
fn malformed_keep_store_aborts_scan_with_store_path() {
    let repo = TestRepo::new();
    repo.create_unpushed_branch("feature/keep");
    let store_path = repo.keep_store_path();
    fs::create_dir_all(store_path.parent().expect("keep store parent")).expect("store dir created");
    fs::write(&store_path, "{not valid json").expect("broken store written");

    let error =
        scan_selected_modes(repo.local_path(), &[CleanupMode::Unpushed], "origin").unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("failed to parse keep-label store"));
    assert!(message.contains(store_path.to_str().expect("utf8 store path")));
}

#[test]
fn delete_closed_branch_removes_remote_then_local() {
    let repo = TestRepo::new();
    repo.create_remote_tracked_branch("feature/closed", "origin");

    let branch = tracked_branch("feature/closed", "origin");
    let results = delete_branches(repo.local_path(), CleanupMode::Closed, "origin", &[&branch]);

    assert_eq!(results.len(), 1);
    assert!(results[0].success, "{}", results[0].message);
    assert!(!repo.local_branch_exists("feature/closed"));
    assert!(!repo.remote_branch_exists("feature/closed"));
}

#[test]
fn delete_closed_branch_keeps_local_branch_when_remote_delete_fails() {
    let repo = TestRepo::new();
    repo.create_remote_tracked_branch("feature/closed", "origin");
    let broken_remote = repo.temp_path("missing-remote.git");
    repo.git_local([
        "remote",
        "set-url",
        "origin",
        broken_remote.to_str().expect("utf8 path"),
    ]);

    let branch = tracked_branch("feature/closed", "origin");
    let results = delete_branches(repo.local_path(), CleanupMode::Closed, "origin", &[&branch]);

    assert_eq!(results.len(), 1);
    assert!(!results[0].success);
    assert!(repo.local_branch_exists("feature/closed"));
}

struct TestRepo {
    _root: TempDir,
    _remote: TempDir,
    local: PathBuf,
}

impl TestRepo {
    fn new() -> Self {
        let root = TempDir::new().expect("temp root created");
        let remote = TempDir::new().expect("temp remote created");
        let local = root.path().join("local");
        let remote_path = remote.path().to_path_buf();

        git(&remote_path, ["init", "--bare"]);
        fs::create_dir(&local).expect("local repo dir created");
        git(&local, ["init", "--initial-branch=main"]);
        git(&local, ["config", "user.name", "Test User"]);
        git(&local, ["config", "user.email", "test@example.com"]);
        fs::write(local.join("README.md"), "git-broom test repo\n").expect("readme written");
        git(&local, ["add", "README.md"]);
        git(&local, ["commit", "-m", "Initial commit"]);
        git(
            &local,
            [
                "remote",
                "add",
                "origin",
                remote_path.to_str().expect("utf8 path"),
            ],
        );
        git(&local, ["push", "-u", "origin", "main"]);

        Self {
            _root: root,
            _remote: remote,
            local,
        }
    }

    fn create_gone_branch(&self, branch: &str) {
        self.create_local_branch(branch);
        self.git_local(["push", "-u", "origin", branch]);
        self.git_local(["checkout", "main"]);
        self.git_local(["push", "origin", "--delete", branch]);
        self.git_local(["fetch", "--prune", "origin"]);
    }

    fn create_unpushed_branch(&self, branch: &str) {
        self.create_local_branch(branch);
        self.git_local(["checkout", "main"]);
    }

    fn create_remote_tracked_branch(&self, branch: &str, remote: &str) {
        self.create_local_branch(branch);
        self.git_local(["push", "-u", remote, branch]);
        self.git_local(["checkout", "main"]);
    }

    fn create_local_branch(&self, branch: &str) {
        let file_name = branch.replace('/', "_");
        self.git_local(["checkout", "-b", branch]);
        fs::write(
            self.local.join(format!("{file_name}.txt")),
            format!("content for {branch}\n"),
        )
        .expect("branch file written");
        self.git_local(["add", "."]);
        self.git_local(["commit", "-m", &format!("Add {branch}")]);
    }

    fn git_local<const N: usize>(&self, args: [&str; N]) {
        git(&self.local, args);
    }

    fn git_local_stdout<const N: usize>(&self, args: [&str; N]) -> String {
        git_stdout(&self.local, args)
    }

    fn local_path(&self) -> &Path {
        &self.local
    }

    fn temp_path(&self, name: &str) -> PathBuf {
        self._root.path().join(name)
    }

    fn keep_store_path(&self) -> PathBuf {
        PathBuf::from(
            self.git_local_stdout(["rev-parse", "--path-format=absolute", "--git-common-dir"])
                .trim(),
        )
        .join("git-broom/keep-labels.json")
    }

    fn install_fake_gh(&self, pr_list_json: &str) -> PathBuf {
        let bin_dir = self.temp_path("fake-bin");
        fs::create_dir_all(&bin_dir).expect("fake bin dir created");
        let script_path = bin_dir.join("gh");
        let script = format!(
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo 'gh version 999.0.0'
  exit 0
fi
if [ "$1" = "auth" ] && [ "$2" = "status" ]; then
  exit 0
fi
if [ "$1" = "repo" ] && [ "$2" = "view" ]; then
  printf '%s' '{{"defaultBranchRef":{{"name":"main"}}}}'
  exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "list" ]; then
  cat <<'EOF'
{pr_list_json}
EOF
  exit 0
fi
printf 'unexpected gh invocation: %s\n' "$*" >&2
exit 1
"#
        );
        fs::write(&script_path, script).expect("fake gh written");
        let output = Command::new("chmod")
            .args(["+x", script_path.to_str().expect("utf8 path")])
            .output()
            .expect("chmod runs");
        assert!(output.status.success(), "chmod failed");
        bin_dir
    }

    fn install_fake_gh_with_head_search(
        &self,
        search_term: &str,
        head_search_json: &str,
    ) -> PathBuf {
        let bin_dir = self.temp_path("fake-bin-head-search");
        fs::create_dir_all(&bin_dir).expect("fake bin dir created");
        let script_path = bin_dir.join("gh");
        let script = format!(
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo 'gh version 999.0.0'
  exit 0
fi
if [ "$1" = "auth" ] && [ "$2" = "status" ]; then
  exit 0
fi
if [ "$1" = "repo" ] && [ "$2" = "view" ]; then
  printf '%s' '{{"defaultBranchRef":{{"name":"main"}}}}'
  exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "list" ]; then
  case "$*" in
    *{search_term}*)
      cat <<'EOF'
{head_search_json}
EOF
      ;;
    *)
      printf '[]'
      ;;
  esac
  exit 0
fi
printf 'unexpected gh invocation: %s\n' "$*" >&2
exit 1
"#
        );
        fs::write(&script_path, script).expect("fake gh written");
        let output = Command::new("chmod")
            .args(["+x", script_path.to_str().expect("utf8 path")])
            .output()
            .expect("chmod runs");
        assert!(output.status.success(), "chmod failed");
        bin_dir
    }

    fn local_branch_exists(&self, branch: &str) -> bool {
        let output = self.git_local_stdout(["branch", "--list", branch]);
        output.lines().any(|line| line.trim_end().ends_with(branch))
    }

    fn remote_branch_exists(&self, branch: &str) -> bool {
        self.git_local_stdout(["ls-remote", "--heads", "origin", branch])
            .lines()
            .any(|line| !line.trim().is_empty())
    }
}

fn find_group(
    groups: &[git_broom::app::CleanupGroup],
    mode: CleanupMode,
) -> &git_broom::app::CleanupGroup {
    groups
        .iter()
        .find(|group| group.mode == mode)
        .expect("group present")
}

fn find_branch<'a>(
    groups: &'a [git_broom::app::CleanupGroup],
    mode: CleanupMode,
    branch_name: &str,
) -> &'a git_broom::app::Branch {
    find_group(groups, mode)
        .branches
        .iter()
        .find(|branch| branch.name == branch_name)
        .expect("branch present")
}

fn git<const N: usize>(repo: &Path, args: [&str; N]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git command runs");

    assert!(
        output.status.success(),
        "git {:?} failed\nstdout: {}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout<const N: usize>(repo: &Path, args: [&str; N]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git command runs");

    assert!(
        output.status.success(),
        "git {:?} failed\nstdout: {}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("utf8 git output")
}

fn tracked_branch(name: &str, remote: &str) -> Branch {
    Branch {
        name: name.to_string(),
        upstream: Some(format!("{remote}/{name}")),
        upstream_track: String::new(),
        committed_at: 1_700_000_000,
        relative_date: String::from("1 day ago"),
        subject: String::from("subject"),
        pr_url: None,
        detail: None,
        saved: false,
        protections: Vec::new(),
        decision: Decision::Delete,
    }
}

fn path_with_prefix(prefix: &Path) -> String {
    let existing = env::var("PATH").unwrap_or_default();
    format!("{}:{existing}", prefix.display())
}
