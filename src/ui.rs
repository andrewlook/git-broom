use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Branch, Decision};

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

    let content = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let header = Paragraph::new(render_header(content[0].width as usize))
        .style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(header, content[0]);

    let items = app
        .branches
        .iter()
        .map(|branch| render_branch(branch, content[1].width.saturating_sub(3) as usize))
        .collect::<Vec<_>>();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    let mut state = ListState::default();
    if !app.is_empty() {
        state.select(Some(app.selected));
    }

    frame.render_stateful_widget(list, content[1], &mut state);

    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(28)])
        .split(chunks[1]);

    let footer_left = Paragraph::new(Line::from(vec![
        key_hint("j / k"),
        desc_hint(" (up / down)  "),
        key_hint("d"),
        desc_hint(" (delete)  "),
        key_hint("a"),
        desc_hint(" (delete all)  "),
        key_hint("u"),
        desc_hint(" (clear)  "),
        key_hint("q"),
        desc_hint(" (quit)"),
    ]));
    frame.render_widget(footer_left, footer_chunks[0]);

    let footer_right = Paragraph::new(Line::from(vec![
        key_hint("enter"),
        desc_hint(" (review deletions)"),
    ]))
    .alignment(Alignment::Right);
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

fn render_header(width: usize) -> Line<'static> {
    let row_prefix_width = 5;
    let (branch_width, commit_width, age_width) =
        column_widths(width.saturating_sub(row_prefix_width));
    let mut spans = vec![
        Span::raw(" ".repeat(row_prefix_width)),
        Span::styled(
            "branch name",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
        ),
        Span::raw(" ".repeat(branch_width.saturating_sub("branch name".chars().count()))),
        Span::raw("  "),
    ];
    spans.extend(right_aligned_header("last commit", commit_width));
    spans.push(Span::raw("  "));
    spans.extend(right_aligned_header("age", age_width));

    Line::from(spans)
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

fn right_aligned_header(label: &'static str, width: usize) -> Vec<Span<'static>> {
    let padding = width.saturating_sub(label.chars().count());
    vec![
        Span::raw(" ".repeat(padding)),
        Span::styled(
            label,
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
        ),
    ]
}

fn render_branch(branch: &Branch, width: usize) -> ListItem<'static> {
    let marker = match branch.decision {
        Decision::Delete => ("✗", Style::default().fg(Color::Red)),
        Decision::Undecided => ("·", Style::default().fg(Color::DarkGray)),
    };
    let (branch_width, commit_width, age_width) = column_widths(width.saturating_sub(6));

    let mut line_style = Style::default();
    if branch.decision == Decision::Delete {
        line_style = line_style.add_modifier(Modifier::CROSSED_OUT);
    }
    if branch.is_protected() {
        line_style = line_style.fg(Color::DarkGray);
    }

    let primary = Line::from(vec![
        Span::styled(format!("{} ", marker.0), marker.1),
        Span::styled(pad(&branch.display_name(), branch_width), line_style),
        Span::raw("  "),
        Span::styled(
            left_pad(
                &format!("\"{}\"", truncate(&branch.subject, commit_width)),
                commit_width,
            ),
            line_style
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
        Span::raw("  "),
        Span::styled(left_pad(&branch.relative_date, age_width), line_style),
    ]);

    if let Some(detail) = &branch.detail {
        let detail_width = width.saturating_sub(5);
        let detail_style = line_style
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);
        return ListItem::new(vec![
            primary,
            Line::from(vec![
                Span::raw("     "),
                Span::styled(truncate(detail, detail_width), detail_style),
            ]),
        ]);
    }

    ListItem::new(vec![primary])
}

fn column_widths(width: usize) -> (usize, usize, usize) {
    let age_width = 14;
    let commit_width = 28.min(width.saturating_sub(age_width + 6));
    let branch_width = width.saturating_sub(commit_width + age_width + 4);
    (branch_width.max(12), commit_width.max(12), age_width)
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
