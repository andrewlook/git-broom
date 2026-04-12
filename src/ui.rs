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

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(
                format!("git-broom (step {}/{}): ", app.step_index, app.step_count),
                Style::default(),
            ),
            Span::styled(
                format!("{}: ", app.mode.name()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.mode.description(), Style::default().fg(Color::Gray)),
        ]))
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
        .map(ListItem::new)
        .collect::<Vec<_>>();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    let mut state = ListState::default();
    if !app.is_empty() {
        state.select(Some(app.selected));
    }

    frame.render_stateful_widget(list, content[1], &mut state);

    let footer = Paragraph::new(
        "j/k move  d toggle delete  a all  u clear  Enter review  q/ctrl-c/ctrl-d quit",
    );
    frame.render_widget(footer, chunks[1]);

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
    let (branch_width, commit_width, age_width) = column_widths(width.saturating_sub(4));
    Line::from(vec![
        Span::raw(pad("branch name", branch_width)),
        Span::raw("  "),
        Span::raw(left_pad("last commit", commit_width)),
        Span::raw("  "),
        Span::raw(left_pad("age", age_width)),
    ])
}

fn render_branch(branch: &Branch, width: usize) -> Line<'static> {
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

    let spans = vec![
        Span::styled(format!("{} ", marker.0), marker.1),
        Span::styled(pad(&branch.display_name(), branch_width), line_style),
        Span::styled("  ", line_style),
        Span::styled(
            left_pad(
                &format!("\"{}\"", truncate(&branch.subject, commit_width)),
                commit_width,
            ),
            line_style
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
        Span::styled("  ", line_style),
        Span::styled(left_pad(&branch.relative_date, age_width), line_style),
    ];

    Line::from(spans)
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
