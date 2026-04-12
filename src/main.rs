use std::env;
use std::io::{self, Write};
use std::panic;
use std::path::Path;
use std::process;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use git_broom::app::{
    App, Branch, CleanupGroup, CleanupMode, DeleteResult, IMPLEMENTED_MODES, delete_branches,
    scan_selected_modes,
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
    let groups = scan_selected_modes(&repo, &cli.modes)?;

    match cli.output {
        OutputMode::Interactive => run_interactive(&repo, groups),
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
}

fn parse_cli(args: impl Iterator<Item = String>) -> Result<CliOptions> {
    let mut modes = Vec::new();
    let mut output = OutputMode::Interactive;

    for arg in args {
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

    Ok(CliOptions { modes, output })
}

fn run_interactive(repo: &Path, groups: Vec<CleanupGroup>) -> Result<()> {
    if groups.iter().all(|group| group.branches.is_empty()) {
        println!("No branches found for selected cleanup modes.");
        return Ok(());
    }

    let group_count = groups.len();
    let mut total_deleted = 0;
    let mut total_failed = 0;

    for (index, group) in groups.into_iter().enumerate() {
        if group.branches.is_empty() {
            println!("{}", group.mode.no_matches_message());
            continue;
        }

        let mut app = App::from_group(group, index + 1, group_count);
        match run_tui(&mut app)? {
            ExitAction::Quit => {
                println!("Aborted.");
                return Ok(());
            }
            ExitAction::Confirm => {
                let branches_to_delete = app
                    .delete_candidates()
                    .into_iter()
                    .map(|branch| branch.name.clone())
                    .collect::<Vec<_>>();

                if branches_to_delete.is_empty() {
                    println!("No {} branches selected for deletion.", app.mode.name());
                    continue;
                }

                println!(
                    "About to delete {} {} branches ({}):",
                    branches_to_delete.len(),
                    app.mode.name(),
                    app.mode.description()
                );
                for branch in &branches_to_delete {
                    println!("  {branch}");
                }

                if !prompt_for_confirmation()? {
                    println!("Skipped {}.", app.mode.name());
                    continue;
                }

                let (deleted, failed) =
                    print_delete_results(app.mode, &delete_branches(repo, &branches_to_delete));
                total_deleted += deleted;
                total_failed += failed;
            }
        }
    }

    println!("Workflow complete. Deleted {total_deleted} branches. {total_failed} failed.");
    Ok(())
}

fn run_batch(groups: &[CleanupGroup]) -> Result<()> {
    for line in format_preview_lines(groups) {
        println!("{line}");
    }

    Ok(())
}

fn run_dry_run(groups: &[CleanupGroup]) -> Result<()> {
    for line in format_preview_lines(groups) {
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

fn print_delete_results(mode: CleanupMode, results: &[DeleteResult]) -> (usize, usize) {
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

    println!(
        "{}: deleted {deleted} branches. {failed} failed.",
        mode.name()
    );

    (deleted, failed)
}

fn format_preview_lines(groups: &[CleanupGroup]) -> Vec<String> {
    let mut lines = Vec::new();
    let (branch_width, commit_width, age_width) = preview_column_widths();
    let total_width = preview_total_width(branch_width, commit_width, age_width);
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
        lines.push(format_preview_title(
            group.mode,
            step_index,
            step_count,
            total_width,
        ));
        lines.push(format_preview_header(branch_width, commit_width, age_width));
        lines.push(format_preview_rule(branch_width, commit_width, age_width));
        lines.extend(
            group
                .branches
                .iter()
                .map(|branch| format_preview_branch(branch, branch_width, commit_width, age_width)),
        );
    }

    if lines.is_empty() {
        return vec![String::from(
            "No branches found for selected cleanup modes.",
        )];
    }

    lines
}

fn format_preview_title(
    mode: CleanupMode,
    step_index: usize,
    step_count: usize,
    total_width: usize,
) -> String {
    let left = format!("  git-broom   [{}: {}]", mode.name(), mode.description());
    let right = format!("({step_index}/{step_count})");
    let spacer_width = total_width.saturating_sub(left.chars().count() + right.chars().count());

    format!("{left}{}{right}", " ".repeat(spacer_width.max(1)))
}

fn format_preview_header(branch_width: usize, commit_width: usize, age_width: usize) -> String {
    format!(
        "  {}  {}  {}",
        pad("branch name", branch_width),
        left_pad("last commit", commit_width),
        left_pad("age", age_width),
    )
}

fn format_preview_rule(branch_width: usize, commit_width: usize, age_width: usize) -> String {
    format!(
        "  {}  {}  {}",
        pad(
            "-".repeat("branch name".chars().count()).as_str(),
            branch_width
        ),
        left_pad(
            "-".repeat("last commit".chars().count()).as_str(),
            commit_width
        ),
        left_pad("-".repeat("age".chars().count()).as_str(), age_width),
    )
}

fn format_preview_branch(
    branch: &Branch,
    branch_width: usize,
    commit_width: usize,
    age_width: usize,
) -> String {
    format!(
        "  {}  {}  {}",
        pad(&branch.display_name(), branch_width),
        left_pad(
            &format!("\"{}\"", truncate(&branch.subject, commit_width)),
            commit_width,
        ),
        left_pad(&branch.relative_date, age_width),
    )
}

fn preview_column_widths() -> (usize, usize, usize) {
    (38, 28, 14)
}

fn preview_total_width(branch_width: usize, commit_width: usize, age_width: usize) -> usize {
    2 + branch_width + 2 + commit_width + 2 + age_width
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

fn usage_text() -> &'static str {
    "usage: git-broom [gone] [unpushed] [--batch | --dry-run]"
}

fn print_usage() {
    println!("{}", usage_text());
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
            relative_date: "2 days ago".to_string(),
            subject: "subject line".to_string(),
            decision: Decision::Undecided,
            protections: Vec::new(),
        }
    }

    #[test]
    fn parse_cli_defaults_to_all_modes() {
        let cli = parse_cli(std::iter::empty()).expect("cli parses");

        assert_eq!(cli.modes, vec![CleanupMode::Gone, CleanupMode::Unpushed]);
        assert_eq!(cli.output, OutputMode::Interactive);
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
        let lines = format_preview_lines(&[
            CleanupGroup {
                mode: CleanupMode::Gone,
                branches: vec![sample_branch("feature/delete-me")],
            },
            CleanupGroup {
                mode: CleanupMode::Unpushed,
                branches: vec![sample_branch("feature/local-only")],
            },
        ]);

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
