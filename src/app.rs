use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, anyhow, bail};

const FIELD_SEPARATOR: char = '\u{1f}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Undecided,
    Delete,
    Keep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protection {
    Current,
    Worktree,
    Main,
    Master,
}

impl Protection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Worktree => "worktree",
            Self::Main => "main",
            Self::Master => "master",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub upstream: Option<String>,
    pub upstream_track: String,
    pub relative_date: String,
    pub subject: String,
    pub protections: Vec<Protection>,
    pub decision: Decision,
}

impl Branch {
    pub fn is_protected(&self) -> bool {
        !self.protections.is_empty()
    }

    pub fn is_deletable(&self) -> bool {
        !self.is_protected()
    }

    pub fn display_name(&self) -> String {
        if self.protections.is_empty() {
            return self.name.clone();
        }

        let labels = self
            .protections
            .iter()
            .map(|protection| format!("({})", protection.label()))
            .collect::<Vec<_>>()
            .join(" ");

        format!("{} {}", self.name, labels)
    }
}

#[derive(Debug, Clone)]
pub struct App {
    pub branches: Vec<Branch>,
    pub selected: usize,
}

impl App {
    pub fn load(repo: &Path) -> Result<Self> {
        let branches = scan_gone_branches(repo)?;
        Ok(Self {
            branches,
            selected: 0,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.branches.is_empty()
    }

    pub fn next(&mut self) {
        if self.branches.is_empty() {
            return;
        }

        self.selected = (self.selected + 1) % self.branches.len();
    }

    pub fn previous(&mut self) {
        if self.branches.is_empty() {
            return;
        }

        if self.selected == 0 {
            self.selected = self.branches.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn mark_delete(&mut self) {
        if let Some(branch) = self.branches.get_mut(self.selected)
            && branch.is_deletable()
        {
            branch.decision = Decision::Delete;
        }
    }

    pub fn mark_keep(&mut self) {
        if let Some(branch) = self.branches.get_mut(self.selected)
            && branch.is_deletable()
        {
            branch.decision = Decision::Keep;
        }
    }

    pub fn mark_all_delete(&mut self) {
        for branch in &mut self.branches {
            if branch.is_deletable() {
                branch.decision = Decision::Delete;
            }
        }
    }

    pub fn unmark_all(&mut self) {
        for branch in &mut self.branches {
            if branch.is_deletable() {
                branch.decision = Decision::Undecided;
            }
        }
    }

    pub fn delete_candidates(&self) -> Vec<&Branch> {
        self.branches
            .iter()
            .filter(|branch| branch.decision == Decision::Delete)
            .collect()
    }

    pub fn deletable_branches(&self) -> Vec<&Branch> {
        self.branches
            .iter()
            .filter(|branch| branch.is_deletable())
            .collect()
    }

    pub fn delete_count(&self) -> usize {
        self.delete_candidates().len()
    }
}

#[derive(Debug, Clone)]
pub struct DeleteResult {
    pub branch: String,
    pub success: bool,
    pub message: String,
}

pub fn scan_gone_branches(repo: &Path) -> Result<Vec<Branch>> {
    ensure_work_tree(repo)?;

    let current_branch = current_branch(repo)?;
    let worktree_branches = other_worktree_branches(repo, current_branch.as_deref())?;
    let lines = git_output(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)\u{1f}%(upstream:short)\u{1f}%(upstream:track)\u{1f}%(committerdate:relative)\u{1f}%(subject)",
            "refs/heads/",
        ],
    )?;

    let mut branches = Vec::new();
    for line in lines.lines().filter(|line| !line.trim().is_empty()) {
        let Some(branch) = parse_branch_line(line, current_branch.as_deref(), &worktree_branches)
        else {
            continue;
        };

        if branch.upstream_track.contains("[gone]") {
            branches.push(branch);
        }
    }

    branches.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(branches)
}

pub fn delete_branches(repo: &Path, branches: &[String]) -> Vec<DeleteResult> {
    branches
        .iter()
        .map(|branch| {
            let output = Command::new("git")
                .args(["branch", "-D", branch])
                .current_dir(repo)
                .output();

            match output {
                Ok(output) if output.status.success() => DeleteResult {
                    branch: branch.clone(),
                    success: true,
                    message: String::from_utf8_lossy(&output.stdout).trim().to_string(),
                },
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let message = if stderr.is_empty() { stdout } else { stderr };

                    DeleteResult {
                        branch: branch.clone(),
                        success: false,
                        message,
                    }
                }
                Err(error) => DeleteResult {
                    branch: branch.clone(),
                    success: false,
                    message: error.to_string(),
                },
            }
        })
        .collect()
}

fn parse_branch_line(
    line: &str,
    current_branch: Option<&str>,
    worktree_branches: &HashSet<String>,
) -> Option<Branch> {
    let mut fields = line.split(FIELD_SEPARATOR);
    let name = fields.next()?.to_string();
    let upstream = fields
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let upstream_track = fields.next()?.trim().to_string();
    let relative_date = fields.next()?.trim().to_string();
    let subject = fields.next()?.trim().to_string();

    let mut protections = Vec::new();
    if current_branch == Some(name.as_str()) {
        protections.push(Protection::Current);
    } else if worktree_branches.contains(&name) {
        protections.push(Protection::Worktree);
    }

    if name == "main" {
        protections.push(Protection::Main);
    }
    if name == "master" {
        protections.push(Protection::Master);
    }

    let decision = if protections.is_empty() {
        Decision::Undecided
    } else {
        Decision::Keep
    };

    Some(Branch {
        name,
        upstream,
        upstream_track,
        relative_date,
        subject,
        protections,
        decision,
    })
}

fn ensure_work_tree(repo: &Path) -> Result<()> {
    let inside_work_tree = git_output(repo, &["rev-parse", "--is-inside-work-tree"])?;
    if inside_work_tree.trim() != "true" {
        bail!("git-broom must be run inside a git working tree");
    }

    let is_bare = git_output(repo, &["rev-parse", "--is-bare-repository"])?;
    if is_bare.trim() == "true" {
        bail!("git-broom does not support bare repositories");
    }

    Ok(())
}

fn current_branch(repo: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .current_dir(repo)
        .output()
        .context("failed to determine current branch")?;

    if output.status.success() {
        let branch = String::from_utf8(output.stdout)
            .context("git symbolic-ref returned non-utf8 output")?
            .trim()
            .to_string();
        return Ok(Some(branch));
    }

    if output.status.code() == Some(1) {
        return Ok(None);
    }

    Err(anyhow!(
        "failed to determine current branch: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn other_worktree_branches(repo: &Path, current_branch: Option<&str>) -> Result<HashSet<String>> {
    let output = git_output(repo, &["worktree", "list", "--porcelain"])?;
    let mut branches = HashSet::new();

    for line in output.lines() {
        let Some(branch) = line.strip_prefix("branch refs/heads/") else {
            continue;
        };

        if Some(branch) != current_branch {
            branches.insert(branch.to_string());
        }
    }

    Ok(branches)
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String> {
    let output = git_output_raw(repo, args)?;
    String::from_utf8(output.stdout).context("git returned non-utf8 output")
}

fn git_output_raw(repo: &Path, args: &[&str]) -> Result<Output> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{Decision, Protection, parse_branch_line};

    #[test]
    fn parse_branch_line_marks_current_branch_as_protected() {
        let branch = parse_branch_line(
            "feature/foo\u{1f}origin/feature/foo\u{1f}[gone]\u{1f}2 days ago\u{1f}test subject",
            Some("feature/foo"),
            &HashSet::new(),
        )
        .expect("branch parsed");

        assert_eq!(branch.decision, Decision::Keep);
        assert_eq!(branch.protections, vec![Protection::Current]);
        assert!(branch.display_name().contains("(current)"));
    }

    #[test]
    fn parse_branch_line_marks_other_worktree_branch_as_protected() {
        let worktree_branches = HashSet::from([String::from("feature/foo")]);
        let branch = parse_branch_line(
            "feature/foo\u{1f}origin/feature/foo\u{1f}[gone]\u{1f}2 days ago\u{1f}test subject",
            Some("main"),
            &worktree_branches,
        )
        .expect("branch parsed");

        assert_eq!(branch.decision, Decision::Keep);
        assert_eq!(branch.protections, vec![Protection::Worktree]);
    }

    #[test]
    fn parse_branch_line_marks_main_branch_as_protected() {
        let branch = parse_branch_line(
            "main\u{1f}origin/main\u{1f}[gone]\u{1f}2 days ago\u{1f}test subject",
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");

        assert_eq!(branch.protections, vec![Protection::Main]);
        assert_eq!(branch.decision, Decision::Keep);
    }
}
