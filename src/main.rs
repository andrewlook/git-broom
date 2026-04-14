use std::env;
use std::io::{self, IsTerminal, Write};
use std::panic;
use std::path::Path;
use std::process;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::style::Stylize;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
};
use git_broom::app::{
    App, AppScreen, Branch, CleanupGroup, CleanupMode, DEFAULT_CLEAN_GROUPS,
    DEFAULT_PREVIEW_GROUPS, ScanOptions, ScanOutcome, ScanProgress, delete_branch,
    scan_with_options,
};
use git_broom::keep_store::KeepStore;
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
    let outcome = match cli.intent {
        CliIntent::Preview => scan_with_options(
            &repo,
            ScanOptions::preview(&cli.modes, &cli.remote),
            |_, _| {},
        )?,
        CliIntent::Clean if cli.modes.iter().copied().any(CleanupMode::uses_pr_metadata) => {
            let mut status = ScanStatusLine::new();
            let outcome = scan_with_options(
                &repo,
                ScanOptions::clean(&cli.modes, &cli.remote),
                |stage, detail| {
                    status.update(stage, detail);
                },
            )?;
            status.finish();
            outcome
        }
        CliIntent::Clean => scan_with_options(
            &repo,
            ScanOptions::clean(&cli.modes, &cli.remote),
            |_, _| {},
        )?,
    };

    match cli.intent {
        CliIntent::Preview => run_preview(&outcome),
        CliIntent::Clean => {
            print_scan_notes(&outcome.notes);
            run_interactive(&repo, &cli.remote, outcome.groups)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliIntent {
    Preview,
    Clean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    modes: Vec<CleanupMode>,
    intent: CliIntent,
    remote: String,
}

fn parse_cli(args: impl Iterator<Item = String>) -> Result<CliOptions> {
    let mut modes = Vec::new();
    let mut remote = String::from("origin");
    let mut args = args.peekable();
    let intent = if matches!(args.peek().map(String::as_str), Some("clean")) {
        args.next();
        CliIntent::Clean
    } else {
        CliIntent::Preview
    };
    let mut saw_preview_alias = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--batch" | "--dry-run" => saw_preview_alias = true,
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            "-g" | "--groups" => {
                let Some(value) = args.next() else {
                    bail!(
                        "{} requires a comma-separated value\n\n{}",
                        arg,
                        usage_text()
                    );
                };
                for mode in parse_groups_value(&value)? {
                    if !modes.contains(&mode) {
                        modes.push(mode);
                    }
                }
            }
            "--remote" => {
                let Some(value) = args.next() else {
                    bail!("--remote requires a value\n\n{}", usage_text());
                };
                remote = value;
            }
            value => {
                bail!(
                    "unknown argument `{value}`. Use -g/--groups to choose cleanup groups.\n\n{}",
                    usage_text()
                );
            }
        }
    }

    if modes.is_empty() {
        modes = match intent {
            CliIntent::Preview => DEFAULT_PREVIEW_GROUPS.to_vec(),
            CliIntent::Clean => DEFAULT_CLEAN_GROUPS.to_vec(),
        };
    }

    if intent == CliIntent::Clean && saw_preview_alias {
        bail!(
            "`git-broom clean` is destructive. Remove `--dry-run` / `--batch`, or run `git-broom` without `clean` to preview groups.\n\n{}",
            usage_text()
        );
    }

    if intent == CliIntent::Clean && modes.iter().any(|mode| !mode.is_cleanable()) {
        bail!(
            "`pr` is a preview-only group. Remove it from `git-broom clean`, or run `git-broom --groups pr` to browse open-PR branches.\n\n{}",
            usage_text()
        );
    }

    Ok(CliOptions {
        modes,
        intent,
        remote,
    })
}

fn parse_groups_value(value: &str) -> Result<Vec<CleanupMode>> {
    let mut modes = Vec::new();

    for raw in value.split(',') {
        let group = raw.trim();
        if group.is_empty() {
            bail!("--groups cannot contain empty group names");
        }

        let Some(mode) = CleanupMode::from_arg(group) else {
            bail!("unknown cleanup group `{group}`");
        };

        if !modes.contains(&mode) {
            modes.push(mode);
        }
    }

    if modes.is_empty() {
        bail!("--groups requires at least one cleanup group");
    }

    Ok(modes)
}

