use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::keep_store::KeepStore;
use crate::pr_cache::{CachedPullRequestRecord, PrCache, PrCacheRemoteEntry};

const FIELD_SEPARATOR: char = '\u{1f}';
const PR_CACHE_TTL_SECONDS: i64 = 10 * 60;

pub const DEFAULT_PREVIEW_GROUPS: [CleanupMode; 6] = [
    CleanupMode::Gone,
    CleanupMode::Unpushed,
    CleanupMode::Pr,
    CleanupMode::NoPr,
    CleanupMode::Closed,
    CleanupMode::Merged,
];

pub const DEFAULT_CLEAN_GROUPS: [CleanupMode; 5] = [
    CleanupMode::Gone,
    CleanupMode::Unpushed,
    CleanupMode::NoPr,
    CleanupMode::Closed,
    CleanupMode::Merged,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanIntent {
    Preview,
    Clean,
}

#[derive(Debug, Clone, Copy)]
pub struct ScanOptions<'a> {
    pub modes: &'a [CleanupMode],
    pub remote: &'a str,
    pub intent: ScanIntent,
}

impl<'a> ScanOptions<'a> {
    pub fn preview(modes: &'a [CleanupMode], remote: &'a str) -> Self {
        Self {
            modes,
            remote,
            intent: ScanIntent::Preview,
        }
    }

