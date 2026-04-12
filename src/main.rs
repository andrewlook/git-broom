use std::env;
use std::io::{self, IsTerminal, Write};
use std::panic;
use std::path::Path;
use std::process;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
};
use git_broom::app::{
    App, Branch, CleanupGroup, CleanupMode, DeleteResult, IMPLEMENTED_MODES, ScanProgress,
    delete_branches, scan_selected_modes, scan_selected_modes_with_progress,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = parse_cli(env::args().skip(1))?;
    let repo = env::current_dir()?;
    let groups = if cli.modes.contains(&CleanupMode::Closed) {
        let mut status = ScanStatusLine::new();
        let groups =
            scan_selected_modes_with_progress(&repo, &cli.modes, &cli.remote, |stage, detail| {
                status.update(stage, detail);
            })?;
        status.finish();
        groups
    } else {
        scan_selected_modes(&repo, &cli.modes, &cli.remote)?
    };

    match cli.output {
        OutputMode::Interactive => run_interactive(&repo, &cli.remote, groups),
        OutputMode::Batch => run_batch(&groups),
        OutputMode::DryRun => run_dry_run(&groups),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Interactive,
    Batch,
    DryRun,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    modes: Vec<CleanupMode>,
    output: OutputMode,
    remote: String,
}

fn parse_cli(args: impl Iterator<Item = String>) -> Result<CliOptions> {
    let mut modes = Vec::new();
    let mut output = OutputMode::Interactive;
    let mut remote = String::from("origin");
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--batch" => {
                if output == OutputMode::DryRun {
                    bail!("--batch and --dry-run cannot be used together");
                }
                output = OutputMode::Batch;
            }
            "--dry-run" => {
                if output == OutputMode::Batch {
                    bail!("--batch and --dry-run cannot be used together");
                }
                output = OutputMode::DryRun;
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            "--remote" => {
                let Some(value) = args.next() else {
                    bail!("--remote requires a value\n\n{}", usage_text());
                };
                remote = value;
            }
            value => {
                let Some(mode) = CleanupMode::from_arg(value) else {
                    bail!("unknown cleanup mode `{value}`\n\n{}", usage_text());
                };

                if !modes.contains(&mode) {
                    modes.push(mode);
                }
            }
        }
    }

    if modes.is_empty() {
        modes = IMPLEMENTED_MODES.to_vec();
    }

    Ok(CliOptions {
        modes,
        output,
        remote,
    })
}

fn run_interactive(repo: &Path, remote: &str, groups: Vec<CleanupGroup>) -> Result<()> {
    if groups.iter().all(|group| group.branches.is_empty()) {
        println!("No branches found for selected cleanup modes.");
        return Ok(());
    }

    let group_count = groups.len();
    let mut total_deleted = 0;
    let mut total_failed = 0;

    for (index, group) in groups.into_iter().enumerate() {
        if group.branches.is_empty() {
            if group.show_empty_message {
                println!("{}", group.mode.no_matches_message());
            }
            continue;
        }

        let mut app = App::from_group(group, index + 1, group_count);
        match run_tui(&mut app)? {
            ExitAction::Quit => {
                println!("Aborted.");
                return Ok(());
            }
            ExitAction::Confirm => {
                let branches_to_delete = app.delete_candidates();

                if branches_to_delete.is_empty() {
                    println!("No {} branches selected for deletion.", app.group_name);
                    continue;
                }

                println!(
                    "About to delete {} {} branches ({}):",
                    branches_to_delete.len(),
                    app.group_name,
                    app.group_description
                );
                for branch in &branches_to_delete {
                    println!("  {}", branch.name);
                }

                if !prompt_for_confirmation()? {
                    println!("Skipped {}.", app.group_name);
                    continue;
                }

                let (deleted, failed) = print_delete_results(
                    &app.group_name,
                    &delete_branches(repo, app.mode, remote, &branches_to_delete),
                );
                total_deleted += deleted;
                total_failed += failed;
            }
        }
    }

    println!("Workflow complete. Deleted {total_deleted} branches. {total_failed} failed.");
    Ok(())
}

fn run_batch(groups: &[CleanupGroup]) -> Result<()> {
    for line in format_preview_lines(groups, preview_width()) {
        println!("{line}");
    }

    Ok(())
}

