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
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                format!("Tranche {} of {}: ", app.step_index, app.step_count),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                app.mode.name(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(app.mode.description()),
    ]);
    frame.render_widget(info, chunks[0]);

    let items = app
        .branches
        .iter()
        .map(render_branch)
        .map(ListItem::new)
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(
                    " git-broom: {} selected of {} {} branches ",
                    app.delete_count(),
                    app.branches.len(),
                    app.mode.name(),
                ))
                .borders(Borders::ALL),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    let mut state = ListState::default();
    if !app.is_empty() {
        state.select(Some(app.selected));
    }

    frame.render_stateful_widget(list, chunks[1], &mut state);

    let footer =
        Paragraph::new("j/k: move  d: toggle delete  a: all  u: clear  Enter: review  q: quit");
    frame.render_widget(footer, chunks[2]);

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

fn render_branch(branch: &Branch) -> Line<'static> {
    let marker = match branch.decision {
        Decision::Delete => ("✗", Style::default().fg(Color::Red)),
        Decision::Undecided => ("·", Style::default().fg(Color::DarkGray)),
    };

    let mut line_style = Style::default();
    if branch.decision == Decision::Delete {
        line_style = line_style.add_modifier(Modifier::CROSSED_OUT);
    }
    if branch.is_protected() {
        line_style = line_style.fg(Color::DarkGray);
    }

    let spans = vec![
        Span::styled(format!("{} ", marker.0), marker.1),
        Span::styled(pad(&branch.display_name(), 38), line_style),
        Span::styled(
            pad(&branch.relative_date, 12),
            line_style.fg(Color::DarkGray),
        ),
        Span::styled(format!("\"{}\"", truncate(&branch.subject, 32)), line_style),
    ];

    Line::from(spans)
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
