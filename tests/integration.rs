use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use git_broom::app::{Protection, scan_gone_branches};
use tempfile::TempDir;

#[test]
fn scan_returns_gone_branch_as_deletable() {
    let repo = TestRepo::new();
    repo.create_gone_branch("feature/gone");

    let branches = scan_gone_branches(repo.local_path()).expect("scan succeeds");
    let branch = branches
        .iter()
        .find(|branch| branch.name == "feature/gone")
        .expect("gone branch present");

    assert!(branch.protections.is_empty());
    assert!(branch.is_deletable());
}

#[test]
fn scan_marks_current_branch_as_protected() {
    let repo = TestRepo::new();
    repo.create_gone_branch("feature/current");
    repo.git_local(["checkout", "feature/current"]);

    let branches = scan_gone_branches(repo.local_path()).expect("scan succeeds");
    let branch = branches
        .iter()
        .find(|branch| branch.name == "feature/current")
        .expect("current gone branch present");

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

    let branches = scan_gone_branches(repo.local_path()).expect("scan succeeds");
    let branch = branches
        .iter()
        .find(|branch| branch.name == "feature/worktree")
        .expect("worktree branch present");

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

    let branches = scan_gone_branches(repo.local_path()).expect("scan succeeds");
    assert!(
        branches
            .iter()
            .any(|branch| branch.name == "feature/detached")
    );
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
        let file_name = branch.replace('/', "_");
        self.git_local(["checkout", "-b", branch]);
        fs::write(
            self.local.join(format!("{file_name}.txt")),
            format!("content for {branch}\n"),
        )
        .expect("branch file written");
        self.git_local(["add", "."]);
        self.git_local(["commit", "-m", &format!("Add {branch}")]);
        self.git_local(["push", "-u", "origin", branch]);
        self.git_local(["checkout", "main"]);
        self.git_local(["push", "origin", "--delete", branch]);
        self.git_local(["fetch", "--prune", "origin"]);
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
