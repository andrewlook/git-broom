use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::{
    App, AppScreen, Branch, CleanupMode, CommandLineState, CommandPlanItem, Decision,
};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let title_width = chunks[0].width.saturating_sub(2) as usize;
    let block = Block::default()
        .title(render_title(app, title_width))
        .borders(Borders::ALL);
    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);

    match &app.screen {
        AppScreen::Triage => {
            let body_area = inner;

            let items = app
                .branches
                .iter()
                .enumerate()
                .map(|(index, branch)| {
                    let next_section = app.branches.get(index + 1).map(Branch::section);
                    render_branch(
                        app,
                        branch,
                        next_section,
                        body_area.width.saturating_sub(3) as usize,
                    )
                })
                .collect::<Vec<_>>();

            let list = List::new(items)
                .highlight_style(Style::default().add_modifier(Modifier::BOLD))
                .highlight_symbol(">> ");

            let mut state = ListState::default();
            if !app.is_empty() {
                state.select(Some(app.selected));
            }

            frame.render_stateful_widget(list, body_area, &mut state);
        }
        AppScreen::Review(review) => {
            let content = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(inner);
            render_review(frame, app, review, content[0], content[1]);
        }
        AppScreen::Executing(execution) => {
            let content = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(inner);
            render_execution(frame, app, execution, content[0], content[1]);
        }
    }

    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(28)])
        .split(chunks[1]);

    let footer_left = Paragraph::new(render_footer_left(app));
    frame.render_widget(footer_left, footer_chunks[0]);

    let footer_right = Paragraph::new(render_footer_right(app)).alignment(Alignment::Right);
    frame.render_widget(footer_right, footer_chunks[1]);

    if let Some(modal) = &app.modal {
        let area = centered_rect(72, 26, frame.area());
        frame.render_widget(Clear, area);
        let dialog = Paragraph::new(modal.message.as_str())
            .block(Block::default().title(modal.title).borders(Borders::ALL))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(dialog, area);
    }
}

fn render_review(
    frame: &mut Frame<'_>,
    app: &App,
    review: &crate::app::ReviewState,
    header_area: Rect,
    body_area: Rect,
) {
    let summary =
        Paragraph::new(render_review_summary(app, review.items.len())).wrap(Wrap { trim: true });
    frame.render_widget(summary, header_area);

    let items = review
        .items
        .iter()
        .map(render_review_command)
        .collect::<Vec<_>>();
    frame.render_widget(List::new(items), body_area);
}

fn render_execution(
    frame: &mut Frame<'_>,
    _app: &App,
    execution: &crate::app::ExecutionState,
    header_area: Rect,
    body_area: Rect,
) {
    let summary = Paragraph::new(Line::from(vec![Span::styled(
        "Executing cleanup commands...",
        Style::default().add_modifier(Modifier::BOLD),
    )]));
    frame.render_widget(summary, header_area);

    let items = execution
        .items
        .iter()
        .map(render_execution_command)
        .collect::<Vec<_>>();
    frame.render_widget(List::new(items), body_area);
}

fn render_review_summary(app: &App, count: usize) -> Line<'static> {
    let noun = if count == 1 { "branch" } else { "branches" };
    Line::from(vec![
        Span::raw("About to run cleanup commands for "),
        Span::styled(
            format!("{count} {} {noun}", app.group_name),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("({})", app.group_description),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
        Span::raw(":"),
    ])
}

fn render_review_command(item: &CommandPlanItem) -> ListItem<'static> {
    let mut spans = vec![Span::raw("  ")];
    if let Some(remote_command) = &item.remote_command {
        spans.push(Span::styled(
            remote_command.clone(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" && ", Style::default().fg(Color::DarkGray)));
    }
    spans.push(Span::styled(
        item.local_command.clone(),
        Style::default().fg(Color::Yellow),
    ));

    ListItem::new(Line::from(spans))
}

fn render_execution_command(item: &CommandPlanItem) -> ListItem<'static> {
    let (prefix, command_style) = match item.state {
        CommandLineState::Pending => ("  ", Style::default()),
        CommandLineState::Success => (
            "✓ ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::CROSSED_OUT),
        ),
        CommandLineState::Failed => ("x ", Style::default().fg(Color::Red)),
        CommandLineState::Skipped => ("- ", Style::default().fg(Color::DarkGray)),
    };

    let prefix_style = match item.state {
        CommandLineState::Success => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        CommandLineState::Failed => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        CommandLineState::Skipped => Style::default().fg(Color::DarkGray),
        CommandLineState::Pending => Style::default(),
    };

    ListItem::new(Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(item.plain_command(), command_style),
    ]))
}

fn render_footer_left(app: &App) -> Line<'static> {
    match &app.screen {
        AppScreen::Triage => Line::from(vec![
            key_hint("j / k"),
            desc_hint(" (up / down)  "),
            key_hint("d"),
            desc_hint(" (delete)  "),
            key_hint("s"),
            desc_hint(" (save)  "),
            key_hint("a"),
            desc_hint(" (delete all)  "),
            key_hint("u"),
            desc_hint(" (clear deletions)  "),
            key_hint("q"),
            desc_hint(" (quit)"),
        ]),
        AppScreen::Review(_) => Line::from(vec![
            key_hint("y"),
            desc_hint(" (confirm)  "),
            key_hint("n"),
            desc_hint(" (back)  "),
            key_hint("q"),
            desc_hint(" (quit)"),
        ]),
        AppScreen::Executing(_) => Line::from(vec![desc_hint("running cleanup commands...")]),
    }
}

