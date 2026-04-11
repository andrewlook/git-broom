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
use git_broom::app::{App, DeleteResult, delete_branches};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let mode = parse_mode(env::args().skip(1))?;
    let repo = env::current_dir()?;

    match mode {
        Mode::Interactive => run_interactive(&repo),
        Mode::Batch => run_batch(&repo),
        Mode::DryRun => run_dry_run(&repo),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Interactive,
    Batch,
    DryRun,
}

fn parse_mode(args: impl Iterator<Item = String>) -> Result<Mode> {
    let args = args.collect::<Vec<_>>();
    match args.as_slice() {
        [] => Ok(Mode::Interactive),
        [flag] if flag == "--batch" => Ok(Mode::Batch),
        [flag] if flag == "--dry-run" => Ok(Mode::DryRun),
        _ => bail!("usage: git-broom [--batch | --dry-run]"),
    }
}

fn run_interactive(repo: &Path) -> Result<()> {
    println!("Scanning branches...");
    let mut app = App::load(repo)?;

    if app.is_empty() {
        println!("No gone branches found.");
        return Ok(());
    }

    match run_tui(&mut app)? {
        ExitAction::Quit => {
            println!("No branches deleted.");
            Ok(())
        }
        ExitAction::Confirm => {
            let branches_to_delete = app
                .delete_candidates()
                .into_iter()
                .map(|branch| branch.name.clone())
                .collect::<Vec<_>>();

            if branches_to_delete.is_empty() {
                println!("No branches selected for deletion.");
                return Ok(());
            }

            println!("About to delete {} branches:", branches_to_delete.len());
            for branch in &branches_to_delete {
                println!("  {branch}");
            }

            if !prompt_for_confirmation()? {
                println!("Aborted.");
                return Ok(());
            }

            print_delete_results(&delete_branches(repo, &branches_to_delete));
            Ok(())
        }
    }
}

fn run_batch(repo: &Path) -> Result<()> {
    let app = App::load(repo)?;
    let branches = app
        .deletable_branches()
        .into_iter()
        .map(|branch| branch.name.as_str())
        .collect::<Vec<_>>();

    if branches.is_empty() {
        eprintln!("No deletable gone branches found.");
        return Ok(());
    }

    for branch in branches {
        println!("{branch}");
    }

    Ok(())
}

fn run_dry_run(repo: &Path) -> Result<()> {
    let app = App::load(repo)?;
    if app.is_empty() {
        println!("No gone branches found.");
        return Ok(());
    }

    for branch in app.branches {
        let status = if branch.is_protected() {
            "keep"
        } else {
            "candidate"
        };
        let upstream = branch.upstream.as_deref().unwrap_or("-");
        println!(
            "{status}\t{}\t{}\t{}\t{}",
            branch.display_name(),
            upstream,
            branch.relative_date,
            branch.subject
        );
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

            match key.code {
                KeyCode::Char('j') | KeyCode::Down => app.next(),
                KeyCode::Char('k') | KeyCode::Up => app.previous(),
                KeyCode::Char('d') => app.mark_delete(),
                KeyCode::Char('s') => app.mark_keep(),
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

fn print_delete_results(results: &[DeleteResult]) {
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

    println!("Deleted {deleted} branches. {failed} failed.");
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