fn run_dry_run(groups: &[CleanupGroup]) -> Result<()> {
    for line in format_preview_lines(groups, preview_width()) {
        println!("{line}");
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitAction {
    Quit,
    Confirm,
}

fn run_tui(app: &mut App) -> Result<ExitAction> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|frame| git_broom::ui::render(frame, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if is_immediate_exit(key) {
                return Ok(ExitAction::Quit);
            }

            if app.modal.is_some() {
                if matches!(key.code, KeyCode::Enter) {
                    app.dismiss_modal();
                }
                continue;
            }

            match key.code {
                KeyCode::Char('j') | KeyCode::Down => app.next(),
                KeyCode::Char('k') | KeyCode::Up => app.previous(),
                KeyCode::Char('d') => app.toggle_delete(),
                KeyCode::Char('a') => app.mark_all_delete(),
                KeyCode::Char('u') => app.unmark_all(),
                KeyCode::Enter => return Ok(ExitAction::Confirm),
                KeyCode::Char('q') | KeyCode::Esc => return Ok(ExitAction::Quit),
                _ => {}
            }
        }
    }
}

fn is_immediate_exit(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'))
}

fn prompt_for_confirmation() -> Result<bool> {
    print!("Proceed? [y/N] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(matches!(input.trim(), "y" | "Y"))
}

fn print_delete_results(group_name: &str, results: &[DeleteResult]) -> (usize, usize) {
    let mut deleted = 0;
    let mut failed = 0;

    for result in results {
        if result.success {
            deleted += 1;
            println!("Deleted {}.", result.branch);
        } else {
            failed += 1;
            println!("Failed to delete {}: {}", result.branch, result.message);
        }
    }

    println!("{group_name}: deleted {deleted} branches. {failed} failed.");

    (deleted, failed)
}

fn format_preview_lines(groups: &[CleanupGroup], available_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let step_count = groups
        .iter()
        .filter(|group| !group.branches.is_empty())
        .count();
    let mut step_index = 0;

    for group in groups {
        if group.branches.is_empty() {
            continue;
        }

        if !lines.is_empty() {
            lines.push(String::new());
        }

        step_index += 1;
        let (branch_width, secondary_width, age_width) =
            preview_column_widths(group.mode, available_width);
        let total_width = preview_total_width(branch_width, secondary_width, age_width);
        lines.push(format_preview_title(
            &group.name,
            &group.description,
            step_index,
            step_count,
            total_width,
        ));
        lines.push(format_preview_header(
            group.mode,
            branch_width,
            secondary_width,
            age_width,
        ));
        lines.push(format_preview_rule(
            group.mode,
            branch_width,
            secondary_width,
            age_width,
        ));
        lines.extend(group.branches.iter().flat_map(|branch| {
            format_preview_branch(group.mode, branch, branch_width, secondary_width, age_width)
        }));
    }

    if lines.is_empty() {
        return vec![String::from(
            "No branches found for selected cleanup modes.",
        )];
    }

    lines
}

fn format_preview_title(
    group_name: &str,
    group_description: &str,
    step_index: usize,
    step_count: usize,
    total_width: usize,
) -> String {
    let left = format!("  git-broom   [{group_name}: {group_description}]");
    let right = format!("({step_index}/{step_count})");
    let spacer_width = total_width.saturating_sub(left.chars().count() + right.chars().count());

    format!("{left}{}{right}", " ".repeat(spacer_width.max(1)))
}

fn format_preview_header(
    mode: CleanupMode,
    branch_width: usize,
    secondary_width: usize,
    age_width: usize,
) -> String {
    let secondary_label = secondary_column_label(mode);
    format!(
        "  {}  {}  {}",
        pad("branch name", branch_width),
        left_pad(secondary_label, secondary_width),
        left_pad("age", age_width),
    )
}

fn format_preview_rule(
    mode: CleanupMode,
    branch_width: usize,
    secondary_width: usize,
    age_width: usize,
) -> String {
    let secondary_label = secondary_column_label(mode);
    format!(
        "  {}  {}  {}",
        pad(
            "-".repeat("branch name".chars().count()).as_str(),
            branch_width
        ),
        left_pad(
            "-".repeat(secondary_label.chars().count()).as_str(),
            secondary_width
        ),
        left_pad("-".repeat("age".chars().count()).as_str(), age_width),
    )
}

fn format_preview_branch(
    mode: CleanupMode,
    branch: &Branch,
    branch_width: usize,
    secondary_width: usize,
    age_width: usize,
) -> Vec<String> {
    let secondary_value = secondary_column_value(branch, mode);
    let mut lines = vec![format!(
        "  {}  {}  {}",
        pad(&branch.display_name(), branch_width),
        left_pad(
            &truncate(&secondary_value, secondary_width),
            secondary_width
        ),
        left_pad(&branch.relative_date, age_width),
    )];

    if let Some(detail) = &branch.detail {
        lines.push(format!(
            "     {}",
            truncate(
                detail,
                preview_total_width(branch_width, secondary_width, age_width) - 5
            )
        ));
    }

    lines
}

fn preview_column_widths(mode: CleanupMode, available_width: usize) -> (usize, usize, usize) {
    let content_width = available_width.saturating_sub(2).max(40);
    column_widths(mode, content_width.saturating_sub(2))
}

fn preview_total_width(branch_width: usize, commit_width: usize, age_width: usize) -> usize {
    2 + branch_width + 2 + commit_width + 2 + age_width
}

fn preview_width() -> usize {
    if io::stdout().is_terminal() {
        size().map(|(width, _)| width as usize).unwrap_or(80)
    } else {
        80
    }
}

fn fit_for_column(value: &str, width: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= width {
        return value.to_string();
    }

    if width <= 3 {
        return ".".repeat(width);
    }

    let truncated = value.chars().take(width - 3).collect::<String>();
    format!("{truncated}...")
}

fn truncate(value: &str, width: usize) -> String {
    fit_for_column(value, width)
}

fn pad(value: &str, width: usize) -> String {
    let visible = value.chars().count();
    if visible >= width {
        return truncate(value, width);
    }

    let mut padded = value.to_string();
    padded.push_str(&" ".repeat(width - visible));
    padded
}

fn left_pad(value: &str, width: usize) -> String {
    let truncated = truncate(value, width);
    let visible = truncated.chars().count();
    if visible >= width {
        return truncated;
    }

    format!("{}{}", " ".repeat(width - visible), truncated)
}

fn secondary_column_label(mode: CleanupMode) -> &'static str {
    match mode {
        CleanupMode::Closed => "pull request",
        _ => "last commit",
    }
}