fn render_footer_right(app: &App) -> Line<'static> {
    match &app.screen {
        AppScreen::Triage => Line::from(vec![key_hint("enter"), desc_hint(" (review deletions)")]),
        AppScreen::Review(review) if review.require_explicit_choice => {
            Line::from(vec![Span::styled(
                "y or n required",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )])
        }
        AppScreen::Review(_) => Line::from(vec![key_hint("y / n"), desc_hint(" (confirm / back)")]),
        AppScreen::Executing(_) => Line::from(vec![]),
    }
}

fn render_title(app: &App, width: usize) -> Line<'static> {
    let left_segments = [
        "  ".len(),
        "git-broom".len(),
        "   ".len(),
        "[".len(),
        app.group_name.len(),
        ": ".len(),
        app.group_description.len(),
        "]".len(),
    ];
    let left_width = left_segments.into_iter().sum::<usize>();
    let right_text = format!("({}/{})", app.step_index, app.step_count);
    let spacer_width = width.saturating_sub(left_width + right_text.chars().count());

    Line::from(vec![
        Span::raw("  "),
        Span::styled("git-broom", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled(
            app.group_name.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(": "),
        Span::styled(
            app.group_description.clone(),
            Style::default().fg(Color::Gray),
        ),
        Span::styled("]", Style::default().fg(Color::DarkGray)),
        Span::raw(" ".repeat(spacer_width)),
        Span::styled(right_text, Style::default().fg(Color::Gray)),
    ])
}

fn render_branch(
    app: &App,
    branch: &Branch,
    next_section: Option<crate::app::BranchSection>,
    width: usize,
) -> ListItem<'static> {
    let marker = match branch.decision {
        Decision::Delete => ("✗", Style::default().fg(Color::Red)),
        Decision::Undecided => ("·", Style::default().fg(Color::DarkGray)),
    };
    let compact_age = compact_age_display(branch.committed_at);
    let (branch_width, secondary_width) = column_widths(
        app.mode,
        width.saturating_sub(4),
        &branch.display_name(),
        &compact_age,
    );

    let mut line_style = Style::default();
    if branch.decision == Decision::Delete {
        line_style = line_style.add_modifier(Modifier::CROSSED_OUT);
    }
    if branch.is_protected() {
        line_style = line_style.fg(Color::DarkGray);
    } else if branch.saved {
        line_style = line_style.fg(Color::Green);
    }
    let secondary_value = secondary_column_value(branch, app.mode);
    let secondary_style = if app.mode.uses_pr_metadata() {
        if branch.is_protected() {
            line_style.fg(Color::DarkGray)
        } else if branch.saved {
            line_style.fg(Color::Green)
        } else {
            line_style.fg(Color::Cyan)
        }
    } else {
        line_style
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC)
    };

    let branch_name_width = branch_width.saturating_sub(compact_age.chars().count() + 1);
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{} ", marker.0), marker.1),
        Span::styled(pad(&branch.display_name(), branch_name_width), line_style),
        Span::raw(" "),
        Span::styled(compact_age, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(
            left_pad(
                &truncate(&secondary_value, secondary_width),
                secondary_width,
            ),
            secondary_style,
        ),
    ])];

    if let Some(detail) = &branch.detail {
        let detail_width = width.saturating_sub(5);
        let detail_style = line_style
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(vec![
            Span::raw("     "),
            Span::styled(truncate(detail, detail_width), detail_style),
        ]));
    }

    if next_section.is_some() && next_section != Some(branch.section()) {
        lines.push(Line::from(""));
    }

    ListItem::new(lines)
}

fn secondary_column_value(branch: &Branch, mode: CleanupMode) -> String {
    if mode.uses_pr_metadata() {
        branch
            .pr_url
            .clone()
            .unwrap_or_else(|| String::from("no PR"))
    } else {
        format!("\"{}\"", branch.subject)
    }
}

fn column_widths(
    mode: CleanupMode,
    width: usize,
    branch_label: &str,
    compact_age: &str,
) -> (usize, usize) {
    let min_branch = 12;
    let min_secondary = 12;
    let branch_width = (branch_label.chars().count() + 1 + compact_age.chars().count())
        .max(min_branch)
        .min(width.saturating_sub(min_secondary + 2));
    let secondary_width = width.saturating_sub(branch_width + 2).max(min_secondary);

    let _ = mode;
    (branch_width, secondary_width)
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

fn truncate(value: &str, width: usize) -> String {
    let visible = value.chars().count();
    if visible <= width {
        return value.to_string();
    }

    value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
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

fn centered_rect(horizontal_percent: u16, vertical_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - vertical_percent) / 2),
            Constraint::Percentage(vertical_percent),
            Constraint::Percentage((100 - vertical_percent) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - horizontal_percent) / 2),
            Constraint::Percentage(horizontal_percent),
            Constraint::Percentage((100 - horizontal_percent) / 2),
        ])
        .split(vertical[1])[1]
}

fn key_hint(text: &'static str) -> Span<'static> {
    Span::styled(text, Style::default().add_modifier(Modifier::BOLD))
}

fn desc_hint(text: &'static str) -> Span<'static> {
    Span::styled(text, Style::default().fg(Color::DarkGray))
}