fn run_interactive(repo: &Path, remote: &str, groups: Vec<CleanupGroup>) -> Result<()> {
    if groups.iter().all(|group| group.branches.is_empty()) {
        println!("No branches found for selected cleanup groups.");
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

        let mut app = App::from_group(group, remote, index + 1, group_count);
        match run_tui(repo, &mut app)? {
            ExitAction::Quit => {
                persist_saved_branches(repo, &app)?;
                println!("Aborted.");
                return Ok(());
            }
            ExitAction::Skip => {
                persist_saved_branches(repo, &app)?;
                println!("No {} branches selected for deletion.", app.group_name);
            }
            ExitAction::Completed { deleted, failed } => {
                persist_saved_branches(repo, &app)?;
                total_deleted += deleted;
                total_failed += failed;
                if failed > 0 {
                    println!(
                        "Workflow complete. Deleted {total_deleted} branches. {total_failed} failed."
                    );
                    return Ok(());
                }
            }
        }
    }

    println!("Workflow complete. Deleted {total_deleted} branches. {total_failed} failed.");
    Ok(())
}

fn run_preview(outcome: &ScanOutcome) -> Result<()> {
    if !outcome.groups.is_empty() {
        for line in format_preview_lines(&outcome.groups, preview_width()) {
            println!("{line}");
        }
    } else if outcome.notes.is_empty() {
        println!("No branches found for selected cleanup groups.");
    }

    if !outcome.notes.is_empty() && !outcome.groups.is_empty() {
        println!();
    }
    print_scan_notes(&outcome.notes);

    Ok(())
}