fn secondary_column_value(branch: &Branch, mode: CleanupMode) -> String {
    match mode {
        CleanupMode::Closed => branch
            .pr_url
            .clone()
            .unwrap_or_else(|| String::from("no PR")),
        _ => format!("\"{}\"", branch.subject),
    }
}

fn column_widths(mode: CleanupMode, width: usize) -> (usize, usize, usize) {
    let min_branch = 12;
    let min_secondary = 12;
    let max_age = 14;
    let age_width = width
        .saturating_sub(min_branch + min_secondary + 4)
        .min(max_age);
    let remaining = width.saturating_sub(age_width + 4);
    let preferred_branch = match mode {
        CleanupMode::Closed => remaining / 3,
        _ => remaining * 2 / 5,
    };
    let branch_width = preferred_branch
        .max(min_branch)
        .min(remaining.saturating_sub(min_secondary));
    let secondary_width = remaining.saturating_sub(branch_width).max(min_secondary);

    (branch_width, secondary_width, age_width)
}

fn usage_text() -> &'static str {
    r#"git-broom cleans up stale local branches in step-by-step review groups.

Usage:
  git-broom [gone] [unpushed] [closed] [--remote <name>] [--batch | --dry-run]

Cleanup modes:
  gone       Upstream branch no longer exists on the remote.
  unpushed   Local branch has no upstream tracking branch configured.
  closed     Remote-tracked branch has a closed or missing GitHub PR.
             This can expand into multiple review groups, such as:
             - closed: closed PR or no PR
             - merged: PR merged but remote branch still exists

How it works:
  - With no modes, git-broom reviews all implemented cleanup modes in order.
  - Interactive mode walks one group at a time and asks for confirmation
    before deleting that group's selected branches.
  - --dry-run and --batch print a readable preview of the same grouped flow
    without deleting anything.
  - Protected branches stay visible for context but cannot be deleted.

Options:
  --dry-run        Show the grouped preview without deleting anything.
  --batch          Print the same readable grouped preview without entering the TUI.
  --remote <name>  Remote to use for closed mode. Default: origin.
  -h, --help       Show this help text.

Examples:
  git-broom
      Review all cleanup modes in order.

  git-broom gone unpushed
      Only review gone and unpushed branches.

  git-broom closed --dry-run
      Preview closed/merged GitHub-backed cleanup groups.

  git-broom closed --remote upstream
      Use the upstream remote instead of origin for closed mode.
"#
}

fn print_usage() {
    println!("{}", usage_text());
}

struct ScanStatusLine {
    enabled: bool,
}

impl ScanStatusLine {
    fn new() -> Self {
        Self {
            enabled: io::stderr().is_terminal(),
        }
    }

    fn update(&mut self, stage: ScanProgress, detail: Option<&str>) {
        if !self.enabled {
            return;
        }

        let detail_suffix = detail
            .map(|value| format!(": {}", fit_for_column(value, 36)))
            .unwrap_or_default();
        let trailing = if detail_suffix.ends_with("...") {
            ""
        } else {
            "..."
        };
        eprint!(
            "\r\x1b[2Kgit-broom: [{}/{}] {}{}{}",
            stage.step(),
            ScanProgress::TOTAL_STEPS,
            stage.message(),
            detail_suffix,
            trailing
        );
        let _ = io::stderr().flush();
    }

    fn finish(&mut self) {
        if !self.enabled {
            return;
        }

        eprint!("\r\x1b[2K");
        let _ = io::stderr().flush();
        self.enabled = false;
    }
}

