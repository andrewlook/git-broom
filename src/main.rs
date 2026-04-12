use std::env;
use std::io::{self, Write};
use std::panic;
use std::path::Path;
use std::process;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use git_broom::app::{
    App, Branch, CleanupMode, DeleteResult, IMPLEMENTED_MODES, Tranche, delete_branches,
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
    let tranches = scan_selected_modes(&repo, &cli.modes)?;

    match cli.output {
        OutputMode::Interactive => run_interactive(&repo, tranches),
        OutputMode::Batch => run_batch(&tranches),
        OutputMode::DryRun => run_dry_run(&tranches),
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

fn run_interactive(repo: &Path, tranches: Vec<Tranche>) -> Result<()> {
    if tranches.iter().all(|tranche| tranche.branches.is_empty()) {
        println!("No branches found for selected cleanup modes.");
        return Ok(());
    }

    let tranche_count = tranches.len();
    let mut total_deleted = 0;
    let mut total_failed = 0;

    for (index, tranche) in tranches.into_iter().enumerate() {
        if tranche.branches.is_empty() {
            println!("{}", tranche.mode.no_matches_message());
            continue;
        }

        let mut app = App::from_tranche(tranche, index + 1, tranche_count);
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

fn run_batch(tranches: &[Tranche]) -> Result<()> {
    let branches = tranches
        .iter()
        .flat_map(|tranche| tranche.branches.iter())
        .filter(|branch| branch.is_deletable())
        .map(|branch| branch.name.as_str())
        .collect::<Vec<_>>();

    if branches.is_empty() {
        eprintln!("No deletable branches found for selected cleanup modes.");
        return Ok(());
    }

    for branch in branches {
        println!("{branch}");
    }

    Ok(())
}

fn run_dry_run(tranches: &[Tranche]) -> Result<()> {
    for line in format_dry_run_lines(tranches) {
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

fn format_dry_run_lines(tranches: &[Tranche]) -> Vec<String> {
    let mut lines = Vec::new();

    for tranche in tranches {
        let deletable = tranche
            .branches
            .iter()
            .filter(|branch| branch.is_deletable())
            .collect::<Vec<_>>();

        if deletable.is_empty() {
            continue;
        }

        if !lines.is_empty() {
            lines.push(String::new());
        }

        lines.push(format!(
            "{} ({})",
            tranche.mode.name(),
            tranche.mode.description()
        ));
        lines.extend(deletable.into_iter().map(format_dry_run_branch));
    }

    if lines.is_empty() {
        return vec![String::from(
            "No deletable branches found for selected cleanup modes.",
        )];
    }

    lines
}

fn format_dry_run_branch(branch: &Branch) -> String {
    format!(
        "  {:<40} ({})",
        fit_for_column(&branch.name, 40),
        branch.relative_date
    )
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
    use git_broom::app::{Branch, CleanupMode, Decision, Tranche};

    use super::{OutputMode, fit_for_column, format_dry_run_lines, parse_cli};

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
    fn format_dry_run_lines_groups_branches_by_mode() {
        let lines = format_dry_run_lines(&[
            Tranche {
                mode: CleanupMode::Gone,
                branches: vec![sample_branch("feature/delete-me")],
            },
            Tranche {
                mode: CleanupMode::Unpushed,
                branches: vec![sample_branch("feature/local-only")],
            },
        ]);

        assert_eq!(lines[0], "gone (upstream branch no longer exists)");
        assert!(lines[1].contains("feature/delete-me"));
        assert_eq!(
            lines[3],
            "unpushed (no upstream tracking branch is configured)"
        );
        assert!(lines[4].contains("feature/local-only"));
    }
}