    pub fn clean(modes: &'a [CleanupMode], remote: &'a str) -> Self {
        Self {
            modes,
            remote,
            intent: ScanIntent::Clean,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScanOutcome {
    pub groups: Vec<CleanupGroup>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CleanupMode {
    Gone,
    Unpushed,
    Pr,
    NoPr,
    Closed,
    Merged,
}

impl CleanupMode {
    pub fn from_arg(value: &str) -> Option<Self> {
        match value {
            "gone" => Some(Self::Gone),
            "unpushed" => Some(Self::Unpushed),
            "pr" => Some(Self::Pr),
            "nopr" => Some(Self::NoPr),
            "closed" => Some(Self::Closed),
            "merged" => Some(Self::Merged),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Gone => "gone",
            Self::Unpushed => "unpushed",
            Self::Pr => "pr",
            Self::NoPr => "nopr",
            Self::Closed => "closed",
            Self::Merged => "merged",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Gone => "gone",
            Self::Unpushed => "unpushed",
            Self::Pr => "PR",
            Self::NoPr => "No PR",
            Self::Closed => "closed",
            Self::Merged => "merged",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Gone => "upstream branch no longer exists",
            Self::Unpushed => "no upstream tracking branch is configured",
            Self::Pr => "open pull request on GitHub",
            Self::NoPr => "no pull request found on GitHub",
            Self::Closed => "pull request closed on GitHub",
            Self::Merged => "pull request merged but remote branch still exists",
        }
    }

    pub fn no_matches_message(self) -> &'static str {
        match self {
            Self::Gone => "No gone branches found.",
            Self::Unpushed => "No unpushed branches found.",
            Self::Pr => "No open PR branches found.",
            Self::NoPr => "No branches without PRs found.",
            Self::Closed => "No closed branches found.",
            Self::Merged => "No merged branches found.",
        }
    }

    pub fn uses_pr_metadata(self) -> bool {
        matches!(self, Self::Pr | Self::NoPr | Self::Closed | Self::Merged)
    }

    pub fn is_cleanable(self) -> bool {
        !matches!(self, Self::Pr)
    }

    fn matches(self, branch: &Branch) -> bool {
        match self {
            Self::Gone => branch.upstream_track.contains("[gone]"),
            Self::Unpushed => branch.upstream.is_none(),
            Self::Pr | Self::NoPr | Self::Closed | Self::Merged => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Undecided,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BranchSection {
    Protected,
    Saved,
    Regular,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protection {
    Current,
    Worktree,
    Main,
    Master,
    DefaultBranch,
}

impl Protection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "current branch",
            Self::Worktree => "worktree",
            Self::Main => "main",
            Self::Master => "master",
            Self::DefaultBranch => "default branch",
        }
    }

    pub fn ineligible_message(self) -> &'static str {
        match self {
            Self::Current => "Current branch is ineligible for cleanup.",
            Self::Worktree => {
                "This branch is checked out in another worktree and is ineligible for cleanup."
            }
            Self::Main => "The main branch is ineligible for cleanup.",
            Self::Master => "The master branch is ineligible for cleanup.",
            Self::DefaultBranch => "The default branch is ineligible for cleanup.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub upstream: Option<String>,
    pub upstream_track: String,
    pub committed_at: i64,
    pub relative_date: String,
    pub subject: String,
    pub pr_url: Option<String>,
    pub detail: Option<String>,
    pub saved: bool,
    pub protections: Vec<Protection>,
    pub decision: Decision,
}

impl Branch {
    pub fn is_protected(&self) -> bool {
        !self.protections.is_empty()
    }

    pub fn is_deletable(&self) -> bool {
        self.section() == BranchSection::Regular
    }

    pub fn section(&self) -> BranchSection {
        if self.is_protected() {
            BranchSection::Protected
        } else if self.saved {
            BranchSection::Saved
        } else {
            BranchSection::Regular
        }
    }

    pub fn display_name(&self) -> String {
        let mut labels = self
            .protections
            .iter()
            .map(|protection| format!("({})", protection.label()))
            .collect::<Vec<_>>();
        if self.saved {
            labels.push(String::from("(saved)"));
        }

        if labels.is_empty() {
            return self.name.clone();
        }

        format!("{} {}", self.name, labels.join(" "))
    }

    pub fn upstream_remote(&self) -> Option<&str> {
        self.upstream
            .as_deref()
            .and_then(|upstream| upstream.split_once('/').map(|(remote, _)| remote))
    }

    pub fn upstream_branch_name(&self) -> Option<&str> {
        self.upstream
            .as_deref()
            .and_then(|upstream| upstream.split_once('/').map(|(_, branch)| branch))
    }
}

#[derive(Debug, Clone)]
pub struct CleanupGroup {
    pub mode: CleanupMode,
    pub name: String,
    pub description: String,
    pub show_empty_message: bool,
    pub branches: Vec<Branch>,
}

impl CleanupGroup {
    pub fn from_mode(mode: CleanupMode, branches: Vec<Branch>) -> Self {
        Self {
            mode,
            name: mode.display_name().to_string(),
            description: mode.description().to_string(),
            show_empty_message: true,
            branches,
        }
    }

    pub fn named(
        mode: CleanupMode,
        name: impl Into<String>,
        description: impl Into<String>,
        branches: Vec<Branch>,
    ) -> Self {
        Self {
            mode,
            name: name.into(),
            description: description.into(),
            show_empty_message: false,
            branches,
        }
    }
}

#[derive(Debug, Clone)]
pub struct App {
    pub mode: CleanupMode,
    pub remote: String,
    pub group_name: String,
    pub group_description: String,
    pub step_index: usize,
    pub step_count: usize,
    pub branches: Vec<Branch>,
    pub selected: usize,
    pub screen: AppScreen,
    pub modal: Option<Modal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modal {
    pub title: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum AppScreen {
    Triage,
    Review(ReviewState),
    Executing(ExecutionState),
}

#[derive(Debug, Clone)]
pub struct ReviewState {
    pub items: Vec<CommandPlanItem>,
    pub require_explicit_choice: bool,
}

#[derive(Debug, Clone)]
pub struct ExecutionState {
    pub items: Vec<CommandPlanItem>,
    pub failure: Option<ExecutionFailure>,
}

#[derive(Debug, Clone)]
pub struct CommandPlanItem {
    pub branch: Branch,
    pub remote_command: Option<String>,
    pub local_command: String,
    pub state: CommandLineState,
}

#[derive(Debug, Clone)]
pub struct ExecutionFailure {
    pub branch: String,
    pub command: String,
    pub output: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandLineState {
    Pending,
    Success,
    Failed,
    Skipped,
}

impl App {
    pub fn from_group(
        group: CleanupGroup,
        remote: impl Into<String>,
        step_index: usize,
        step_count: usize,
    ) -> Self {
        let selected = initial_selection(&group.branches);
        Self {
            mode: group.mode,
            remote: remote.into(),
            group_name: group.name,
            group_description: group.description,
            step_index,
            step_count,
            branches: group.branches,
            selected,
            screen: AppScreen::Triage,
            modal: None,
        }
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

    pub fn toggle_delete(&mut self) {
        let Some(branch) = self.branches.get_mut(self.selected) else {
            return;
        };

        if let Some(protection) = branch.protections.first().copied() {
            self.modal = Some(Modal {
                title: "Branch Ineligible",
                message: format!(
                    "{} Press Enter to return to branch triage.",
                    protection.ineligible_message()
                ),
            });
            return;
        }
        if branch.saved {
            self.modal = Some(Modal {
                title: "Branch Saved",
                message: String::from(
                    "Saved branches must be unsaved before deletion. Press s to remove the saved label, then press Enter to return to branch triage.",
                ),
            });
            return;
        }

        branch.decision = match branch.decision {
            Decision::Undecided => Decision::Delete,
            Decision::Delete => Decision::Undecided,
        };
    }

    pub fn toggle_save(&mut self) {
        let old_selected = self.selected;
        let Some(selected_name) = self
            .branches
            .get(self.selected)
            .map(|branch| branch.name.clone())
        else {
            return;
        };
        let Some(branch) = self.branches.get_mut(self.selected) else {
            return;
        };
        let was_saved = branch.saved;

        branch.saved = !branch.saved;
        branch.decision = Decision::Undecided;

        reorder_branches(&mut self.branches);
        if was_saved {
            if let Some(index) = self
                .branches
                .iter()
                .position(|branch| branch.name == selected_name)
            {
                self.selected = index;
            } else {
                self.selected = initial_selection(&self.branches);
            }
            return;
        }

        if let Some(index) = first_regular_from(&self.branches, old_selected)
            .or_else(|| first_regular_from(&self.branches, 0))
        {
            self.selected = index;
        } else {
            self.selected = self
                .branches
                .iter()
                .position(|branch| branch.name == selected_name)
                .unwrap_or_else(|| initial_selection(&self.branches));
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

    pub fn saved_branch_names(&self) -> Vec<String> {
        self.branches
            .iter()
            .filter(|branch| branch.saved)
            .map(|branch| branch.name.clone())
            .collect()
    }

    pub fn dismiss_modal(&mut self) {
        self.modal = None;
    }

    pub fn in_triage(&self) -> bool {
        matches!(self.screen, AppScreen::Triage)
    }

    pub fn review_items(&self) -> Option<&[CommandPlanItem]> {
        match &self.screen {
            AppScreen::Review(review) => Some(&review.items),
            _ => None,
        }
    }

    pub fn review_requires_explicit_choice(&self) -> bool {
        match &self.screen {
            AppScreen::Review(review) => review.require_explicit_choice,
            _ => false,
        }
    }

    pub fn execution_items(&self) -> Option<&[CommandPlanItem]> {
        match &self.screen {
            AppScreen::Executing(execution) => Some(&execution.items),
            _ => None,
        }
    }

    pub fn execution_failure(&self) -> Option<&ExecutionFailure> {
        match &self.screen {
            AppScreen::Executing(execution) => execution.failure.as_ref(),
            _ => None,
        }
    }

    pub fn enter_review(&mut self) -> bool {
        let items = self
            .delete_candidates()
            .into_iter()
            .map(|branch| CommandPlanItem::new(self.mode, &self.remote, branch))
            .collect::<Vec<_>>();

        if items.is_empty() {
            return false;
        }

        self.screen = AppScreen::Review(ReviewState {
            items,
            require_explicit_choice: false,
        });
        true
    }

    pub fn exit_review(&mut self) {
        self.screen = AppScreen::Triage;
    }

    pub fn require_review_confirmation(&mut self) {
        if let AppScreen::Review(review) = &mut self.screen {
            review.require_explicit_choice = true;
        }
    }

    pub fn begin_execution(&mut self) {
        let items = match &self.screen {
            AppScreen::Review(review) => review.items.clone(),
            _ => return,
        };
        self.screen = AppScreen::Executing(ExecutionState {
            items,
            failure: None,
        });
    }

    pub fn next_pending_execution_index(&self) -> Option<usize> {
        match &self.screen {
            AppScreen::Executing(execution) if execution.failure.is_none() => execution
                .items
                .iter()
                .position(|item| item.state == CommandLineState::Pending),
            _ => None,
        }
    }

    pub fn execution_branch(&self, index: usize) -> Option<&Branch> {
        match &self.screen {
            AppScreen::Executing(execution) => execution.items.get(index).map(|item| &item.branch),
            _ => None,
        }
    }

    pub fn mark_execution_result(&mut self, index: usize, success: bool) {
        if let AppScreen::Executing(execution) = &mut self.screen
            && let Some(item) = execution.items.get_mut(index)
        {
            item.state = if success {
                CommandLineState::Success
            } else {
                CommandLineState::Failed
            };
        }
    }

    pub fn mark_execution_skipped_from(&mut self, start: usize) {
        if let AppScreen::Executing(execution) = &mut self.screen {
            for item in execution.items.iter_mut().skip(start) {
                if item.state == CommandLineState::Pending {
                    item.state = CommandLineState::Skipped;
                }
            }
        }
    }

    pub fn set_execution_failure(&mut self, index: usize, output: impl Into<String>) {
        if let AppScreen::Executing(execution) = &mut self.screen
            && let Some(item) = execution.items.get(index)
        {
            execution.failure = Some(ExecutionFailure {
                branch: item.branch.name.clone(),
                command: item.plain_command(),
                output: output.into(),
            });
        }
    }
}

impl CommandPlanItem {
    pub fn new(mode: CleanupMode, remote: &str, branch: &Branch) -> Self {
        let local_command = format!("git branch -D {}", shell_quote(&branch.name));
        let remote_command = if mode.uses_pr_metadata() && mode.is_cleanable() {
            branch.upstream_branch_name().map(|remote_branch| {
                format!(
                    "git push {} :refs/heads/{}",
                    shell_quote(remote),
                    shell_quote(remote_branch)
                )
            })
        } else {
            None
        };

        Self {
            branch: branch.clone(),
            remote_command,
            local_command,
            state: CommandLineState::Pending,
        }
    }

    pub fn plain_command(&self) -> String {
        match &self.remote_command {
            Some(remote_command) => format!("{remote_command} && {}", self.local_command),
            None => self.local_command.clone(),
        }
    }
}

pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return String::from("''");
    }

    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '_' | '.' | ':'))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[derive(Debug, Clone)]
pub struct DeleteResult {
    pub branch: String,
    pub success: bool,
    pub message: String,
    pub output: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PullRequestRef {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RepoView {
    #[serde(rename = "defaultBranchRef")]
    default_branch_ref: PullRequestRef,
}

#[derive(Debug, Clone, Deserialize)]
struct PullRequestRecord {
    state: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    url: String,
}

#[derive(Debug, Clone)]
struct ClosedModeData {
    default_branch: String,
    pull_requests: HashMap<String, PullRequestRecord>,
}

#[derive(Debug, Clone)]
struct ClosedModeResolution {
    data: Option<ClosedModeData>,
    notes: Vec<String>,
}

const GH_HEAD_SEARCH_CHUNK_SIZE: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanProgress {
    ValidatingRepository,
    ReadingCurrentBranch,
    ReadingWorktrees,
    SyncingRemoteRefs,
    LoadingLocalBranches,
    LoadingGithubData,
    MatchingClosedBranches,
}

impl ScanProgress {
    pub const TOTAL_STEPS: usize = 7;

    pub fn step(self) -> usize {
        match self {
            Self::ValidatingRepository => 1,
            Self::ReadingCurrentBranch => 2,
            Self::ReadingWorktrees => 3,
            Self::SyncingRemoteRefs => 4,
            Self::LoadingLocalBranches => 5,
            Self::LoadingGithubData => 6,
            Self::MatchingClosedBranches => 7,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::ValidatingRepository => "validating repository",
            Self::ReadingCurrentBranch => "reading current branch",
            Self::ReadingWorktrees => "reading linked worktrees",
            Self::SyncingRemoteRefs => "syncing remote refs",
            Self::LoadingLocalBranches => "loading local branches",
            Self::LoadingGithubData => "loading GitHub data",
            Self::MatchingClosedBranches => "matching branches against GitHub state",
        }
    }
}

pub fn scan_selected_modes(
    repo: &Path,
    modes: &[CleanupMode],
    remote: &str,
) -> Result<Vec<CleanupGroup>> {
    Ok(scan_with_options(repo, ScanOptions::preview(modes, remote), |_, _| {})?.groups)
}

pub fn scan_selected_modes_with_progress<F>(
    repo: &Path,
    modes: &[CleanupMode],
    remote: &str,
    mut progress: F,
) -> Result<Vec<CleanupGroup>>
where
    F: FnMut(ScanProgress, Option<&str>),
{
    Ok(scan_with_options(
        repo,
        ScanOptions::preview(modes, remote),
        |stage, detail| {
            progress(stage, detail);
        },
    )?
    .groups)
}

pub fn scan_with_options<F>(
    repo: &Path,
    options: ScanOptions<'_>,
    mut progress: F,
) -> Result<ScanOutcome>
where
    F: FnMut(ScanProgress, Option<&str>),
{
    progress(ScanProgress::ValidatingRepository, None);
    ensure_work_tree(repo)?;
    let keep_store = KeepStore::load(repo)?;

    progress(ScanProgress::ReadingCurrentBranch, None);
    let current_branch = current_branch(repo)?;

    progress(ScanProgress::ReadingWorktrees, None);
    let worktree_branches = other_worktree_branches(repo, current_branch.as_deref())?;

    if options.intent == ScanIntent::Clean && modes_need_pr_metadata(options.modes) {
        progress(ScanProgress::SyncingRemoteRefs, Some(options.remote));
        fetch_prune_remote(repo, options.remote)?;
    }

    progress(ScanProgress::LoadingLocalBranches, None);
    let mut all_branches =
        load_branch_inventory(repo, current_branch.as_deref(), None, &worktree_branches)?;

    let closed_candidate_heads = closed_candidate_heads(&all_branches, options.remote);

    let closed_resolution = if modes_need_pr_metadata(options.modes) {
        let resolution = resolve_closed_mode_data(
            repo,
            options.remote,
            &closed_candidate_heads,
            options.intent,
            &mut progress,
        )?;
        if let Some(closed_mode_data) = &resolution.data {
            apply_default_branch_protection(&mut all_branches, &closed_mode_data.default_branch);
        }
        Some(resolution)
    } else {
        None
    };

    let mut groups = Vec::new();
    let mut pr_groups_added = false;
    for mode in options.modes.iter().copied() {
        match mode {
            mode if mode.uses_pr_metadata() => {
                if pr_groups_added {
                    continue;
                }
                pr_groups_added = true;

                if let Some(closed_mode_data) = closed_resolution
                    .as_ref()
                    .and_then(|resolution| resolution.data.as_ref())
                {
                    groups.extend(
                        build_pr_groups(
                            options.modes,
                            &all_branches,
                            closed_mode_data,
                            options.remote,
                            &mut progress,
                        )
                        .into_iter()
                        .map(|mut group| {
                            group.branches =
                                apply_keep_labels(group.mode, group.branches, &keep_store);
                            group
                        }),
                    );
                }
            }
            _ => {
                let branches = all_branches
                    .iter()
                    .filter(|branch| mode.matches(branch))
                    .cloned()
                    .collect::<Vec<_>>();
                groups.push(CleanupGroup::from_mode(
                    mode,
                    apply_keep_labels(mode, branches, &keep_store),
                ));
            }
        }
    }

    let notes = closed_resolution
        .map(|resolution| resolution.notes)
        .unwrap_or_default();

    Ok(ScanOutcome { groups, notes })
}

pub fn delete_branches(
    repo: &Path,
    mode: CleanupMode,
    remote: &str,
    branches: &[&Branch],
) -> Vec<DeleteResult> {
    let mut results = Vec::new();
    for branch in branches {
        let result = delete_branch(repo, mode, remote, branch);
        let should_stop = !result.success;
        results.push(result);
        if should_stop {
            break;
        }
    }

    results
}

pub fn delete_branch(
    repo: &Path,
    mode: CleanupMode,
    remote: &str,
    branch: &Branch,
) -> DeleteResult {
    if mode.uses_pr_metadata() && mode.is_cleanable() {
        delete_closed_branch(repo, remote, branch)
    } else {
        delete_local_branch(repo, branch)
    }
}

fn load_branch_inventory(
    repo: &Path,
    current_branch: Option<&str>,
    default_branch: Option<&str>,
    worktree_branches: &HashSet<String>,
) -> Result<Vec<Branch>> {
    let lines = git_output(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)\u{1f}%(upstream:short)\u{1f}%(upstream:track)\u{1f}%(committerdate:unix)\u{1f}%(subject)",
            "refs/heads/",
        ],
    )?;

    let mut branches = Vec::new();
    for line in lines.lines().filter(|line| !line.trim().is_empty()) {
        let Some(branch) =
            parse_branch_line(line, current_branch, default_branch, worktree_branches)
        else {
            continue;
        };

        branches.push(branch);
    }

    branches.sort_by(|left, right| {
        right
            .committed_at
            .cmp(&left.committed_at)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(branches)
}

fn apply_default_branch_protection(branches: &mut [Branch], default_branch: &str) {
    if matches!(default_branch, "main" | "master") {
        return;
    }

    for branch in branches {
        if branch.name == default_branch && !branch.protections.contains(&Protection::DefaultBranch)
        {
            branch.protections.push(Protection::DefaultBranch);
        }
    }
}

fn closed_candidate_heads(branches: &[Branch], remote: &str) -> Vec<String> {
    branches
        .iter()
        .filter(|branch| {
            branch.upstream_remote() == Some(remote)
                && !branch.upstream_track.contains("[gone]")
                && !branch
                    .protections
                    .iter()
                    .any(|protection| matches!(protection, Protection::Main | Protection::Master))
        })
        .filter_map(|branch| branch.upstream_branch_name().map(ToOwned::to_owned))
        .collect()
}

fn modes_need_pr_metadata(modes: &[CleanupMode]) -> bool {
    modes.iter().copied().any(CleanupMode::uses_pr_metadata)
}

fn chunk_summary(heads: &[String]) -> Option<String> {
    if heads.is_empty() {
        return None;
    }

    let preview = heads
        .iter()
        .take(3)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = heads.len().saturating_sub(3);

    if remaining == 0 {
        Some(preview)
    } else {
        Some(format!("{preview} (+{remaining} more)"))
    }
}

fn build_pr_groups<F>(
    selected_modes: &[CleanupMode],
    all_branches: &[Branch],
    closed_mode_data: &ClosedModeData,
    remote: &str,
    progress: &mut F,
) -> Vec<CleanupGroup>
where
    F: FnMut(ScanProgress, Option<&str>),
{
    let mut pr = Vec::new();
    let mut closed = Vec::new();
    let mut no_pr = Vec::new();
    let mut merged = Vec::new();

    for branch in all_branches {
        progress(
            ScanProgress::MatchingClosedBranches,
            Some(branch.name.as_str()),
        );

        if branch.protections.iter().any(|protection| {
            matches!(
                protection,
                Protection::Main | Protection::Master | Protection::DefaultBranch
            )
        }) {
            continue;
        }

        if branch.upstream_remote() != Some(remote) {
            continue;
        }
        if branch.upstream_track.contains("[gone]") {
            continue;
        }

        let Some(head_ref_name) = branch.upstream_branch_name() else {
            continue;
        };

        let mut branch = branch.clone();
        match closed_mode_data.pull_requests.get(head_ref_name) {
            Some(record) if record.state == "OPEN" => {
                branch.pr_url = Some(record.url.clone());
                pr.push(branch);
            }
            Some(record) if record.state == "MERGED" => {
                branch.pr_url = Some(record.url.clone());
                merged.push(branch);
            }
            Some(record) if record.state == "CLOSED" => {
                branch.pr_url = Some(record.url.clone());
                closed.push(branch);
            }
            Some(record) => {
                branch.pr_url = Some(record.url.clone());
                closed.push(branch);
            }
            None => no_pr.push(branch),
        }
    }

    let mut groups = Vec::new();
    for mode in selected_modes
        .iter()
        .copied()
        .filter(|mode| mode.uses_pr_metadata())
    {
        let branches = match mode {
            CleanupMode::Pr => pr.clone(),
            CleanupMode::NoPr => no_pr.clone(),
            CleanupMode::Closed => closed.clone(),
            CleanupMode::Merged => merged.clone(),
            CleanupMode::Gone | CleanupMode::Unpushed => continue,
        };
        groups.push(CleanupGroup::from_mode(mode, branches));
    }

    groups
}

fn apply_keep_labels(
    mode: CleanupMode,
    mut branches: Vec<Branch>,
    keep_store: &KeepStore,
) -> Vec<Branch> {
    for branch in &mut branches {
        branch.saved = keep_store.is_saved(mode, &branch.name);
        branch.decision = Decision::Undecided;
    }

    reorder_branches(&mut branches);
    branches
}

fn reorder_branches(branches: &mut [Branch]) {
    branches.sort_by_key(|branch| branch.section());
}

fn initial_selection(branches: &[Branch]) -> usize {
    first_regular_from(branches, 0)
        .or_else(|| {
            branches
                .iter()
                .position(|branch| branch.section() == BranchSection::Saved)
        })
        .or_else(|| {
            branches
                .iter()
                .position(|branch| branch.section() == BranchSection::Protected)
        })
        .unwrap_or(0)
}

fn first_regular_from(branches: &[Branch], start: usize) -> Option<usize> {
    branches
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, branch)| branch.section() == BranchSection::Regular)
        .map(|(index, _)| index)
}

fn resolve_closed_mode_data<F>(
    repo: &Path,
    remote: &str,
    candidate_heads: &[String],
    intent: ScanIntent,
    progress: &mut F,
) -> Result<ClosedModeResolution>
where
    F: FnMut(ScanProgress, Option<&str>),
{
    match intent {
        ScanIntent::Clean => {
            let closed_mode_data =
                refresh_closed_mode_data(repo, remote, candidate_heads, progress)?;
            let mut notes = Vec::new();
            if let Err(error) = persist_closed_mode_cache(repo, remote, &closed_mode_data) {
                notes.push(format!("failed to update GitHub metadata cache: {error:#}"));
            }
            Ok(ClosedModeResolution {
                data: Some(closed_mode_data),
                notes,
            })
        }
        ScanIntent::Preview => {
            let mut notes = Vec::new();
            match load_fresh_closed_mode_cache(repo, remote)? {
                CacheLoad::Fresh(closed_mode_data) => {
                    return Ok(ClosedModeResolution {
                        data: Some(closed_mode_data),
                        notes,
                    });
                }
                CacheLoad::Unavailable(note) => notes.push(note),
                CacheLoad::Missing => {}
            }

            match refresh_closed_mode_data(repo, remote, candidate_heads, progress) {
                Ok(closed_mode_data) => {
                    if let Err(error) = persist_closed_mode_cache(repo, remote, &closed_mode_data) {
                        notes.push(format!("failed to update GitHub metadata cache: {error:#}"));
                    }
                    Ok(ClosedModeResolution {
                        data: Some(closed_mode_data),
                        notes,
                    })
                }
                Err(error) => {
                    notes.push(format!("GitHub metadata unavailable: {error:#}"));
                    Ok(ClosedModeResolution { data: None, notes })
                }
            }
        }
    }
}

enum CacheLoad {
    Fresh(ClosedModeData),
    Missing,
    Unavailable(String),
}

fn load_fresh_closed_mode_cache(repo: &Path, remote: &str) -> Result<CacheLoad> {
    let remote_url = remote_url(repo, remote)?;
    let cache = match PrCache::load(repo) {
        Ok(cache) => cache,
        Err(error) => {
            return Ok(CacheLoad::Unavailable(format!(
                "ignoring unreadable GitHub metadata cache: {error:#}"
            )));
        }
    };
    let Some(entry) = cache.remote_entry(remote, &remote_url) else {
        return Ok(CacheLoad::Missing);
    };

    if !pr_cache_is_fresh(entry.refreshed_at) {
        return Ok(CacheLoad::Missing);
    }

    Ok(CacheLoad::Fresh(closed_mode_data_from_cache(entry)))
}

fn persist_closed_mode_cache(repo: &Path, remote: &str, data: &ClosedModeData) -> Result<()> {
    let remote_url = remote_url(repo, remote)?;
    let mut cache = match PrCache::load(repo) {
        Ok(cache) => cache,
        Err(_) => PrCache::new(repo)?,
    };
    let entry = PrCacheRemoteEntry {
        remote_url,
        refreshed_at: current_unix_timestamp(),
        default_branch: data.default_branch.clone(),
        pull_requests_by_head: data
            .pull_requests
            .iter()
            .map(|(head_ref_name, record)| {
                (
                    head_ref_name.clone(),
                    CachedPullRequestRecord {
                        state: record.state.clone(),
                        url: record.url.clone(),
                    },
                )
            })
            .collect(),
    };
    cache.replace_remote(remote, entry)
}

fn closed_mode_data_from_cache(entry: &PrCacheRemoteEntry) -> ClosedModeData {
    ClosedModeData {
        default_branch: entry.default_branch.clone(),
        pull_requests: entry
            .pull_requests_by_head
            .iter()
            .map(|(head_ref_name, record)| {
                (
                    head_ref_name.clone(),
                    PullRequestRecord {
                        state: record.state.clone(),
                        head_ref_name: head_ref_name.clone(),
                        url: record.url.clone(),
                    },
                )
            })
            .collect(),
    }
}

fn pr_cache_is_fresh(refreshed_at: i64) -> bool {
    current_unix_timestamp().saturating_sub(refreshed_at) <= PR_CACHE_TTL_SECONDS
}

fn refresh_closed_mode_data<F>(
    repo: &Path,
    remote: &str,
    candidate_heads: &[String],
    progress: &mut F,
) -> Result<ClosedModeData>
where
    F: FnMut(ScanProgress, Option<&str>),
{
    ensure_remote_exists(repo, remote)?;
    ensure_gh_installed()?;
    ensure_gh_authenticated(repo)?;
    let repo_view: RepoView = serde_json::from_str(&gh_output(
        repo,
        &["repo", "view", "--json", "defaultBranchRef"],
    )?)?;

    let mut pull_requests = HashMap::new();
    for chunk in candidate_heads.chunks(GH_HEAD_SEARCH_CHUNK_SIZE) {
        let summary = chunk_summary(chunk);
        progress(ScanProgress::LoadingGithubData, summary.as_deref());

        let search = chunk
            .iter()
            .map(|head| format!("head:{head}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let limit = (chunk.len() * 5).max(20).to_string();
        let args = vec![
            String::from("pr"),
            String::from("list"),
            String::from("--state"),
            String::from("all"),
            String::from("--search"),
            search,
            String::from("--json"),
            String::from("state,headRefName,url"),
            String::from("--limit"),
            limit,
        ];
        let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let records = serde_json::from_str::<Vec<PullRequestRecord>>(&gh_output(repo, &arg_refs)?)?;

        for pull_request in records {
            let head_ref_name = pull_request.head_ref_name.clone();
            match pull_requests.get(&head_ref_name) {
                Some(existing)
                    if pull_request_rank(existing) >= pull_request_rank(&pull_request) => {}
                _ => {
                    pull_requests.insert(head_ref_name, pull_request);
                }
            }
        }
    }

    Ok(ClosedModeData {
        default_branch: repo_view.default_branch_ref.name,
        pull_requests,
    })
}

fn pull_request_rank(pull_request: &PullRequestRecord) -> usize {
    match pull_request.state.as_str() {
        "OPEN" => 3,
        "MERGED" => 2,
        "CLOSED" => 1,
        _ => 0,
    }
}

fn delete_closed_branch(repo: &Path, remote: &str, branch: &Branch) -> DeleteResult {
    let Some(remote_branch) = branch.upstream_branch_name() else {
        return DeleteResult {
            branch: branch.name.clone(),
            success: false,
            message: String::from("branch has no remote tracking branch"),
            output: String::from("branch has no remote tracking branch"),
        };
    };

    let remote_ref = format!(":refs/heads/{remote_branch}");
    match Command::new("git")
        .args(["push", remote, &remote_ref])
        .current_dir(repo)
        .output()
    {
        Ok(output) if output.status.success() => delete_local_branch(repo, branch),
        Ok(output) => {
            let output_text = command_message(&output);
            DeleteResult {
                branch: branch.name.clone(),
                success: false,
                message: format!("git push {remote} {remote_ref} failed"),
                output: output_text,
            }
        }
        Err(error) => DeleteResult {
            branch: branch.name.clone(),
            success: false,
            message: error.to_string(),
            output: error.to_string(),
        },
    }
}

fn delete_local_branch(repo: &Path, branch: &Branch) -> DeleteResult {
    match Command::new("git")
        .args(["branch", "-D", &branch.name])
        .current_dir(repo)
        .output()
    {
        Ok(output) if output.status.success() => DeleteResult {
            branch: branch.name.clone(),
            success: true,
            message: command_message(&output),
            output: command_message(&output),
        },
        Ok(output) => {
            let output_text = command_message(&output);
            DeleteResult {
                branch: branch.name.clone(),
                success: false,
                message: format!("git branch -D {} failed", branch.name),
                output: output_text,
            }
        }
        Err(error) => DeleteResult {
            branch: branch.name.clone(),
            success: false,
            message: error.to_string(),
            output: error.to_string(),
        },
    }
}

fn command_message(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::from("command failed with no output"),
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("{stderr}\n{stdout}"),
    }
}

fn parse_branch_line(
    line: &str,
    current_branch: Option<&str>,
    default_branch: Option<&str>,
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
    let committed_at = fields.next()?.trim().parse::<i64>().ok()?;
    let relative_date = format_relative_age(committed_at);
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
    if default_branch == Some(name.as_str()) && name != "main" && name != "master" {
        protections.push(Protection::DefaultBranch);
    }

    Some(Branch {
        name,
        upstream,
        upstream_track,
        committed_at,
        relative_date,
        subject,
        pr_url: None,
        detail: None,
        saved: false,
        protections,
        decision: Decision::Undecided,
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

fn format_relative_age(committed_at: i64) -> String {
    format_age_from_seconds(current_unix_timestamp().saturating_sub(committed_at).max(0) as u64)
}

fn format_age_from_seconds(seconds: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    const WEEK: u64 = 7 * DAY;
    const MONTH: u64 = 30 * DAY;

    if seconds < MINUTE {
        return unit_label(seconds.max(1), "second");
    }
    if seconds < HOUR {
        return unit_label(seconds / MINUTE, "minute");
    }
    if seconds < DAY {
        return unit_label(seconds / HOUR, "hour");
    }
    if seconds < WEEK {
        return unit_label(seconds / DAY, "day");
    }
    if seconds < MONTH {
        return unit_label(seconds / WEEK, "week");
    }

    unit_label(seconds / MONTH, "month")
}

fn unit_label(value: u64, unit: &str) -> String {
    if value == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{value} {unit}s ago")
    }
}

fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_secs() as i64
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

fn ensure_remote_exists(repo: &Path, remote: &str) -> Result<()> {
    remote_url(repo, remote).map(|_| ())
}

fn remote_url(repo: &Path, remote: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", remote])
        .current_dir(repo)
        .output()
        .context("failed to inspect git remotes")?;

    if output.status.success() {
        return String::from_utf8(output.stdout)
            .context("git remote get-url returned non-utf8 output")
            .map(|url| url.trim().to_string());
    }

    bail!("remote `{remote}` does not exist")
}

fn fetch_prune_remote(repo: &Path, remote: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["fetch", remote, "--prune"])
        .current_dir(repo)
        .output()
        .context("failed to sync remote refs")?;

    if output.status.success() {
        return Ok(());
    }

    bail!(
        "git fetch {} --prune failed: {}",
        remote,
        command_message(&output)
    )
}

fn ensure_gh_installed() -> Result<()> {
    match Command::new("gh").args(["--version"]).output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => {
            bail!("gh CLI required for GitHub-backed groups. Install from https://cli.github.com")
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("gh CLI required for GitHub-backed groups. Install from https://cli.github.com")
        }
        Err(error) => Err(error).context("failed to run gh --version"),
    }
}

fn ensure_gh_authenticated(repo: &Path) -> Result<()> {
    let output = Command::new("gh")
        .args(["auth", "status"])
        .current_dir(repo)
        .output()
        .context("failed to run gh auth status")?;

    if output.status.success() {
        return Ok(());
    }

    bail!("Run `gh auth login` first")
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String> {
    let output = git_output_raw(repo, args)?;
    String::from_utf8(output.stdout).context("git returned non-utf8 output")
}

fn gh_output(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("gh")
        .args(args)
        .current_dir(repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run gh {}", args.join(" ")))?;

    if !output.status.success() {
        bail!(
            "gh {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    String::from_utf8(output.stdout).context("gh returned non-utf8 output")
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

    use super::{
        App, AppScreen, Branch, BranchSection, CleanupGroup, CleanupMode, Decision,
        FIELD_SEPARATOR, Protection, chunk_summary, closed_candidate_heads,
        format_age_from_seconds, parse_branch_line,
    };

    const SAMPLE_TIMESTAMP: &str = "1700000000";

    #[test]
    fn parse_branch_line_marks_current_branch_as_protected() {
        let branch = parse_branch_line(
            &format!(
                "feature/foo{FIELD_SEPARATOR}origin/feature/foo{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            Some("feature/foo"),
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");

        assert_eq!(branch.decision, Decision::Undecided);
        assert_eq!(branch.protections, vec![Protection::Current]);
        assert!(branch.display_name().contains("(current branch)"));
    }

    #[test]
    fn parse_branch_line_marks_other_worktree_branch_as_protected() {
        let worktree_branches = HashSet::from([String::from("feature/foo")]);
        let branch = parse_branch_line(
            &format!(
                "feature/foo{FIELD_SEPARATOR}origin/feature/foo{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            Some("main"),
            None,
            &worktree_branches,
        )
        .expect("branch parsed");

        assert_eq!(branch.decision, Decision::Undecided);
        assert_eq!(branch.protections, vec![Protection::Worktree]);
    }

    #[test]
    fn parse_branch_line_marks_main_branch_as_protected() {
        let branch = parse_branch_line(
            &format!(
                "main{FIELD_SEPARATOR}origin/main{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");

        assert_eq!(branch.protections, vec![Protection::Main]);
        assert_eq!(branch.decision, Decision::Undecided);
    }

    #[test]
    fn cleanup_mode_matches_gone_branches() {
        let branch = parse_branch_line(
            &format!(
                "feature/foo{FIELD_SEPARATOR}origin/feature/foo{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");

        assert!(CleanupMode::Gone.matches(&branch));
        assert!(!CleanupMode::Unpushed.matches(&branch));
    }

    #[test]
    fn cleanup_mode_matches_unpushed_branches() {
        let branch = parse_branch_line(
            &format!(
                "feature/foo{FIELD_SEPARATOR}{FIELD_SEPARATOR}{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");

        assert!(CleanupMode::Unpushed.matches(&branch));
        assert!(!CleanupMode::Gone.matches(&branch));
    }

    #[test]
    fn toggle_delete_marks_branch_for_deletion() {
        let branch = parse_branch_line(
            &format!(
                "feature/foo{FIELD_SEPARATOR}origin/feature/foo{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");
        let mut app = App::from_group(
            CleanupGroup::from_mode(CleanupMode::Gone, vec![branch]),
            "origin",
            1,
            1,
        );

        app.toggle_delete();
        assert_eq!(app.branches[0].decision, Decision::Delete);

        app.toggle_delete();
        assert_eq!(app.branches[0].decision, Decision::Undecided);
    }

    #[test]
    fn toggle_delete_shows_modal_for_protected_branch() {
        let branch = parse_branch_line(
            &format!(
                "feature/foo{FIELD_SEPARATOR}origin/feature/foo{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            Some("feature/foo"),
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");
        let mut app = App::from_group(
            CleanupGroup::from_mode(CleanupMode::Gone, vec![branch]),
            "origin",
            1,
            1,
        );

        app.toggle_delete();

        let modal = app.modal.expect("modal shown");
        assert_eq!(modal.title, "Branch Ineligible");
        assert!(
            modal
                .message
                .contains("Current branch is ineligible for cleanup.")
        );
        assert_eq!(app.branches[0].decision, Decision::Undecided);
    }

    #[test]
    fn toggle_delete_shows_modal_for_saved_branch() {
        let mut branch = parse_branch_line(
            &format!(
                "feature/foo{FIELD_SEPARATOR}origin/feature/foo{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");
        branch.saved = true;
        let mut app = App::from_group(
            CleanupGroup::from_mode(CleanupMode::Gone, vec![branch]),
            "origin",
            1,
            1,
        );

        app.toggle_delete();

        let modal = app.modal.expect("modal shown");
        assert_eq!(modal.title, "Branch Saved");
        assert!(modal.message.contains("must be unsaved before deletion"));
        assert_eq!(app.branches[0].decision, Decision::Undecided);
    }

    #[test]
    fn from_group_starts_selection_at_first_regular_branch() {
        let mut protected = parse_branch_line(
            &format!(
                "feature/protected{FIELD_SEPARATOR}origin/feature/protected{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            Some("feature/protected"),
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");
        protected.saved = true;

        let mut saved = parse_branch_line(
            &format!(
                "feature/saved{FIELD_SEPARATOR}origin/feature/saved{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");
        saved.saved = true;

        let regular = parse_branch_line(
            &format!(
                "feature/regular{FIELD_SEPARATOR}origin/feature/regular{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");

        let app = App::from_group(
            CleanupGroup::from_mode(CleanupMode::Gone, vec![protected, saved, regular]),
            "origin",
            1,
            1,
        );

        assert_eq!(app.selected, 2);
        assert_eq!(app.branches[app.selected].section(), BranchSection::Regular);
    }

    #[test]
    fn toggle_save_moves_branch_into_saved_section() {
        let first = parse_branch_line(
            &format!(
                "feature/first{FIELD_SEPARATOR}origin/feature/first{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");
        let second = parse_branch_line(
            &format!(
                "feature/second{FIELD_SEPARATOR}origin/feature/second{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");
        let mut app = App::from_group(
            CleanupGroup::from_mode(CleanupMode::Gone, vec![first, second]),
            "origin",
            1,
            1,
        );

        app.selected = 1;
        app.toggle_save();

        assert_eq!(app.branches[0].name, "feature/second");
        assert!(app.branches[0].saved);
        assert_eq!(app.selected, 1);
        assert_eq!(app.branches[app.selected].name, "feature/first");
        assert_eq!(
            app.saved_branch_names(),
            vec![String::from("feature/second")]
        );
    }

    #[test]
    fn enter_review_requires_explicit_y_or_n_after_enter() {
        let mut branch = parse_branch_line(
            &format!(
                "feature/foo{FIELD_SEPARATOR}origin/feature/foo{FIELD_SEPARATOR}[gone]{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");
        branch.decision = Decision::Delete;

        let mut app = App::from_group(
            CleanupGroup::from_mode(CleanupMode::Gone, vec![branch]),
            "origin",
            1,
            1,
        );

        assert!(app.enter_review());
        assert!(matches!(app.screen, AppScreen::Review(_)));
        assert!(!app.review_requires_explicit_choice());

        app.require_review_confirmation();
        assert!(app.review_requires_explicit_choice());
    }

    #[test]
    fn begin_execution_builds_command_plan_for_deleted_branch() {
        let mut branch = parse_branch_line(
            &format!(
                "feature/foo{FIELD_SEPARATOR}origin/feature/foo{FIELD_SEPARATOR}{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");
        branch.decision = Decision::Delete;

        let mut app = App::from_group(
            CleanupGroup::from_mode(CleanupMode::Closed, vec![branch]),
            "origin",
            1,
            1,
        );

        assert!(app.enter_review());
        app.begin_execution();

        let items = app.execution_items().expect("execution items available");
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].plain_command(),
            "git push origin :refs/heads/feature/foo && git branch -D feature/foo"
        );
    }

    #[test]
    fn set_execution_failure_records_output_and_stops_progression() {
        let mut branch = parse_branch_line(
            &format!(
                "feature/foo{FIELD_SEPARATOR}origin/feature/foo{FIELD_SEPARATOR}{FIELD_SEPARATOR}{SAMPLE_TIMESTAMP}{FIELD_SEPARATOR}test subject"
            ),
            None,
            None,
            &HashSet::new(),
        )
        .expect("branch parsed");
        branch.decision = Decision::Delete;

        let mut app = App::from_group(
            CleanupGroup::from_mode(CleanupMode::Closed, vec![branch]),
            "origin",
            1,
            1,
        );

        assert!(app.enter_review());
        app.begin_execution();
        app.set_execution_failure(0, "fatal: remote ref does not exist");

        let failure = app.execution_failure().expect("failure captured");
        assert_eq!(failure.branch, "feature/foo");
        assert_eq!(
            failure.command,
            "git push origin :refs/heads/feature/foo && git branch -D feature/foo"
        );
        assert_eq!(failure.output, "fatal: remote ref does not exist");
        assert_eq!(app.next_pending_execution_index(), None);
    }

    #[test]
    fn format_age_uses_months_for_long_durations() {
        assert_eq!(
            format_age_from_seconds(18 * 30 * 24 * 60 * 60),
            "18 months ago"
        );
    }

    #[test]
    fn closed_candidate_heads_filters_to_remote_tracked_branches() {
        let branches = vec![
            Branch {
                name: String::from("feature/one"),
                upstream: Some(String::from("origin/feature/one")),
                upstream_track: String::new(),
                committed_at: 1_700_000_001,
                relative_date: String::from("1 day ago"),
                subject: String::from("first"),
                pr_url: None,
                detail: None,
                saved: false,
                protections: Vec::new(),
                decision: Decision::Undecided,
            },
            Branch {
                name: String::from("feature/two"),
                upstream: Some(String::from("origin/feature/two")),
                upstream_track: String::new(),
                committed_at: 1_700_000_000,
                relative_date: String::from("2 days ago"),
                subject: String::from("second"),
                pr_url: None,
                detail: None,
                saved: false,
                protections: Vec::new(),
                decision: Decision::Undecided,
            },
            Branch {
                name: String::from("main"),
                upstream: Some(String::from("origin/main")),
                upstream_track: String::new(),
                committed_at: 1_699_999_999,
                relative_date: String::from("3 days ago"),
                subject: String::from("main"),
                pr_url: None,
                detail: None,
                saved: false,
                protections: vec![Protection::Main],
                decision: Decision::Undecided,
            },
            Branch {
                name: String::from("feature/gone"),
                upstream: Some(String::from("origin/feature/gone")),
                upstream_track: String::from("[gone]"),
                committed_at: 1_699_999_998,
                relative_date: String::from("4 days ago"),
                subject: String::from("gone"),
                pr_url: None,
                detail: None,
                saved: false,
                protections: Vec::new(),
                decision: Decision::Undecided,
            },
        ];

        assert_eq!(
            closed_candidate_heads(&branches, "origin"),
            vec![String::from("feature/one"), String::from("feature/two")]
        );
    }

    #[test]
    fn chunk_summary_shows_head_preview_and_remaining_count() {
        let heads = vec![
            String::from("feature/one"),
            String::from("feature/two"),
            String::from("feature/three"),
            String::from("feature/four"),
        ];

        assert_eq!(
            chunk_summary(&heads),
            Some(String::from(
                "feature/one, feature/two, feature/three (+1 more)"
            ))
        );
    }
}