impl Drop for ScanStatusLine {
    fn drop(&mut self) {
        self.finish();
    }
}

type PanicHook = dyn Fn(&panic::PanicHookInfo<'_>) + Sync + Send + 'static;

struct TerminalGuard {
    previous_hook: Arc<Mutex<Option<Box<PanicHook>>>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        let previous_hook = Arc::new(Mutex::new(Some(panic::take_hook())));
        let hook_for_panic = Arc::clone(&previous_hook);

        panic::set_hook(Box::new(move |panic_info| {
            restore_terminal();
            if let Some(previous) = hook_for_panic
                .lock()
                .expect("panic hook mutex poisoned")
                .as_ref()
            {
                previous(panic_info);
            }
        }));

        Ok(Self { previous_hook })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
        if let Some(previous_hook) = self
            .previous_hook
            .lock()
            .expect("panic hook mutex poisoned")
            .take()
        {
            panic::set_hook(previous_hook);
        }
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use git_broom::app::{Branch, CleanupGroup, CleanupMode, Decision};

    use super::{OutputMode, fit_for_column, format_preview_lines, is_immediate_exit, parse_cli};

    fn sample_branch(name: &str) -> Branch {
        Branch {
            name: name.to_string(),
            upstream: Some(format!("origin/{name}")),
            upstream_track: "[gone]".to_string(),
            committed_at: 1_700_000_000,
            relative_date: "2 days ago".to_string(),
            subject: "subject line".to_string(),
            pr_url: None,
            detail: None,
            decision: Decision::Undecided,
            protections: Vec::new(),
        }
    }

    #[test]
    fn parse_cli_defaults_to_all_modes() {
        let cli = parse_cli(std::iter::empty()).expect("cli parses");

        assert_eq!(
            cli.modes,
            vec![
                CleanupMode::Gone,
                CleanupMode::Unpushed,
                CleanupMode::Closed
            ]
        );
        assert_eq!(cli.output, OutputMode::Interactive);
        assert_eq!(cli.remote, "origin");
    }

    #[test]
    fn parse_cli_accepts_multiple_modes_with_dry_run() {
        let cli = parse_cli(
            ["gone", "unpushed", "--dry-run"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli parses");

        assert_eq!(cli.modes, vec![CleanupMode::Gone, CleanupMode::Unpushed]);
        assert_eq!(cli.output, OutputMode::DryRun);
        assert_eq!(cli.remote, "origin");
    }

    #[test]
    fn parse_cli_accepts_remote_for_closed_mode() {
        let cli = parse_cli(
            ["closed", "--remote", "upstream"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli parses");

        assert_eq!(cli.modes, vec![CleanupMode::Closed]);
        assert_eq!(cli.output, OutputMode::Interactive);
        assert_eq!(cli.remote, "upstream");
    }

    #[test]
    fn fit_for_column_truncates_long_values() {
        assert_eq!(
            fit_for_column("feature/some-very-long-branch-name", 12),
            "feature/s..."
        );
    }

    #[test]
    fn format_preview_lines_groups_branches_by_mode() {
        let lines = format_preview_lines(
            &[
                CleanupGroup::from_mode(
                    CleanupMode::Gone,
                    vec![sample_branch("feature/delete-me")],
                ),
                CleanupGroup::from_mode(
                    CleanupMode::Unpushed,
                    vec![sample_branch("feature/local-only")],
                ),
            ],
            80,
        );

        assert!(lines[0].starts_with("  git-broom   [gone: upstream branch no longer exists]"));
        assert!(lines[0].ends_with("(1/2)"));
        assert!(lines[1].contains("branch name"));
        assert!(lines[2].contains("-----------"));
        assert!(lines[3].contains("feature/delete-me"));
        assert!(
            lines[5]
                .starts_with("  git-broom   [unpushed: no upstream tracking branch is configured]")
        );
        assert!(lines[5].ends_with("(2/2)"));
        assert!(lines[6].contains("branch name"));
        assert!(lines[7].contains("-----------"));
        assert!(lines[8].contains("feature/local-only"));
    }

    #[test]
    fn format_preview_lines_uses_pull_request_column_for_closed_mode() {
        let mut branch = sample_branch("feature/closed");
        branch.pr_url = Some(String::from("https://example.test/pr/1"));

        let lines = format_preview_lines(
            &[CleanupGroup::named(
                CleanupMode::Closed,
                "closed",
                "closed pull request or no pull request on GitHub",
                vec![branch],
            )],
            120,
        );

        assert!(lines[1].contains("pull request"));
        assert!(lines[3].contains("https://example.test/pr/1"));
    }

    #[test]
    fn ctrl_c_and_ctrl_d_exit_immediately() {
        assert!(is_immediate_exit(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
        assert!(is_immediate_exit(KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL,
        )));
        assert!(!is_immediate_exit(KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        )));
    }
}