fn print_scan_notes(notes: &[String]) {
    for note in notes {
        if stdout_supports_styling() {
            println!("{} {}", "Note:".yellow().bold(), note.as_str().dark_grey());
        } else {
            println!("Note: {note}");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitAction {
    Quit,
    Skip,
    Completed { deleted: usize, failed: usize },
}

fn run_tui(repo: &Path, app: &mut App) -> Result<ExitAction> {
    struct ExecutionWorker {
        index: usize,
        receiver: Receiver<git_broom::app::DeleteResult>,
    }

    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut deleted = 0;
    let mut failed = 0;
    let mut worker: Option<ExecutionWorker> = None;

    loop {
        terminal.draw(|frame| git_broom::ui::render(frame, app))?;

        if matches!(app.screen, AppScreen::Executing(_)) && app.execution_failure().is_none() {
            if let Some(active) = &worker {
                match active.receiver.try_recv() {
                    Ok(result) => {
                        let index = active.index;
                        worker = None;
                        if result.success {
                            app.mark_execution_result(index, true);
                            deleted += 1;
                            continue;
                        }

                        app.mark_execution_result(index, false);
                        app.mark_execution_skipped_from(index + 1);
                        app.set_execution_failure(index, result.output);
                        failed += 1;
                        continue;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        let index = active.index;
                        worker = None;
                        app.mark_execution_result(index, false);
                        app.mark_execution_skipped_from(index + 1);
                        app.set_execution_failure(index, "cleanup command worker disconnected");
                        failed += 1;
                        continue;
                    }
                }
            } else {
                let Some(index) = app.next_pending_execution_index() else {
                    return Ok(ExitAction::Completed { deleted, failed });
                };
                let Some(branch) = app.execution_branch(index).cloned() else {
                    return Ok(ExitAction::Completed { deleted, failed });
                };

                app.start_execution(index);
                let repo = repo.to_path_buf();
                let remote = app.remote.clone();
                let mode = app.mode;
                let (sender, receiver) = mpsc::channel();
                thread::spawn(move || {
                    let result = delete_branch(&repo, mode, &remote, &branch);
                    let _ = sender.send(result);
                });
                worker = Some(ExecutionWorker { index, receiver });
            }
        }

        let next_event =
            if matches!(app.screen, AppScreen::Executing(_)) && app.execution_failure().is_none() {
                if event::poll(Duration::from_millis(80))? {
                    Some(event::read()?)
                } else {
                    if app.execution_running_index().is_some() {
                        app.advance_execution_spinner();
                    }
                    None
                }
            } else {
                Some(event::read()?)
            };

        let Some(Event::Key(key)) = next_event else {
            continue;
        };

        {
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

            match &app.screen {
                AppScreen::Triage => match key.code {
                    KeyCode::Char('j') | KeyCode::Down => app.next(),
                    KeyCode::Char('k') | KeyCode::Up => app.previous(),
                    KeyCode::Char('d') => app.toggle_delete(),
                    KeyCode::Char('s') => app.toggle_save(),
                    KeyCode::Char('a') => app.mark_all_delete(),
                    KeyCode::Char('u') => app.unmark_all(),
                    KeyCode::Enter => {
                        if app.enter_review() {
                            continue;
                        }
                        return Ok(ExitAction::Skip);
                    }
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(ExitAction::Quit),
                    _ => {}
                },
                AppScreen::Review(_) => match key.code {
                    KeyCode::Char('y') => app.begin_execution(),
                    KeyCode::Char('n') => app.exit_review(),
                    KeyCode::Enter => app.require_review_confirmation(),
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(ExitAction::Quit),
                    _ => {}
                },
                AppScreen::Executing(_) => {
                    if app.execution_failure().is_some() && matches!(key.code, KeyCode::Enter) {
                        return Ok(ExitAction::Completed { deleted, failed });
                    }
                }
            }
        }
    }
}

fn is_immediate_exit(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'))
}

fn persist_saved_branches(repo: &Path, app: &App) -> Result<()> {
    let mut keep_store = KeepStore::load(repo)?;
    keep_store.replace_mode(app.mode, app.saved_branch_names())
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

        if lines.is_empty() {
            lines.push(String::new());
        }

        step_index += 1;
        let (branch_width, secondary_width) =
            preview_column_widths(group.mode, available_width, &group.branches);
        lines.push(format_preview_header(group, step_index, step_count));
        let mut previous_section = None;
        for branch in &group.branches {
            if previous_section.is_some() && previous_section != Some(branch.section()) {
                lines.push(String::new());
            }
            lines.extend(format_preview_branch(
                group.mode,
                branch,
                branch_width,
                secondary_width,
            ));
            previous_section = Some(branch.section());
        }
    }

    if lines.is_empty() {
        return vec![String::from(
            "No branches found for selected cleanup groups.",
        )];
    }

    lines
}

fn format_preview_header(group: &CleanupGroup, step_index: usize, step_count: usize) -> String {
    format_group_header(&group.name, &group.description, step_index, step_count)
}

fn format_preview_branch(
    mode: CleanupMode,
    branch: &Branch,
    branch_width: usize,
    secondary_width: usize,
) -> Vec<String> {
    let secondary_value = secondary_column_value(branch, mode);
    let compact_age = compact_age_display(branch.committed_at);
    let branch_name_width = branch_width.saturating_sub(compact_age.chars().count() + 1);
    let branch_value = pad(&branch.display_name(), branch_name_width);
    let secondary_padded = left_pad(
        &truncate(&secondary_value, secondary_width),
        secondary_width,
    );
    let mut lines = vec![format!(
        "  {} {}  {}",
        style_branch_preview(branch, &branch_value),
        style_age_preview(&compact_age),
        style_secondary_preview(branch, mode, &secondary_padded),
    )];

    if let Some(detail) = &branch.detail {
        lines.push(format!(
            "     {}",
            truncate(
                detail,
                preview_total_width(branch_width, secondary_width) - 5
            )
        ));
    }

    lines
}

fn preview_column_widths(
    mode: CleanupMode,
    available_width: usize,
    branches: &[Branch],
) -> (usize, usize) {
    let content_width = available_width.saturating_sub(2).max(40);
    column_widths(mode, content_width.saturating_sub(2), branches)
}

fn preview_total_width(branch_width: usize, commit_width: usize) -> usize {
    2 + branch_width + 2 + commit_width
}

fn preview_width() -> usize {
    if stdout_supports_styling() {
        size().map(|(width, _)| width as usize).unwrap_or(80)
    } else {
        80
    }
}

fn format_group_header(
    group_name: &str,
    group_description: &str,
    _step_index: usize,
    _step_count: usize,
) -> String {
    let plain = format!("{group_name} ({group_description})");
    if !stdout_supports_styling() {
        return plain;
    }

    let explanation_color = group_header_color(group_name);
    format!(
        "{} {}",
        group_name
            .to_ascii_uppercase()
            .as_str()
            .bold()
            .with(group_header_color(group_name)),
        format!("({group_description})")
            .with(explanation_color)
            .dim(),
    )
}

fn style_branch_preview(branch: &Branch, value: &str) -> String {
    if !stdout_supports_styling() {
        return value.to_string();
    }

    match branch.section() {
        git_broom::app::BranchSection::Protected => value.to_string(),
        git_broom::app::BranchSection::Saved => format!("{}", value.green()),
        git_broom::app::BranchSection::Regular => value.to_string(),
    }
}

fn style_secondary_preview(branch: &Branch, mode: CleanupMode, value: &str) -> String {
    if !stdout_supports_styling() {
        return value.to_string();
    }

    match mode {
        mode if mode.uses_pr_metadata() && branch.pr_url.is_some() => {
            let trimmed = value.trim_start();
            let padding = " ".repeat(
                value
                    .chars()
                    .count()
                    .saturating_sub(trimmed.chars().count()),
            );
            format!("{padding}{}", trimmed.blue().underlined())
        }
        mode if mode.uses_pr_metadata() => format!("{}", value.dark_grey()),
        _ => match branch.section() {
            git_broom::app::BranchSection::Protected => format!("{}", value.dark_grey().italic()),
            git_broom::app::BranchSection::Saved => format!("{}", value.green().italic()),
            git_broom::app::BranchSection::Regular => format!("{}", value.dark_grey().italic()),
        },
    }
}

fn style_age_preview(value: &str) -> String {
    if !stdout_supports_styling() {
        return value.to_string();
    }

    format!("{}", value.dark_grey())
}

fn stdout_supports_styling() -> bool {
    !cfg!(test) && io::stdout().is_terminal()
}

fn group_header_color(group_name: &str) -> crossterm::style::Color {
    match group_name {
        "gone" => crossterm::style::Color::Red,
        "unpushed" => crossterm::style::Color::Yellow,
        "PR" => crossterm::style::Color::Cyan,
        "No PR" => crossterm::style::Color::DarkYellow,
        "closed" => crossterm::style::Color::Blue,
        "merged" => crossterm::style::Color::Green,
        _ => crossterm::style::Color::White,
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

fn secondary_column_value(branch: &Branch, mode: CleanupMode) -> String {
    if mode.uses_pr_metadata() {
        branch
            .pr_url
            .clone()
            .unwrap_or_else(|| String::from("no PR"))
    } else {
        format!("\"{}\"", truncate_commit_subject(&branch.subject))
    }
}

fn truncate_commit_subject(subject: &str) -> String {
    fit_for_column(subject, 50)
}

fn column_widths(mode: CleanupMode, width: usize, branches: &[Branch]) -> (usize, usize) {
    let min_branch = 12;
    let min_secondary = 12;
    let remaining = width.saturating_sub(2);
    let max_branch = branches
        .iter()
        .map(|branch| {
            branch.display_name().chars().count()
                + 1
                + compact_age_display(branch.committed_at).chars().count()
        })
        .max()
        .unwrap_or(min_branch);
    let max_secondary = branches
        .iter()
        .map(|branch| secondary_column_value(branch, mode).chars().count())
        .max()
        .unwrap_or(min_secondary);

    let mut branch_width = max_branch.max(min_branch);
    let mut secondary_width = max_secondary.max(min_secondary);
    let total = branch_width + 2 + secondary_width;
    if total > width {
        let overflow = total - width;
        let branch_reduction = overflow.min(branch_width.saturating_sub(min_branch));
        branch_width -= branch_reduction;
    }

    branch_width = branch_width.min(remaining.saturating_sub(min_secondary));
    secondary_width = remaining.saturating_sub(branch_width).max(min_secondary);

    (branch_width, secondary_width)
}

fn compact_age_display(committed_at: i64) -> String {
    let age_seconds = current_unix_timestamp().saturating_sub(committed_at).max(0) as u64;
    if age_seconds < 60 {
        return String::from("now");
    }

    let minute = 60;
    let hour = 60 * minute;
    let day = 24 * hour;
    let week = 7 * day;
    let month = 30 * day;

    if age_seconds < hour {
        return format!("{}m", age_seconds / minute);
    }
    if age_seconds < day {
        return format!("{}h", age_seconds / hour);
    }
    if age_seconds < week {
        return format!("{}d", age_seconds / day);
    }
    if age_seconds < month {
        return format!("{}w", age_seconds / week);
    }
    format!("{}mo", age_seconds / month)
}

fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn usage_text() -> &'static str {
    r#"git-broom shows grouped local-branch inventory by default, then cleans branches only when you ask it to.

Usage:
  git-broom [-g <gone,unpushed,pr,nopr,closed,merged>] [--remote <name>] [--batch | --dry-run]
  git-broom clean [-g <gone,unpushed,nopr,closed,merged>] [--remote <name>]

Cleanup groups:
  gone       Upstream branch no longer exists on the remote.
  unpushed   Local branch has no upstream tracking branch configured.
  pr         Remote-tracked branch has an open pull request on GitHub.
  nopr       Remote-tracked branch has no pull request on GitHub.
  closed     Remote-tracked branch has a closed pull request on GitHub.
  merged     Remote-tracked branch has a merged pull request whose branch still exists.

How it works:
  - `git-broom` previews all selected groups without deleting anything.
  - `git-broom clean` enters the step-by-step destructive review flow.
  - Preview defaults include `pr` so the grouped view covers open-PR branches too.
  - `pr` is preview-only. `git-broom clean` rejects it.
  - GitHub-backed groups reuse cached PR metadata when it is fresh.
    `git-broom clean` refreshes GitHub data before any destructive review.
  - `--dry-run` and `--batch` are compatibility aliases for the same default
    grouped preview output.
  - Protected branches stay visible for context but cannot be deleted.
  - Press s in the TUI to save or unsave a branch for this repo and mode.
    Saved branches stay visible but are excluded from delete-all until unsaved.

Options:
  -g, --groups    Comma-separated groups to show or clean. Default: all for each mode.
  --dry-run        Compatibility alias for the default grouped preview.
  --batch          Compatibility alias for the default grouped preview.
  --remote <name>  Remote to use for GitHub-backed groups. Default: origin.
  -h, --help       Show this help text.

Examples:
  git-broom
      Preview all implemented review groups.

  git-broom --groups gone,pr
      Preview only gone and open-PR groups.

  git-broom clean
      Review all groups interactively and confirm deletions per group.

  git-broom clean --groups gone,nopr
      Only clean gone and no-PR branches.

  git-broom --groups pr,closed,merged --remote upstream
      Preview GitHub-backed groups using the upstream remote.
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

    use super::{
        CliIntent, fit_for_column, format_preview_lines, is_immediate_exit, parse_cli,
        parse_groups_value, truncate_commit_subject,
    };

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
            saved: false,
            decision: Decision::Undecided,
            protections: Vec::new(),
        }
    }

    #[test]
    fn parse_cli_defaults_to_preview_all_modes() {
        let cli = parse_cli(std::iter::empty()).expect("cli parses");

        assert_eq!(
            cli.modes,
            vec![
                CleanupMode::Gone,
                CleanupMode::Unpushed,
                CleanupMode::Pr,
                CleanupMode::NoPr,
                CleanupMode::Closed,
                CleanupMode::Merged,
            ]
        );
        assert_eq!(cli.intent, CliIntent::Preview);
        assert_eq!(cli.remote, "origin");
    }

    #[test]
    fn parse_cli_accepts_multiple_modes_for_preview_alias() {
        let cli = parse_cli(
            ["--groups", "gone,unpushed", "--dry-run"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli parses");

        assert_eq!(cli.modes, vec![CleanupMode::Gone, CleanupMode::Unpushed]);
        assert_eq!(cli.intent, CliIntent::Preview);
        assert_eq!(cli.remote, "origin");
    }

    #[test]
    fn parse_cli_accepts_remote_for_clean_closed_mode() {
        let cli = parse_cli(
            ["clean", "--groups", "closed", "--remote", "upstream"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli parses");

        assert_eq!(cli.modes, vec![CleanupMode::Closed]);
        assert_eq!(cli.intent, CliIntent::Clean);
        assert_eq!(cli.remote, "upstream");
    }

    #[test]
    fn parse_cli_rejects_clean_with_preview_alias() {
        let error = parse_cli(["clean", "--dry-run"].into_iter().map(str::to_string))
            .expect_err("clean preview alias rejected");

        assert!(
            error
                .to_string()
                .contains("`git-broom clean` is destructive")
        );
    }

    #[test]
    fn parse_groups_value_accepts_comma_separated_groups() {
        let groups =
            parse_groups_value("gone, pr,nopr,closed,merged,unpushed").expect("groups parse");

        assert_eq!(
            groups,
            vec![
                CleanupMode::Gone,
                CleanupMode::Pr,
                CleanupMode::NoPr,
                CleanupMode::Closed,
                CleanupMode::Merged,
                CleanupMode::Unpushed
            ]
        );
    }

    #[test]
    fn parse_cli_rejects_pr_group_for_clean() {
        let error = parse_cli(["clean", "--groups", "pr"].into_iter().map(str::to_string))
            .expect_err("pr rejected for clean");

        assert!(error.to_string().contains("`pr` is a preview-only group"));
    }

    #[test]
    fn parse_cli_rejects_positional_group_arguments() {
        let error =
            parse_cli(["gone"].into_iter().map(str::to_string)).expect_err("positional rejected");

        assert!(
            error
                .to_string()
                .contains("Use -g/--groups to choose cleanup groups")
        );
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
            200,
        );

        let first_header = lines[1].to_lowercase();
        let second_header = lines[3].to_lowercase();

        assert_eq!(lines[0], "");
        assert!(first_header.contains("gone (upstream branch no longer exists)"));
        assert!(lines[2].contains("feature/delete-me"));
        assert!(second_header.contains("unpushed (no upstream tracking branch is configured)"));
        assert!(lines[4].contains("feature/local-only"));
    }

    #[test]
    fn format_preview_lines_separates_saved_rows() {
        let mut protected = sample_branch("feature/current");
        protected.protections = vec![git_broom::app::Protection::Current];

        let mut saved = sample_branch("feature/saved");
        saved.saved = true;

        let regular = sample_branch("feature/regular");

        let lines = format_preview_lines(
            &[CleanupGroup::from_mode(
                CleanupMode::Gone,
                vec![protected, saved, regular],
            )],
            120,
        );

        let saved_index = lines
            .iter()
            .position(|line| line.contains("feature/saved"))
            .expect("saved branch rendered");
        let regular_index = lines
            .iter()
            .position(|line| line.contains("feature/regular"))
            .expect("regular branch rendered");

        assert_eq!(lines[saved_index - 1], "");
        assert_eq!(lines[regular_index - 1], "");
    }

    #[test]
    fn format_preview_lines_uses_pull_request_column_for_closed_mode() {
        let mut branch = sample_branch("feature/closed");
        branch.pr_url = Some(String::from("https://example.test/pr/1"));

        let lines = format_preview_lines(
            &[CleanupGroup::named(
                CleanupMode::Closed,
                "closed",
                "pull request closed on GitHub",
                vec![branch],
            )],
            200,
        );

        assert!(
            lines[1]
                .to_lowercase()
                .contains("closed (pull request closed on github)")
        );
        assert!(lines[2].contains("https://example.test/pr/1"));
    }

    #[test]
    fn format_preview_lines_caps_non_pr_commit_subjects() {
        let mut branch = sample_branch("feature/local-only");
        branch.subject = String::from(
            "this is a very long commit subject that should be capped before layout expansion happens",
        );

        let lines = format_preview_lines(
            &[CleanupGroup::from_mode(CleanupMode::Unpushed, vec![branch])],
            200,
        );

        assert!(lines[2].contains(&format!(
            "\"{}\"",
            truncate_commit_subject(
                "this is a very long commit subject that should be capped before layout expansion happens"
            )
        )));
        assert!(!lines[2].contains("layout expansion happens"));
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
